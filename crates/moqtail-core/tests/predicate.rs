use std::collections::HashMap;
use serde_json::json;
use moqtail_core::{compile, Matcher, Message};

#[test]
fn header_predicate_match() {
    let sel = compile("/msg[qos<=1]").unwrap();
    let msg = Message {
        topic: "",
        headers: HashMap::from([
            ("qos".to_string(), "0".to_string())
        ]),
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
