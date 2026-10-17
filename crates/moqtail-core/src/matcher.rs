use crate::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Stage, Step, Value};
use serde_json::Value as JsonValue;
use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const ABS_EPS: f64 = 1e-12;
const REL_EPS: f64 = 1e-9;

pub struct Message<'a> {
    pub topic: &'a str,
    pub headers: HashMap<Cow<'a, str>, Cow<'a, str>>,
    pub payload: Option<JsonValue>,
}

enum StageState {
    Window {
        duration: Option<Duration>,
        values: VecDeque<(Instant, f64)>,
        sum: f64,
    },
    Counter {
        duration: Option<Duration>,
        timestamps: VecDeque<Instant>,
    },
}

pub struct Matcher {
    selector: Selector,
    stage_states: Vec<StageState>,
    /// Whether the selector can be matched by a single left-to-right walk; see
    /// [`Matcher::match_linear`].
    linear: bool,
}

#[derive(Clone, Copy)]
enum PredicateState {
    Unknown,
    Pass,
    Fail,
}

/// Buffers for [`Matcher::match_steps`], kept per thread so that matching a
/// message does not allocate once they have grown to fit.
#[derive(Default)]
struct Scratch {
    /// Byte ranges of the topic's segments.
    segments: Vec<(usize, usize)>,
    /// One bit per `(step, segment)` state already queued.
    visited: Vec<u64>,
    /// Memoised predicate outcome per step.
    predicates: Vec<PredicateState>,
    stack: Vec<(usize, usize)>,
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default());
}

/// Splits a topic into its levels. The empty topic has no levels.
fn topic_segments(topic: &str) -> impl Iterator<Item = &str> {
    (!topic.is_empty())
        .then(|| topic.split('/'))
        .into_iter()
        .flatten()
}

fn json_path<'a>(root: &'a JsonValue, path: &[String]) -> Option<&'a JsonValue> {
    let mut cur = root;
    for part in path {
        cur = cur.get(part)?;
    }
    Some(cur)
}

impl Matcher {
    pub fn new(selector: Selector) -> Self {
        let mut window_duration = None;
        let mut stage_states = Vec::new();
        for stage in &selector.stages {
            match stage {
                Stage::Window(duration) => {
                    window_duration = Some(*duration);
                }
                Stage::Sum(_) | Stage::Avg(_) => {
                    stage_states.push(StageState::Window {
                        duration: window_duration,
                        values: VecDeque::new(),
                        sum: 0.0,
                    });
                }
                Stage::Count => {
                    stage_states.push(StageState::Counter {
                        duration: window_duration,
                        timestamps: VecDeque::new(),
                    });
                }
            }
        }
        let last = selector.steps.len().saturating_sub(1);
        let linear = selector.steps.iter().enumerate().all(|(i, step)| {
            step.axis == Axis::Child && (step.segment != Segment::Hash || i == last)
        });
        Self {
            selector,
            stage_states,
            linear,
        }
    }

    pub fn matches(&self, msg: &Message) -> bool {
        let steps = &self.selector.steps;
        if self.linear {
            return Self::match_linear(steps, msg);
        }
        SCRATCH.with(|cell| match cell.try_borrow_mut() {
            Ok(mut scratch) => Self::match_steps(steps, msg, &mut scratch),
            // Only reachable if matching re-enters itself on this thread; fall
            // back to a fresh buffer rather than panicking.
            Err(_) => Self::match_steps(steps, msg, &mut Scratch::default()),
        })
    }

    /// The selector that this matcher evaluates.
    pub fn selector(&self) -> &Selector {
        &self.selector
    }

