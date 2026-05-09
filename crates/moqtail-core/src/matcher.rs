use crate::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Stage, Step, Value};
use serde_json::Value as JsonValue;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
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
        Self {
            selector,
            stage_states,
        }
    }

    pub fn matches(&self, msg: &Message) -> bool {
        let segments: Vec<&str> = if msg.topic.is_empty() {
            Vec::new()
        } else {
            msg.topic.split('/').collect()
        };
        Self::match_steps(&self.selector.steps, &segments, msg)
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

    /// Iteratively traverses the selector steps against the topic segments.
    ///
    /// A manual stack stores pairs of `(step_index, topic_index)` representing
    /// the traversal state. This avoids recursion and allows exploring multiple
    /// branches introduced by wildcards and descendant axes. The algorithm
    /// short-circuits once a full match is found.
    ///
    /// Complexity is approximately `O(steps * segments)` in typical cases, but
    /// nested wildcards (e.g. `#` combined with descendant axes) may lead to a
    /// combinatorial explosion of states. Empty topics are represented as an
    /// empty slice and handled naturally by the traversal.
    fn match_steps(steps: &[Step], topic: &[&str], msg: &Message) -> bool {
        let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
        let mut visited: HashSet<(usize, usize)> = HashSet::from([(0, 0)]);

        while let Some((step_idx, topic_idx)) = stack.pop() {
            if step_idx == steps.len() {
                if topic_idx == topic.len() {
                    return true;
                }
                continue;
            }
            let step = &steps[step_idx];
            if !Self::predicates_match(&step.predicates, msg) {
                continue;
            }
            match step.axis {
                Axis::Child => {
                    Self::match_child(
                        &mut stack,
                        step,
                        step_idx + 1,
                        topic,
                        topic_idx,
                        &mut visited,
                    );
                }
                Axis::Descendant => {
                    let mut start = topic_idx;
                    while start <= topic.len() {
                        Self::match_child(
                            &mut stack,
                            step,
                            step_idx + 1,
                            topic,
                            start,
                            &mut visited,
                        );
                        start += 1;
                    }
                }
            }
        }
        false
    }

    /// Expands the traversal stack for a single step at a given topic index.
    ///
    /// Depending on the [`Segment`] variant, it pushes one or more new states:
    ///
    /// * `Literal` matches an exact segment.
    /// * `Plus` consumes exactly one segment.
    /// * `Hash` explores all remaining suffixes, matching zero or more segments.
    /// * `Message` does not consume any segment.
    ///
    /// In the presence of nested wildcards the number of states can grow
    /// quickly, which impacts matching complexity.
    fn match_child(
        stack: &mut Vec<(usize, usize)>,
        step: &Step,
        next_step: usize,
        topic: &[&str],
        idx: usize,
        visited: &mut HashSet<(usize, usize)>,
    ) {
        match step.segment {
            Segment::Literal(ref lit) => {
                if let Some(seg) = topic.get(idx) {
                    if lit == seg {
                        let state = (next_step, idx + 1);
                        if visited.insert(state) {
                            stack.push(state);
                        }
                    }
                }
            }
            Segment::Plus => {
                if topic.get(idx).is_some() {
                    let state = (next_step, idx + 1);
                    if visited.insert(state) {
                        stack.push(state);
                    }
                }
            }
            Segment::Hash => {
                let mut i = idx;
                while i <= topic.len() {
                    let state = (next_step, i);
                    if visited.insert(state) {
                        stack.push(state);
                    }
                    i += 1;
                }
            }
            Segment::Message => {
                let state = (next_step, idx);
                if visited.insert(state) {
                    stack.push(state);
                }
            }
        }
    }

    fn predicates_match(preds: &[Predicate], msg: &Message) -> bool {
        for p in preds {
            if !Self::predicate_match(p, msg) {
                return false;
            }
        }
        true
    }

    fn predicate_match(pred: &Predicate, msg: &Message) -> bool {
        let left = match pred.field {
            Field::Header(ref name) => {
                let hv = match msg.headers.get(name.as_str()) {
                    Some(v) => v.as_ref(),
                    None => return false,
                };
                match &pred.value {
                    Value::Number(_) => match hv.parse::<f64>() {
                        Ok(num) => Value::Number(num),
                        Err(_) => return false,
                    },
                    Value::Bool(_) => match hv {
                        "true" => Value::Bool(true),
                        "false" => Value::Bool(false),
                        _ => return false,
                    },
                    Value::Str(_) => Value::Str(hv.to_string()),
                }
            }
            Field::Json(ref path) => {
                let cur = match msg.payload.as_ref() {
                    Some(j) => json_path(j, path),
                    None => None,
                };
                let cur = match cur {
                    Some(v) => v,
                    None => return false,
                };
                if let Some(b) = cur.as_bool() {
                    Value::Bool(b)
                } else if let Some(n) = cur.as_f64() {
                    Value::Number(n)
                } else if let Some(s) = cur.as_str() {
                    Value::Str(s.to_string())
                } else {
                    return false;
                }
            }
        };

        Self::compare_values(&left, &pred.value, pred.op)
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
        let l = Value::Number(1.0);
        let r = Value::Number(1.0 + ABS_EPS * 4.0);
        assert!(!Matcher::compare_values(&l, &r, Operator::Eq));

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
