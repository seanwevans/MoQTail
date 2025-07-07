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
