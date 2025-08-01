use moqtail_core::{compile, Matcher, Message};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn avg_pipeline() {
    let sel = compile("/sensor |> window(60s) |> avg(json$.value)").unwrap();
    let mut m = Matcher::new(sel);

    let headers = HashMap::new();

    let msg1 = Message {
        topic: "sensor",
        headers: headers.clone(),
        payload: Some(json!({"value": 10})),
    };
    assert_eq!(m.process(&msg1), Some(10.0));

    let msg2 = Message {
        topic: "sensor",
        headers: headers.clone(),
        payload: Some(json!({"value": 20})),
    };
    assert_eq!(m.process(&msg2), Some(15.0));
}
