use std::collections::HashMap;

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

criterion_group!(benches, bench_descendant_long_topic);
criterion_main!(benches);
