use std::collections::HashMap;

use moqtail_core::{compile, Matcher, Message};

#[test]
fn trailing_empty_segment_requires_wildcard() {
    let selector = compile("/foo/bar").unwrap();
    let matcher = Matcher::new(selector);

    let msg = Message {
        topic: "foo/bar/",
        headers: HashMap::new(),
        payload: None,
    };

    assert!(!matcher.matches(&msg));
}

#[test]
fn literal_does_not_skip_empty_segments() {
    let selector = compile("/foo/bar").unwrap();
    let matcher = Matcher::new(selector);

    let msg = Message {
        topic: "foo//bar",
        headers: HashMap::new(),
        payload: None,
    };

    assert!(!matcher.matches(&msg));
}

#[test]
fn plus_matches_empty_segment() {
    let selector = compile("/foo/+").unwrap();
    let matcher = Matcher::new(selector);

    let msg = Message {
        topic: "foo/",
        headers: HashMap::new(),
        payload: None,
    };

    assert!(matcher.matches(&msg));
}
