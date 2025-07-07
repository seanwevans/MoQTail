use moqtail_core::{
    ast::{Axis, Predicate, Segment, Selector, Step},
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
            predicates: vec![Predicate::Equals {
                name: "bar".into(),
                value: "1".into()
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
