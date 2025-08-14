use std::collections::HashMap;

use moqtail_core::{compile, Matcher, Message};

fn build_topic(len: usize, last: &str) -> String {
    let mut segs = Vec::with_capacity(len + 1);
    for i in 0..len {
        segs.push(format!("seg{}", i));
    }
    segs.push(last.to_string());
    segs.join("/")
}

#[test]
fn matches_descendant_in_long_topic() {
    let selector = compile("//sensor").unwrap();
    let matcher = Matcher::new(selector);
    let topic = build_topic(100, "sensor");
    let msg = Message {
        topic: &topic,
        headers: HashMap::new(),
        payload: None,
    };
    assert!(
        matcher.matches(&msg),
        "selector should match long topic: {}",
        topic
    );
}

#[test]
fn no_match_in_long_topic() {
    let selector = compile("//sensor").unwrap();
    let matcher = Matcher::new(selector);
    let topic = build_topic(100, "other");
    let msg = Message {
        topic: &topic,
        headers: HashMap::new(),
        payload: None,
    };
    assert!(!matcher.matches(&msg));
}
