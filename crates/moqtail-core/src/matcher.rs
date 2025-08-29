use crate::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Stage, Step, Value};
use serde_json::Value as JsonValue;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

const FLOAT_TOLERANCE: f64 = f64::EPSILON;

pub struct Message<'a> {
    pub topic: &'a str,
    pub headers: HashMap<Cow<'a, str>, Cow<'a, str>>,
    pub payload: Option<JsonValue>,
}

enum StageState {
    Window { size: usize, values: VecDeque<f64> },
    Counter { size: usize, count: usize },
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
        let mut window_size = 1usize;
        let mut stage_states = Vec::new();
        for stage in &selector.stages {
            match stage {
                Stage::Window(n) => {
                    window_size = *n as usize;
                }
                Stage::Sum(_) | Stage::Avg(_) => {
                    stage_states.push(StageState::Window {
                        size: window_size,
                        values: VecDeque::new(),
                    });
                }
                Stage::Count => {
                    stage_states.push(StageState::Counter {
                        size: window_size,
                        count: 0,
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

    pub fn process(&mut self, msg: &Message) -> Option<f64> {
        if !self.matches(msg) {
            return None;
        }
        let mut result = None;
        let mut state_idx = 0;
        for stage in &self.selector.stages {
            match stage {
                Stage::Window(_) => {}
                Stage::Sum(field) => {
                    if let StageState::Window { size, values } = &mut self.stage_states[state_idx] {
                        let v = Self::extract_field(field, msg)?;
                        values.push_back(v);
                        if values.len() > *size {
                            values.pop_front();
                        }
                        result = Some(values.iter().sum());
                    }
                    state_idx += 1;
                }
                Stage::Avg(field) => {
                    if let StageState::Window { size, values } = &mut self.stage_states[state_idx] {
                        let v = Self::extract_field(field, msg)?;
                        values.push_back(v);
                        if values.len() > *size {
                            values.pop_front();
                        }
                        let sum: f64 = values.iter().sum();
                        result = Some(sum / values.len() as f64);
                    }
                    state_idx += 1;
                }
                Stage::Count => {
                    if let StageState::Counter { size, count } = &mut self.stage_states[state_idx] {
                        *count += 1;
                        if *count > *size {
                            *count -= 1;
                        }
                        result = Some(*count as f64);
                    }
                    state_idx += 1;
                }
            }
        }
        result
    }

    fn match_steps(steps: &[Step], topic: &[&str], msg: &Message) -> bool {
        let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
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
                    Self::match_child(&mut stack, step, step_idx + 1, topic, topic_idx);
                }
                Axis::Descendant => {
                    let mut start = topic_idx;
                    while start <= topic.len() {
                        Self::match_child(&mut stack, step, step_idx + 1, topic, start);
                        start += 1;
                    }
                }
            }
        }
        false
    }

    fn match_child(
        stack: &mut Vec<(usize, usize)>,
        step: &Step,
        next_step: usize,
        topic: &[&str],
        idx: usize,
    ) {
        match step.segment {
            Segment::Literal(ref lit) => {
                if let Some(seg) = topic.get(idx) {
                    if lit == seg {
                        stack.push((next_step, idx + 1));
                    }
                }
            }
            Segment::Plus => {
                if topic.get(idx).is_some() {
                    stack.push((next_step, idx + 1));
                }
            }
            Segment::Hash => {
                let mut i = idx;
                while i <= topic.len() {
                    stack.push((next_step, i));
                    i += 1;
                }
            }
            Segment::Message => {
                stack.push((next_step, idx));
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
                if let Ok(num) = hv.parse::<f64>() {
                    Value::Number(num)
                } else if hv == "true" || hv == "false" {
                    Value::Bool(hv == "true")
                } else {
                    Value::Str(hv.to_string())
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

        let eq = float_cmp::approx_eq!(f64, l, r, epsilon = FLOAT_TOLERANCE);
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
                } else {
                    v.as_i64().map(|i| i as f64)
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
    fn numbers_within_epsilon_are_equal() {
        let l = Value::Number(1.0);
        let r = Value::Number(1.0 + FLOAT_TOLERANCE / 2.0);
        assert!(Matcher::compare_values(&l, &r, Operator::Eq));
    }

    #[test]
    fn numbers_outside_epsilon_are_not_equal() {
        let l = Value::Number(1.0);
        let r = Value::Number(1.0 + FLOAT_TOLERANCE * 2.0);
        assert!(!Matcher::compare_values(&l, &r, Operator::Eq));
    }

    #[test]
    fn ordering_within_tolerance_is_equal() {
        let l = Value::Number(1.0);
        let r = Value::Number(1.0 + FLOAT_TOLERANCE / 2.0);
        assert!(!Matcher::compare_values(&l, &r, Operator::Lt));
        assert!(Matcher::compare_values(&l, &r, Operator::Le));
        assert!(!Matcher::compare_values(&r, &l, Operator::Gt));
        assert!(Matcher::compare_values(&r, &l, Operator::Ge));
    }

    #[test]
    fn ordering_outside_tolerance_respects_direction() {
        let l = Value::Number(1.0);
        let r = Value::Number(1.0 + FLOAT_TOLERANCE * 2.0);
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
        assert_eq!(m.process(&msg1), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("20"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg2), Some(20.0));
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
        assert_eq!(m.process(&msg1), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("20"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg2), Some(30.0));

        let msg3 = Message {
            topic: "sensor",
            headers: HashMap::from([(Cow::Borrowed("temp"), Cow::Borrowed("30"))]),
            payload: None,
        };
        assert_eq!(m.process(&msg3), Some(50.0));
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
        assert_eq!(m.process(&msg1), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: Some(json!({"value": 20})),
        };
        assert_eq!(m.process(&msg2), Some(20.0));
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
        assert_eq!(m.process(&msg1), Some(10.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: Some(json!({"value": 20})),
        };
        assert_eq!(m.process(&msg2), Some(15.0));

        let msg3 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: Some(json!({"value": 30})),
        };
        assert_eq!(m.process(&msg3), Some(25.0));
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
        assert_eq!(m.process(&msg1), Some(1.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        assert_eq!(m.process(&msg2), Some(1.0));
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
        assert_eq!(m.process(&msg1), Some(1.0));

        let msg2 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        assert_eq!(m.process(&msg2), Some(2.0));

        let msg3 = Message {
            topic: "sensor",
            headers: HashMap::new(),
            payload: None,
        };
        assert_eq!(m.process(&msg3), Some(2.0));
    }
}
