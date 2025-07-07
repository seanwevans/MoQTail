use moqtail_core::{
    ast::{Axis, Field, Operator, Predicate, Segment, Selector, Step, Value},
    compile,
};

#[test]
fn parse_selector_with_predicate() {
    let sel = compile("/foo[bar=1]").unwrap();
    assert_eq!(
        sel,
        Selector(vec![Step {
            axis: Axis::Child,
            segment: Segment::Literal("foo".into()),
            predicates: vec![Predicate {
                field: Field::Header("bar".into()),
                op: Operator::Eq,
                value: Value::Number(1)
            }],
        }])
    );
}

#[test]
fn parse_selector_with_wildcards() {
    let sel = compile("/foo/+/#").unwrap();
    assert_eq!(
        sel,
        Selector(vec![
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
        ])
    );
}

#[test]
fn parse_selector_descendant() {
    let sel = compile("//sensor/#").unwrap();
    assert_eq!(
        sel,
        Selector(vec![
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
        ])
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
        Selector(vec![
            Step {
                axis: Axis::Child,
                segment: Segment::Message,
                predicates: vec![Predicate {
                    field: Field::Header("qos".into()),
                    op: Operator::Le,
                    value: Value::Number(1)
                }],
            },
            Step {
                axis: Axis::Child,
                segment: Segment::Literal("foo".into()),
                predicates: vec![],
            }
        ])
    );
}

#[test]
fn parse_json_predicate() {
    let sel = compile("/foo[json$.temp>30]").unwrap();
    assert_eq!(
        sel,
        Selector(vec![Step {
            axis: Axis::Child,
            segment: Segment::Literal("foo".into()),
            predicates: vec![Predicate {
                field: Field::Json(vec!["temp".into()]),
                op: Operator::Gt,
                value: Value::Number(30)
            }],
        }])
    );
}
