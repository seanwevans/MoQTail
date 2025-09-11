use std::collections::HashMap;

use moqtail_core::{compile, Matcher, Message};

#[test]
fn matches_trailing_slash() {
    let selector = compile("/foo/bar").unwrap();
    let matcher = Matcher::new(selector);

    let msg = Message {
        topic: "foo/bar/",
        headers: HashMap::new(),
        payload: None,
    };

    assert!(matcher.matches(&msg));
}

#[test]
fn matches_repeated_slashes() {
    let selector = compile("/foo/bar").unwrap();
    let matcher = Matcher::new(selector);

    let msg = Message {
        topic: "foo//bar",
        headers: HashMap::new(),
        payload: None,
    };

    assert!(matcher.matches(&msg));
}

#[test]
fn trailing_slash_does_not_match_plus() {
    let selector = compile("/foo/+").unwrap();
    let matcher = Matcher::new(selector);

    let msg = Message {
        topic: "foo/",
        headers: HashMap::new(),
        payload: None,
    };

    assert!(!matcher.matches(&msg));
}
