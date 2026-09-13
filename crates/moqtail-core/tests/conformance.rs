//! Runs the golden conformance corpus against the Rust engine.
//!
//! The same file is run against the browser port in `sim/js/moqtail.js` by
//! `sim/tests/conformance.test.mjs`, so a behaviour that only one engine has
//! fails here or there. Adding a case to `conformance/corpus.json` therefore
//! constrains both implementations at once.

use moqtail_core::{compile, Matcher, Message};
use serde_json::Value as JsonValue;
use std::borrow::Cow;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const CORPUS: &str = include_str!("conformance/corpus.json");

fn corpus() -> JsonValue {
    serde_json::from_str(CORPUS).expect("conformance corpus is valid JSON")
}

fn cases<'a>(root: &'a JsonValue, section: &str) -> &'a Vec<JsonValue> {
    root.get(section)
        .and_then(JsonValue::as_array)
        .unwrap_or_else(|| panic!("corpus is missing the `{section}` section"))
}

fn str_at<'a>(case: &'a JsonValue, key: &str) -> &'a str {
    case.get(key)
        .and_then(JsonValue::as_str)
        .unwrap_or_else(|| panic!("case {case} is missing a string `{key}`"))
}

/// Header maps in the corpus are string -> string, matching MQTT properties.
fn headers_of(case: &JsonValue) -> HashMap<Cow<'_, str>, Cow<'_, str>> {
    match case.get("headers").and_then(JsonValue::as_object) {
        Some(map) => map
            .iter()
            .map(|(name, value)| {
                let value = value
                    .as_str()
                    .unwrap_or_else(|| panic!("header `{name}` must be a string in {case}"));
                (Cow::Borrowed(name.as_str()), Cow::Borrowed(value))
            })
            .collect(),
        None => HashMap::new(),
    }
}

/// An absent or null `payload` is `None`, the way a bodyless message arrives.
fn payload_of(case: &JsonValue) -> Option<JsonValue> {
    case.get("payload").filter(|v| !v.is_null()).cloned()
}

fn message_of(case: &JsonValue) -> Message<'_> {
    Message {
        topic: str_at(case, "topic"),
        headers: headers_of(case),
        payload: payload_of(case),
    }
}

fn approx(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9 * left.abs().max(right.abs()).max(1.0)
}

#[test]
fn compile_cases() {
    let root = corpus();
    for case in cases(&root, "compile") {
        let selector = str_at(case, "selector");
        let expect_ok = case["ok"].as_bool().expect("`ok` is a boolean");
        match compile(selector) {
            Ok(compiled) => {
                assert!(
                    expect_ok,
                    "{selector:?} compiled but the corpus expects a failure"
                );
                if let Some(expected) = case.get("display").and_then(JsonValue::as_str) {
                    assert_eq!(
                        compiled.to_string(),
                        expected,
                        "{selector:?} did not round-trip through Display"
                    );
                    // A canonical form that does not itself compile would make
                    // Display useless for round-tripping, so check it does.
                    let recompiled = compile(expected).unwrap_or_else(|e| {
                        panic!("canonical form {expected:?} failed to compile: {e}")
                    });
                    assert_eq!(
                        recompiled.to_string(),
                        expected,
                        "Display is not idempotent for {selector:?}"
                    );
                }
            }
            Err(e) => {
                assert!(!expect_ok, "{selector:?} failed to compile: {e}");
                if let Some(expected) = case.get("error").and_then(JsonValue::as_str) {
                    assert_eq!(e.to_string(), expected, "wrong error for {selector:?}");
                }
            }
        }
    }
}

#[test]
fn match_cases() {
    let root = corpus();
    for case in cases(&root, "match") {
        let selector = str_at(case, "selector");
        let expect = case["expect"].as_bool().expect("`expect` is a boolean");
        let compiled =
            compile(selector).unwrap_or_else(|e| panic!("{selector:?} failed to compile: {e}"));
        let matcher = Matcher::new(compiled);
        let msg = message_of(case);
        assert_eq!(
            matcher.matches(&msg),
            expect,
            "{selector:?} against topic {:?}",
            msg.topic
        );
    }
}

#[test]
fn process_cases() {
    let root = corpus();
    for case in cases(&root, "process") {
        let selector = str_at(case, "selector");
        let compiled =
            compile(selector).unwrap_or_else(|e| panic!("{selector:?} failed to compile: {e}"));
        let mut matcher = Matcher::new(compiled);
        let base = Instant::now();
        let steps = case
            .get("steps")
            .and_then(JsonValue::as_array)
            .expect("`steps` is an array");

        for (index, step) in steps.iter().enumerate() {
            let at_ms = step["at_ms"].as_u64().expect("`at_ms` is an integer");
            let msg = message_of(step);
            let got = matcher.process(&msg, base + Duration::from_millis(at_ms));
            match step["expect"].as_f64() {
                Some(expected) => {
                    let got = got.unwrap_or_else(|| {
                        panic!("{selector:?} step {index} returned None, expected {expected}")
                    });
                    assert!(
                        approx(got, expected),
                        "{selector:?} step {index} returned {got}, expected {expected}"
                    );
                }
                None => assert!(
                    got.is_none(),
                    "{selector:?} step {index} returned {got:?}, expected None"
                ),
            }
        }
    }
}

/// The corpus exists to hold both engines to the same behaviour, so it is worth
/// noticing if it quietly shrinks.
#[test]
fn corpus_is_not_empty() {
    let root = corpus();
    assert!(cases(&root, "compile").len() >= 40);
    assert!(cases(&root, "match").len() >= 50);
    assert!(cases(&root, "process").len() >= 8);
}
