use moqtail_core::{ast::Stage, compile, Error, Matcher, Message};
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// Pipeline stages now operate on time-based windows. These tests exercise the
// eviction behaviour by advancing the synthetic timestamps passed to
// [`Matcher::process`].
#[test]
fn avg_pipeline() {
    let sel = compile("/sensor |> window(60s) |> avg(json$.value)").unwrap();
    let mut m = Matcher::new(sel);

    let headers = HashMap::new();
    let start = Instant::now();

    let msg1 = Message {
        topic: "sensor",
        headers: headers.clone(),
        payload: Some(json!({"value": 10})),
    };
    assert_eq!(m.process(&msg1, start), Some(10.0));

    let msg2 = Message {
        topic: "sensor",
        headers: headers.clone(),
        payload: Some(json!({"value": 20})),
    };
    assert_eq!(
        m.process(&msg2, start + Duration::from_secs(30)),
        Some(15.0)
    );

    let msg3 = Message {
        topic: "sensor",
        headers,
        payload: Some(json!({"value": 30})),
    };
    // The first reading is now older than the 60s window, so it should be dropped.
    assert_eq!(
        m.process(&msg3, start + Duration::from_secs(75)),
        Some(25.0)
    );
}

#[test]
fn sum_pipeline() {
    let sel = compile("/sensor |> window(60s) |> sum(json$.value)").unwrap();
    let mut m = Matcher::new(sel);

    let headers = HashMap::new();
    let start = Instant::now();

    let msg1 = Message {
        topic: "sensor",
        headers: headers.clone(),
        payload: Some(json!({"value": 10})),
    };
    assert_eq!(m.process(&msg1, start), Some(10.0));

    let msg2 = Message {
        topic: "sensor",
        headers: headers.clone(),
        payload: Some(json!({"value": 20})),
    };
    assert_eq!(
        m.process(&msg2, start + Duration::from_secs(30)),
        Some(30.0)
    );

    let msg3 = Message {
        topic: "sensor",
        headers,
        payload: Some(json!({"value": 40})),
    };
    assert_eq!(
        m.process(&msg3, start + Duration::from_secs(75)),
        Some(60.0)
    );
}

#[test]
fn sum_pipeline_large_unsigned() {
    let sel = compile("/sensor |> window(60s) |> sum(json$.value)").unwrap();
    let mut m = Matcher::new(sel);

    let headers = HashMap::new();
    let start = Instant::now();

    let msg = Message {
        topic: "sensor",
        headers,
        payload: Some(json!({"value": u64::MAX})),
    };
    assert_eq!(m.process(&msg, start), Some(u64::MAX as f64));
}

#[test]
fn count_pipeline() {
    let sel = compile("/sensor |> window(60s) |> count()").unwrap();
    let mut m = Matcher::new(sel);

    let headers = HashMap::new();
    let start = Instant::now();

    let msg1 = Message {
        topic: "sensor",
        headers: headers.clone(),
        payload: None,
    };
    assert_eq!(m.process(&msg1, start), Some(1.0));

    let msg2 = Message {
        topic: "sensor",
        headers: headers.clone(),
        payload: None,
    };
    assert_eq!(m.process(&msg2, start + Duration::from_secs(30)), Some(2.0));

    let msg3 = Message {
        topic: "sensor",
        headers,
        payload: None,
    };
    // Only the two most recent events fall in the trailing 60s window.
    assert_eq!(m.process(&msg3, start + Duration::from_secs(90)), Some(2.0));
}

#[test]
fn sum_missing_field() {
    let sel = compile("/sensor |> window(60s) |> sum(json$.value)").unwrap();
    let mut m = Matcher::new(sel);

    let msg = Message {
        topic: "sensor",
        headers: HashMap::new(),
        payload: Some(json!({"other": 10})),
    };
    assert_eq!(m.process(&msg, Instant::now()), None);
}

#[test]
fn window_minutes_and_hours_parse() {
    let minutes = compile("/sensor |> window(5m)").unwrap();
    assert_eq!(
        minutes.stages.as_slice(),
        [Stage::Window(Duration::from_secs(300))]
    );

    let hours = compile("/sensor |> window(1h)").unwrap();
    assert_eq!(
        hours.stages.as_slice(),
        [Stage::Window(Duration::from_secs(3600))]
    );
}

#[test]
fn sum_requires_field_argument() {
    assert!(matches!(
        compile("/foo |> sum(1s)"),
        Err(moqtail_core::Error::SumRequiresField)
    ));
}

#[test]
fn avg_requires_field_argument() {
    assert!(matches!(
        compile("/foo |> avg(window(5s))"),
        Err(moqtail_core::Error::AvgRequiresField)
    ));
}

#[test]
fn count_rejects_arguments() {
    assert!(matches!(
        compile("/foo |> count(temp)"),
        Err(Error::CountDoesNotAcceptArguments)
    ));
}
