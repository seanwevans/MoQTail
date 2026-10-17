use moqtail_core::{
    ast::{Axis, Field, Operator, Predicate, Segment, Selector, Stage, Step, Value},
    compile, Error,
};

#[test]
fn parse_selector_with_predicate() {
    let sel = compile("/foo[bar=1]").unwrap();
    assert_eq!(
        sel,
        Selector {
            steps: vec![Step {
                axis: Axis::Child,
                segment: Segment::Literal("foo".into()),
                predicates: vec![Predicate {
                    field: Field::Header("bar".into()),
                    op: Operator::Eq,
                    value: Value::Number(1.0)
                }],
            }],
            stages: vec![],
        }
    );
}

#[test]
fn parse_selector_with_wildcards() {
    let sel = compile("/foo/+/#").unwrap();
    assert_eq!(
        sel,
        Selector {
            steps: vec![
                Step {
                    axis: Axis::Child,
                    segment: Segment::Literal("foo".into()),
                    predicates: vec![],
                },
                Step {
                    axis: Axis::Child,
                    segment: Segment::Plus,
                    predicates: vec![],
                },
                Step {
                    axis: Axis::Child,
                    segment: Segment::Hash,
                    predicates: vec![],
                },
            ],
            stages: vec![],
        }
    );
}

#[test]
fn parse_selector_descendant() {
    let sel = compile("//sensor/#").unwrap();
    assert_eq!(
        sel,
        Selector {
            steps: vec![
                Step {
                    axis: Axis::Descendant,
                    segment: Segment::Literal("sensor".into()),
                    predicates: vec![],
                },
                Step {
                    axis: Axis::Child,
                    segment: Segment::Hash,
                    predicates: vec![],
                },
            ],
            stages: vec![],
        }
    );
}

#[test]
fn error_on_trailing_slash() {
    assert!(compile("/foo/bar/").is_err());
}

#[test]
fn error_on_unclosed_predicate() {
    assert!(compile("/foo[bar=1").is_err());
}

#[test]
fn parse_header_axis() {
    let sel = compile("/msg[qos<=1]/foo").unwrap();
    assert_eq!(
        sel,
        Selector {
            steps: vec![
                Step {
                    axis: Axis::Child,
                    segment: Segment::Message,
                    predicates: vec![Predicate {
                        field: Field::Header("qos".into()),
                        op: Operator::Le,
                        value: Value::Number(1.0)
                    }],
                },
                Step {
                    axis: Axis::Child,
                    segment: Segment::Literal("foo".into()),
                    predicates: vec![],
                }
            ],
            stages: vec![],
        }
    );
}

#[test]
fn parse_json_predicate() {
    let sel = compile("/foo[json$.temp>30]").unwrap();
    assert_eq!(
        sel,
        Selector {
            steps: vec![Step {
                axis: Axis::Child,
                segment: Segment::Literal("foo".into()),
                predicates: vec![Predicate {
                    field: Field::Json(vec!["temp".into()]),
                    op: Operator::Gt,
                    value: Value::Number(30.0)
                }],
            }],
            stages: vec![],
        }
    );
}

#[test]
fn parse_negative_fractional_number() {
    let sel = compile("/foo[bar=-1.5]").unwrap();
    assert_eq!(
        sel,
        Selector {
            steps: vec![Step {
                axis: Axis::Child,
                segment: Segment::Literal("foo".into()),
                predicates: vec![Predicate {
                    field: Field::Header("bar".into()),
                    op: Operator::Eq,
                    value: Value::Number(-1.5)
                }],
            }],
            stages: vec![],
        }
    );
}

#[test]
fn error_on_malformed_json_prefix() {
    let err = compile("/foo[json.temp>30]").unwrap_err();
    assert!(matches!(err, Error::MissingField));
}

#[test]
fn selector_display_keeps_count_parentheses() {
    let sel = Selector {
        steps: vec![Step {
            axis: Axis::Child,
            segment: Segment::Literal("sensor".into()),
            predicates: vec![],
        }],
        stages: vec![Stage::Count],
    };

    assert_eq!(sel.to_string(), "/sensor |> count()");
}

#[test]
fn needs_payload_tracks_json_fields() {
    assert!(!compile("/msg[qos<=1]/a/+").unwrap().needs_payload());
    assert!(compile("//sensor[json$.v>1]").unwrap().needs_payload());
    assert!(compile("/a |> avg(json$.v)").unwrap().needs_payload());
    assert!(!compile("/a |> window(5s) |> sum(temp)")
        .unwrap()
        .needs_payload());
}

/// MQTT 3.1.1 section 4.7 filter matching, kept independent of the matcher.
fn mqtt_filter_matches(filter: &str, topic: &str) -> bool {
    let mut f = filter.split('/');
    let mut t = topic.split('/');
    loop {
        match (f.next(), t.next()) {
            (Some("#"), _) => return true,
            (Some("+"), Some(_)) => {}
            (Some(a), Some(b)) if a == b => {}
            (None, None) => return true,
            _ => return false,
        }
    }
}

#[test]
fn subscription_filter_admits_everything_the_selector_matches() {
    use moqtail_core::{Matcher, Message};
    use std::collections::HashMap;

    let mut state = 0xD1B5_4A32_D192_ED03_u64;
    let mut next = move |n: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % n
    };
    let segs = ["a", "b", "+", "#", "msg"];
    let mut matched = 0;
    for _ in 0..30_000 {
        let mut sel = String::new();
        for _ in 0..1 + next(4) {
            sel.push_str(if next(4) == 0 { "//" } else { "/" });
            sel.push_str(segs[next(5) as usize]);
        }
        let selector = compile(&sel).unwrap();
        let filter = selector.subscription_filter();
        let matcher = Matcher::new(selector);
        // MQTT topics are never empty.
        let levels: Vec<&str> = (0..1 + next(4))
            .map(|_| ["a", "b", ""][next(3) as usize])
            .collect();
        let topic = levels.join("/");
        let msg = Message {
            topic: &topic,
            headers: HashMap::new(),
            payload: None,
        };
        if matcher.matches(&msg) {
            matched += 1;
            assert!(
                mqtt_filter_matches(&filter, &topic),
                "{sel} matches {topic:?} but its filter {filter:?} does not"
            );
        }
    }
    assert!(matched > 1_000, "only {matched} matches exercised");
}

#[test]
fn subscription_filter_is_exact_for_plain_topic_selectors() {
    assert_eq!(compile("/a/+/b").unwrap().subscription_filter(), "a/+/b");
    assert_eq!(compile("/a/#/b").unwrap().subscription_filter(), "a/#");
    assert_eq!(compile("/msg[qos=1]").unwrap().subscription_filter(), "#");
}
