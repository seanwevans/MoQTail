use moqtail_core::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Step, Value};
use moqtail_core::compile;

#[test]
fn selector_display_roundtrip() {
    let input = "/foo/+//bar/#";
    let selector = compile(input).expect("should compile");
    assert_eq!(selector.to_string(), input);
}

#[test]
fn selector_display_with_predicate() {
    let selector = compile("/foo[bar=1]").unwrap();
    assert_eq!(selector.to_string(), "/foo[bar=1]");
}

#[test]
fn selector_display_with_quoted_string() {
    let selector = Selector {
        steps: vec![Step {
            axis: Axis::Child,
            segment: Segment::Literal("foo".into()),
            predicates: vec![Predicate {
                field: Field::Header("bar".into()),
                op: Operator::Eq,
                value: Value::Str("qu\"ote".into()),
            }],
        }],
        stages: vec![],
    };
    let display = selector.to_string();
    let reparsed = compile(&display).unwrap();
    assert_eq!(reparsed, selector);
}

#[test]
fn selector_display_with_backslash() {
    let selector = Selector {
        steps: vec![Step {
            axis: Axis::Child,
            segment: Segment::Literal("foo".into()),
            predicates: vec![Predicate {
                field: Field::Header("bar".into()),
                op: Operator::Eq,
                value: Value::Str("a\\b".into()),
            }],
        }],
        stages: vec![],
    };
    let display = selector.to_string();
    let reparsed = compile(&display).unwrap();
    assert_eq!(reparsed, selector);
}

#[test]
fn compile_errors_on_unclosed_predicate() {
    let result = compile("/foo[bar=1");
    assert!(result.is_err());
}
