use moqtail_core::{compile, Matcher, Message};
use serde_json::json;
use std::borrow::Cow;
use std::collections::HashMap;

#[test]
fn header_predicate_match() {
    let sel = compile("/msg[qos<=1]").unwrap();
    let msg = Message {
        topic: "",
        headers: HashMap::from([(Cow::Borrowed("qos"), Cow::Borrowed("0"))]),
        payload: None,
    };
    let m = Matcher::new(sel);
    assert!(m.matches(&msg));
}

#[test]
fn json_predicate_match() {
    let sel = compile("/foo[json$.temp>30]").unwrap();
    let payload = json!({"temp": 35});
    let msg = Message {
        topic: "foo",
        headers: HashMap::new(),
        payload: Some(payload),
    };
    let m = Matcher::new(sel);
    assert!(m.matches(&msg));
}

#[test]

fn header_predicate_negative_fractional() {
    let sel = compile("/msg[temp<=-1.5]").unwrap();
    let msg = Message {
        topic: "",
        headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("-1.5"))]),
        payload: None,
    };
    let m = Matcher::new(sel);
    assert!(m.matches(&msg));
}

#[test]
fn json_predicate_fractional() {
    let sel = compile("/foo[json$.temp>=32.5]").unwrap();
    let payload = json!({"temp": 33.1});
    let msg = Message {
        topic: "foo",
        headers: HashMap::new(),
        payload: Some(payload),
    };
    let m = Matcher::new(sel);
    assert!(m.matches(&msg));
}

#[test]
fn json_predicate_missing_field() {
    assert!(compile(" /foo[json$>1]").is_err());
}