    /// Runs the post-match processing stages on a message.
    ///
    /// Each stage is evaluated sequentially once [`matches`](Self::matches) returns
    /// `true`. Windowing stages configure the trailing [`Duration`] used by
    /// subsequent aggregations. Aggregation stages (`sum`, `avg`, `count`) retain
    /// timestamped samples and evict entries whose age exceeds that duration.
    /// Without a configured window, aggregations are evaluated from only the
    /// current message. Missing fields cause processing to short-circuit with
    /// `None`.
    ///
    /// `sum` and `avg` maintain running totals so they execute in `O(1)` time per
    /// message, while `count` remains proportional to the number of retained
    /// samples. Expired entries are removed from the front of the deque in
    /// amortized constant time. Empty topics are handled by
    /// [`matches`](Self::matches) yielding `false` before processing, so `process`
    /// only runs on matching topics.
    pub fn process(&mut self, msg: &Message, timestamp: Instant) -> Option<f64> {
        if !self.matches(msg) {
            return None;
        }
        let mut result = None;
        let mut state_idx = 0;
        for stage in &self.selector.stages {
            match stage {
                Stage::Window(_) => {}
                Stage::Sum(field) => {
                    if let StageState::Window {
                        duration,
                        values,
                        sum,
                    } = &mut self.stage_states[state_idx]
                    {
                        let v = Self::extract_field(field, msg)?;
                        match duration {
                            Some(duration) => {
                                values.push_back((timestamp, v));
                                *sum += v;
                                *sum -= Self::prune_values(values, *duration, timestamp);
                                result = Some(*sum);
                            }
                            None => {
                                values.clear();
                                values.push_back((timestamp, v));
                                *sum = v;
                                result = Some(v);
                            }
                        }
                    }
                    state_idx += 1;
                }
                Stage::Avg(field) => {
                    if let StageState::Window {
                        duration,
                        values,
                        sum,
                    } = &mut self.stage_states[state_idx]
                    {
                        let v = Self::extract_field(field, msg)?;
                        match duration {
                            Some(duration) => {
                                values.push_back((timestamp, v));
                                *sum += v;
                                *sum -= Self::prune_values(values, *duration, timestamp);
                                let len = values.len();
                                result = if len == 0 {
                                    Some(0.0)
                                } else {
                                    Some(*sum / len as f64)
                                };
                            }
                            None => {
                                values.clear();
                                values.push_back((timestamp, v));
                                *sum = v;
                                result = Some(v);
                            }
                        }
                    }
                    state_idx += 1;
                }
                Stage::Count => {
                    if let StageState::Counter {
                        duration,
                        timestamps,
                    } = &mut self.stage_states[state_idx]
                    {
                        match duration {
                            Some(duration) => {
                                timestamps.push_back(timestamp);
                                Self::prune_timestamps(timestamps, *duration, timestamp);
                                result = Some(timestamps.len() as f64);
                            }
                            None => {
                                timestamps.clear();
                                timestamps.push_back(timestamp);
                                result = Some(1.0);
                            }
                        }
                    }
                    state_idx += 1;
                }
            }
        }
        result
    }

    fn prune_values(
        values: &mut VecDeque<(Instant, f64)>,
        duration: Duration,
        now: Instant,
    ) -> f64 {
        let mut removed_sum = 0.0;
        while let Some((ts, _)) = values.front() {
            if now.saturating_duration_since(*ts) > duration {
                if let Some((_, value)) = values.pop_front() {
                    removed_sum += value;
                }
            } else {
                break;
            }
        }
        removed_sum
    }

