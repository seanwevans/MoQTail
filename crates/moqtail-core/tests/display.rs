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
fn compile_errors_on_unclosed_predicate() {
    let result = compile("/foo[bar=1");
    assert!(result.is_err());
}
