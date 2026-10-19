use std::borrow::Cow;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use moqtail_core::{compile, Matcher, Message};

fn long_topic(len: usize) -> String {
    let mut segs = Vec::with_capacity(len + 1);
    for i in 0..len {
        segs.push(format!("seg{}", i));
    }
    segs.push("sensor".to_string());
    segs.join("/")
}

fn bench_descendant_long_topic(c: &mut Criterion) {
    let selector = compile("//sensor").unwrap();
    let matcher = Matcher::new(selector);
    let topic = long_topic(100);
    let msg = Message {
        topic: &topic,
        headers: HashMap::new(),
        payload: None,
    };

    c.bench_function("descendant_long_topic", |b| {
        b.iter(|| matcher.matches(black_box(&msg)));
    });
}

fn bench_window_sum(c: &mut Criterion) {
    let selector = compile("/sensor |> window(60s) |> sum(temp)").unwrap();
    let mut matcher = Matcher::new(selector);
    let mut headers = HashMap::new();
    headers.insert(Cow::Borrowed("temp"), Cow::Borrowed("1"));
    let msg = Message {
        topic: "sensor",
        headers,
        payload: None,
    };
    let mut timestamp = Instant::now();

    // Warm up the state to exercise pruning logic across iterations.
    for _ in 0..10 {
        timestamp += Duration::from_secs(1);
        matcher.process(&msg, timestamp);
    }

    c.bench_function("window_sum_process", |b| {
        b.iter(|| {
            timestamp += Duration::from_secs(1);
            black_box(matcher.process(&msg, timestamp))
        });
    });
}

fn bench_window_avg(c: &mut Criterion) {
    let selector = compile("/sensor |> window(60s) |> avg(json$.value)").unwrap();
    let mut matcher = Matcher::new(selector);
    let msg = Message {
        topic: "sensor",
        headers: HashMap::new(),
        payload: Some(serde_json::json!({"value": 1})),
    };
    let mut timestamp = Instant::now();

    for _ in 0..10 {
        timestamp += Duration::from_secs(1);
        matcher.process(&msg, timestamp);
    }

    c.bench_function("window_avg_process", |b| {
        b.iter(|| {
            timestamp += Duration::from_secs(1);
            black_box(matcher.process(&msg, timestamp))
        });
    });
}

/// Selectors shaped like broker-side filtering: a short topic, one or two
/// predicates, evaluated once per incoming message.
fn bench_typical_filters(c: &mut Criterion) {
    let payload = serde_json::json!({"type": "temp", "value": 31.5, "status": "online"});
    let mut headers = HashMap::new();
    headers.insert(Cow::Borrowed("qos"), Cow::Borrowed("1"));
    headers.insert(Cow::Borrowed("retained"), Cow::Borrowed("false"));
    let msg = Message {
        topic: "building/e/floor4/sensor",
        headers,
        payload: Some(payload),
    };

    for (name, sel) in [
        ("plus_literal_hit", "/building/+/+/sensor"),
        ("plus_literal_miss", "/building/+/+/actuator"),
        ("descendant_json_pred", "//sensor[json$.value>30]"),
        ("descendant_json_str", "//sensor[json$.status=\"online\"]"),
        (
            "header_pred_hash",
            "/msg[qos<=1][retained=false]/building/#",
        ),
    ] {
        let matcher = Matcher::new(compile(sel).unwrap());
        c.bench_function(name, |b| b.iter(|| matcher.matches(black_box(&msg))));
    }
}

criterion_group!(
    benches,
    bench_descendant_long_topic,
    bench_window_sum,
    bench_window_avg,
    bench_typical_filters
);
criterion_main!(benches);