    fn prune_timestamps(timestamps: &mut VecDeque<Instant>, duration: Duration, now: Instant) {
        while let Some(ts) = timestamps.front() {
            if now.saturating_duration_since(*ts) > duration {
                timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// Matches a selector in which every step consumes a fixed number of
    /// segments: only child axes, with `#` (if present) as the final step.
    ///
    /// Such a selector has exactly one way to align with a topic, so the topic
    /// is walked once, segment by segment, with no allocation.
    fn match_linear(steps: &[Step], msg: &Message) -> bool {
        let mut segments = topic_segments(msg.topic);
        for step in steps {
            if !Self::predicates_match(&step.predicates, msg) {
                return false;
            }
            match step.segment {
                Segment::Literal(ref lit) => match segments.next() {
                    Some(seg) if seg == lit => {}
                    _ => return false,
                },
                Segment::Plus => {
                    if segments.next().is_none() {
                        return false;
                    }
                }
                Segment::Message => {}
                // `linear` only admits `#` as the last step, where it matches
                // whatever remains, including nothing.
                Segment::Hash => return true,
            }
        }
        segments.next().is_none()
    }

    /// Traverses the selector steps against the topic segments, exploring every
    /// alignment that `//` and interior `#` allow.
    ///
    /// A manual stack holds `(step_index, topic_index)` states and a bitmap
    /// records the states already queued, so each of the
    /// `(steps + 1) * (segments + 1)` states is expanded at most once. Step
    /// predicates do not depend on the topic position, so each step's
    /// predicates are evaluated at most once per message.
    fn match_steps(steps: &[Step], msg: &Message, scratch: &mut Scratch) -> bool {
        let topic = msg.topic;
        scratch.segments.clear();
        let mut offset = 0;
        for seg in topic_segments(topic) {
            scratch.segments.push((offset, offset + seg.len()));
            offset += seg.len() + 1;
        }
        let seg_count = scratch.segments.len();
        let width = seg_count + 1;
        let states = (steps.len() + 1) * width;
        scratch.visited.clear();
        scratch.visited.resize(states.div_ceil(64), 0);
        scratch.predicates.clear();
        scratch
            .predicates
            .resize(steps.len(), PredicateState::Unknown);
        scratch.stack.clear();

        let Scratch {
            segments,
            visited,
            predicates,
            stack,
        } = scratch;
        let segment = |i: usize| segments.get(i).map(|&(a, b)| &topic[a..b]);
        let push = |visited: &mut [u64], stack: &mut Vec<(usize, usize)>, step, idx| {
            let bit = step * width + idx;
            let (word, mask) = (bit / 64, 1u64 << (bit % 64));
            if visited[word] & mask == 0 {
                visited[word] |= mask;
                stack.push((step, idx));
            }
        };

        push(visited, stack, 0, 0);
        while let Some((step_idx, topic_idx)) = stack.pop() {
            if step_idx == steps.len() {
                if topic_idx == seg_count {
                    return true;
                }
                continue;
            }
            let step = &steps[step_idx];
            let ok = match predicates[step_idx] {
                PredicateState::Pass => true,
                PredicateState::Fail => false,
                PredicateState::Unknown => {
                    let ok = Self::predicates_match(&step.predicates, msg);
                    predicates[step_idx] = if ok {
                        PredicateState::Pass
                    } else {
                        PredicateState::Fail
                    };
                    ok
                }
            };
            if !ok {
                continue;
            }
            let starts = match step.axis {
                Axis::Child => topic_idx..=topic_idx,
                Axis::Descendant => topic_idx..=seg_count,
            };
            let next = step_idx + 1;
            for idx in starts {
                match step.segment {
                    Segment::Literal(ref lit) => {
                        if segment(idx) == Some(lit.as_str()) {
                            push(visited, stack, next, idx + 1);
                        }
                    }
                    Segment::Plus => {
                        if idx < seg_count {
                            push(visited, stack, next, idx + 1);
                        }
                    }
                    Segment::Hash => {
                        for end in idx..=seg_count {
                            push(visited, stack, next, end);
                        }
                    }
                    Segment::Message => push(visited, stack, next, idx),
                }
            }
        }
        false
    }

    fn predicates_match(preds: &[Predicate], msg: &Message) -> bool {
        preds.iter().all(|p| Self::predicate_match(p, msg))
    }

    /// Compares one field of the message against a predicate's literal.
    ///
    /// A field that is missing, or whose type differs from the literal's, does
    /// not match. Strings and booleans only support `=`.
    fn predicate_match(pred: &Predicate, msg: &Message) -> bool {
        match pred.field {
            Field::Header(ref name) => {
                let Some(hv) = msg.headers.get(name.as_str()) else {
                    return false;
                };
                let hv = hv.as_ref();
                match &pred.value {
                    Value::Number(r) => hv
                        .parse::<f64>()
                        .is_ok_and(|l| Self::compare_numbers(l, *r, pred.op)),
                    Value::Bool(r) => match hv {
                        "true" => pred.op == Operator::Eq && *r,
                        "false" => pred.op == Operator::Eq && !*r,
                        _ => false,
                    },
                    Value::Str(r) => pred.op == Operator::Eq && hv == r,
                }
            }
            Field::Json(ref path) => {
                let Some(v) = msg.payload.as_ref().and_then(|j| json_path(j, path)) else {
                    return false;
                };
                match (&pred.value, v) {
                    (Value::Number(r), JsonValue::Number(n)) => n
                        .as_f64()
                        .is_some_and(|l| Self::compare_numbers(l, *r, pred.op)),
                    (Value::Bool(r), JsonValue::Bool(l)) => pred.op == Operator::Eq && l == r,
                    (Value::Str(r), JsonValue::String(l)) => pred.op == Operator::Eq && l == r,
                    _ => false,
                }
            }
        }
    }

    fn compare_numbers(l: f64, r: f64, op: Operator) -> bool {
        if l.is_nan() || r.is_nan() {
            return false;
        }

        if l.is_infinite() || r.is_infinite() {
            return match op {
                Operator::Eq => l == r,
                Operator::Lt => l < r,
                Operator::Gt => l > r,
                Operator::Le => l <= r,
                Operator::Ge => l >= r,
            };
        }

        let diff = (l - r).abs();
        let scale = l.abs().max(r.abs());
        let tol = ABS_EPS.max(REL_EPS * scale);
        let eq = diff <= tol;
        let ord = l.total_cmp(&r);
        match op {
            Operator::Eq => eq,
            Operator::Lt => ord == Ordering::Less && !eq,
            Operator::Gt => ord == Ordering::Greater && !eq,
            Operator::Le => ord != Ordering::Greater || eq,
            Operator::Ge => ord != Ordering::Less || eq,
        }
    }

    #[cfg(test)]
    fn compare_values(left: &Value, right: &Value, op: Operator) -> bool {
        match (left, right) {
            (Value::Number(l), Value::Number(r)) => Self::compare_numbers(*l, *r, op),
            (Value::Bool(l), Value::Bool(r)) => match op {
                Operator::Eq => l == r,
                _ => false,
            },
            (Value::Str(l), Value::Str(r)) => match op {
                Operator::Eq => l == r,
                _ => false,
            },
            _ => false,
        }
    }

    fn extract_field(field: &Field, msg: &Message) -> Option<f64> {
        match field {
            Field::Header(name) => msg.headers.get(name.as_str())?.as_ref().parse::<f64>().ok(),
            Field::Json(path) => {
                let v = json_path(msg.payload.as_ref()?, path)?;
                if let Some(f) = v.as_f64() {
                    Some(f)
                } else if let Some(i) = v.as_i64() {
                    Some(i as f64)
                } else {
                    v.as_u64().map(|u| u as f64)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::compile;
    use serde_json::json;
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    fn make_msg(topic: &str) -> Message<'_> {
        Message {
            topic,
            headers: HashMap::new(),
            payload: None,
        }
    }

    #[test]
    fn simple_match() {
        let sel = compile("/foo/bar").unwrap();
        let m = Matcher::new(sel);
        assert!(m.matches(&make_msg("foo/bar")));
        assert!(!m.matches(&make_msg("foo/baz")));
    }

    #[test]
    fn plus_wildcard() {
        let sel = compile("/foo/+").unwrap();
        let m = Matcher::new(sel);
        assert!(m.matches(&make_msg("foo/bar")));
        assert!(m.matches(&make_msg("foo/baz")));
        assert!(!m.matches(&make_msg("foo")));
    }

    #[test]
    fn hash_wildcard() {
        let sel = compile("/foo/#").unwrap();
        let m = Matcher::new(sel);
        assert!(m.matches(&make_msg("foo")));
        assert!(m.matches(&make_msg("foo/bar/baz")));
    }

    #[test]
    fn descendant_axis() {
        let sel = compile("//sensor").unwrap();
        let m = Matcher::new(sel);
        assert!(m.matches(&make_msg("building/floor/sensor")));
        assert!(!m.matches(&make_msg("building/floor/actuator")));
    }

    #[test]
    fn small_magnitude_values_use_absolute_tolerance() {
        let l = Value::Number(1e-13);
        let r = Value::Number(9e-13);
        assert!(Matcher::compare_values(&l, &r, Operator::Eq));
    }

    #[test]
    fn large_magnitude_values_use_relative_tolerance() {
        let l = Value::Number(1_000_000_000.0);
        let r = Value::Number(1_000_000_000.5);
        assert!(Matcher::compare_values(&l, &r, Operator::Eq));
    }

    #[test]
    fn relative_tolerance_boundary_remains_equal() {
        let l = Value::Number(2_000_000_000.0);
        let r = Value::Number(2_000_000_002.0);
        assert!(Matcher::compare_values(&l, &r, Operator::Eq));
    }

    #[test]
    fn values_outside_hybrid_tolerance_are_not_equal() {
        // The absolute floor only dominates near zero; at any other scale the
        // relative term decides, so the gap has to clear `REL_EPS * scale`.
        let l = Value::Number(0.0);
        let r = Value::Number(ABS_EPS * 4.0);
        assert!(!Matcher::compare_values(&l, &r, Operator::Eq));

        let l_unit = Value::Number(1.0);
        let r_unit = Value::Number(1.0 + REL_EPS * 4.0);
        assert!(!Matcher::compare_values(&l_unit, &r_unit, Operator::Eq));

        let l_large = Value::Number(1_000_000_000.0);
        let r_large = Value::Number(1_000_000_002.0);
        assert!(!Matcher::compare_values(&l_large, &r_large, Operator::Eq));
    }

    #[test]
    fn ordering_within_tolerance_is_equal() {
        let l = Value::Number(1_000_000_000.0);
        let r = Value::Number(1_000_000_000.5);
        assert!(!Matcher::compare_values(&l, &r, Operator::Lt));
        assert!(Matcher::compare_values(&l, &r, Operator::Le));
        assert!(!Matcher::compare_values(&r, &l, Operator::Gt));
        assert!(Matcher::compare_values(&r, &l, Operator::Ge));
    }

    #[test]
    fn ordering_outside_tolerance_respects_direction() {
        let l = Value::Number(1_000_000_000.0);
        let r = Value::Number(1_000_000_002.0);
        assert!(Matcher::compare_values(&l, &r, Operator::Lt));
        assert!(Matcher::compare_values(&r, &l, Operator::Gt));
    }

    #[test]
    fn nan_comparisons_are_always_false() {
        let nan = Value::Number(f64::NAN);
        let num = Value::Number(1.0);
        assert!(!Matcher::compare_values(&nan, &num, Operator::Eq));
        assert!(!Matcher::compare_values(&nan, &num, Operator::Lt));
        assert!(!Matcher::compare_values(&nan, &num, Operator::Gt));
    }

    #[test]
    fn infinity_comparisons() {
        let inf = Value::Number(f64::INFINITY);
        let neg_inf = Value::Number(f64::NEG_INFINITY);
        let num = Value::Number(1.0);
        assert!(Matcher::compare_values(&inf, &inf, Operator::Eq));
        assert!(Matcher::compare_values(&neg_inf, &neg_inf, Operator::Eq));
        assert!(Matcher::compare_values(&inf, &num, Operator::Gt));
        assert!(Matcher::compare_values(&neg_inf, &num, Operator::Lt));
    }

    #[test]
    fn predicate_on_json_field() {
        let sel = compile("/foo[json$.temp>30]").unwrap();
        let m = Matcher::new(sel);
        let msg = Message {
            topic: "foo",
            headers: HashMap::new(),
            payload: Some(json!({"temp": 35})),
        };
        assert!(m.matches(&msg));

        let msg = Message {
            topic: "foo",
            headers: HashMap::new(),
            payload: Some(json!({"temp": 25})),
        };
        assert!(!m.matches(&msg));
    }

    #[test]
    fn extract_json_field() {
        let msg = Message {
            topic: "foo",
            headers: HashMap::new(),
            payload: Some(json!({"temp": 21})),
        };
        let field = Field::Json(vec!["temp".into()]);
        assert_eq!(Matcher::extract_field(&field, &msg), Some(21.0));
    }

    #[test]
    fn process_sum_without_window() {
        let sel = compile("/sensor |> sum(temp)").unwrap();
        let mut m = Matcher::new(sel);

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("10"))]),
            payload: None,
        };
        let start = Instant::now();
        assert_eq!(m.process(&msg1, start), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("20"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg2, start + Duration::from_secs(1)), Some(20.0));
    }

    #[test]
    fn process_sum_without_window_same_instant_uses_current_message_only() {
        let sel = compile("/sensor |> sum(temp)").unwrap();
        let mut m = Matcher::new(sel);
        let timestamp = Instant::now();

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("10"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg1, timestamp), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("20"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg2, timestamp), Some(20.0));
    }

    #[test]
    fn process_sum_with_window() {
        let sel = compile("/sensor |> window(2s) |> sum(temp)").unwrap();
        let mut m = Matcher::new(sel);

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("10"))]),
            payload: None,
        };
        let start = Instant::now();
        assert_eq!(m.process(&msg1, start), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("20"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg2, start + Duration::from_secs(1)), Some(30.0));

        let msg3 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("30"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg3, start + Duration::from_secs(3)), Some(50.0));

        // Once the second sample ages out, only the most recent value remains.
        assert_eq!(m.process(&msg3, start + Duration::from_secs(6)), Some(30.0));
    }

    #[test]
    fn process_explicit_zero_duration_window_keeps_same_instant_samples() {
        let sel = compile("/sensor |> window(0s) |> sum(temp)").unwrap();
        let mut m = Matcher::new(sel);
        let timestamp = Instant::now();

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("10"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg1, timestamp), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("20"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg2, timestamp), Some(30.0));
    }

    #[test]
    fn process_avg_without_window() {
        let sel = compile("/sensor |> avg(json$.value)").unwrap();
        let mut m = Matcher::new(sel);

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: Some(json!({"value": 10})),
        };
        let start = Instant::now();
        assert_eq!(m.process(&msg1, start), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: Some(json!({"value": 20})),
        };
        assert_eq!(m.process(&msg2, start + Duration::from_secs(2)), Some(20.0));
    }

    #[test]
    fn process_avg_without_window_same_instant_uses_current_message_only() {
        let sel = compile("/sensor |> avg(temp)").unwrap();
        let mut m = Matcher::new(sel);
        let timestamp = Instant::now();

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("10"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg1, timestamp), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("20"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg2, timestamp), Some(20.0));
    }

    #[test]
    fn process_avg_with_window() {
        let sel = compile("/sensor |> window(2s) |> avg(json$.value)").unwrap();
        let mut m = Matcher::new(sel);

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: Some(json!({"value": 10})),
        };
        let start = Instant::now();
        assert_eq!(m.process(&msg1, start), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: Some(json!({"value": 20})),
        };
        assert_eq!(m.process(&msg2, start + Duration::from_secs(1)), Some(15.0));

        let msg3 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: Some(json!({"value": 30})),
        };
        assert_eq!(m.process(&msg3, start + Duration::from_secs(3)), Some(25.0));

        assert_eq!(m.process(&msg3, start + Duration::from_secs(6)), Some(30.0));
    }

    #[test]
    fn process_count_without_window() {
        let sel = compile("/sensor |> count()").unwrap();
        let mut m = Matcher::new(sel);

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        let start = Instant::now();
        assert_eq!(m.process(&msg1, start), Some(1.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        assert_eq!(m.process(&msg2, start + Duration::from_secs(1)), Some(1.0));
    }

    #[test]
    fn process_count_without_window_same_instant_uses_current_message_only() {
        let sel = compile("/sensor |> count()").unwrap();
        let mut m = Matcher::new(sel);
        let timestamp = Instant::now();

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        assert_eq!(m.process(&msg1, timestamp), Some(1.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        assert_eq!(m.process(&msg2, timestamp), Some(1.0));
    }

    #[test]
    fn process_count_with_window() {
        let sel = compile("/sensor |> window(2s) |> count()").unwrap();
        let mut m = Matcher::new(sel);

        let msg1 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        let start = Instant::now();
        assert_eq!(m.process(&msg1, start), Some(1.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        assert_eq!(m.process(&msg2, start + Duration::from_secs(1)), Some(2.0));

        let msg3 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        assert_eq!(m.process(&msg3, start + Duration::from_secs(3)), Some(2.0));

        assert_eq!(m.process(&msg3, start + Duration::from_secs(6)), Some(1.0));
    }

    /// The linear walk is an optimisation of the general traversal, so the two
    /// must agree on every selector the linear walk accepts.
    #[test]
    fn linear_walk_agrees_with_general_traversal() {
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = move |n: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % n
        };
        let words = ["a", "b", "msg"];
        let headers = HashMap::from([(Cow::Borrowed("qos"), Cow::Borrowed("1"))]);
        let mut checked = 0;
        for _ in 0..20_000 {
            let steps = 1 + next(4);
            let mut sel = String::new();
            for i in 0..steps {
                sel.push('/');
                match next(5) {
                    0 => sel.push('+'),
                    1 if i == steps - 1 => sel.push('#'),
                    _ => sel.push_str(words[next(3) as usize]),
                }
                if next(6) == 0 {
                    sel.push_str(if next(2) == 0 { "[qos=1]" } else { "[qos=2]" });
                }
            }
            let matcher = Matcher::new(compile(&sel).unwrap());
            assert!(matcher.linear, "{sel} should take the linear walk");

            let levels = next(5);
            let topic: Vec<&str> = (0..levels)
                .map(|_| ["a", "b", ""][next(3) as usize])
                .collect();
            let topic = topic.join("/");
            let msg = Message {
                topic: &topic,
                headers: headers.clone(),
                payload: None,
            };
            let general =
                Matcher::match_steps(&matcher.selector.steps, &msg, &mut Scratch::default());
            assert_eq!(
                Matcher::match_linear(&matcher.selector.steps, &msg),
                general,
                "selector {sel:?} against topic {topic:?}"
            );
            checked += 1;
        }
        assert_eq!(checked, 20_000);
    }

    #[test]
    fn interior_hash_and_descendant_use_general_traversal() {
        for sel in ["//a", "/#/a", "/a//b", "/#/#"] {
            assert!(!Matcher::new(compile(sel).unwrap()).linear, "{sel}");
        }
        for sel in ["/a/+", "/a/#", "/msg[qos=1]/a", "/#"] {
            assert!(Matcher::new(compile(sel).unwrap()).linear, "{sel}");
        }
    }

    #[test]
    fn nested_wildcards_terminate_and_match() {
        let sel = compile("//#/#/sensor").unwrap();
        let m = Matcher::new(sel);
        let mut segments: Vec<String> = (0..20).map(|i| format!("seg{}", i)).collect();
        segments.push("sensor".to_string());
        let topic = segments.join("/");
        let start = Instant::now();
        assert!(m.matches(&make_msg(&topic)));
        assert!(start.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn nested_wildcards_terminate_and_no_match() {
        let sel = compile("//#/#/sensor").unwrap();
        let m = Matcher::new(sel);
        let mut segments: Vec<String> = (0..20).map(|i| format!("seg{}", i)).collect();
        segments.push("other".to_string());
        let topic = segments.join("/");
        assert!(!m.matches(&make_msg(&topic)));
    }
}
