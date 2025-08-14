use crate::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Stage, Step, Value};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, VecDeque};

pub struct Message<'a> {
    pub topic: &'a str,
    pub headers: HashMap<String, String>,
    pub payload: Option<JsonValue>,
}

pub struct Matcher {
    selector: Selector,
    window: VecDeque<f64>,
    window_size: usize,
}

impl Matcher {
    pub fn new(selector: Selector) -> Self {
        let window_size = selector
            .stages
            .iter()
            .find_map(|s| match s {
                Stage::Window(n) => Some(*n as usize),
                _ => None,
            })
            .unwrap_or(1);
        Self {
            selector,
            window: VecDeque::new(),
            window_size,
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
        let stages = &self.selector.stages;
        let window_size = self.window_size;
        let window = &mut self.window;
        for stage in stages {
            match stage {
                Stage::Window(_) => {}
                Stage::Sum(field) => {
                    let v = Self::extract_field(field, msg)?;
                    window.push_back(v);
                    if window.len() > window_size {
                        window.pop_front();
                    }
                    result = Some(window.iter().sum());
                }
                Stage::Avg(field) => {
                    let v = Self::extract_field(field, msg)?;
                    window.push_back(v);
                    if window.len() > window_size {
                        window.pop_front();
                    }
                    let sum: f64 = window.iter().sum();
                    result = Some(sum / window.len() as f64);
                }
                Stage::Count => {
                    window.push_back(0.0);
                    if window.len() > window_size {
                        window.pop_front();
                    }
                    result = Some(window.len() as f64);
                }
            }
        }
        result
    }

    fn match_steps(steps: &[Step], topic: &[&str], msg: &Message) -> bool {
        if steps.is_empty() {
            return topic.is_empty();
        }
        let step = &steps[0];
        let rest = &steps[1..];
        let preds_ok = Self::predicates_match(&step.predicates, msg);

        if !preds_ok {
            return false;
        }

        match step.axis {
            Axis::Child => Self::match_child(step, rest, topic, msg),
            Axis::Descendant => {
                // try to match at current or any subsequent position
                let mut idx = 0;
                while idx <= topic.len() {
                    if Self::match_child(step, rest, &topic[idx..], msg) {
                        return true;
                    }
                    idx += 1;
                }
                false
            }
        }
    }

    fn match_child(step: &Step, rest: &[Step], topic: &[&str], msg: &Message) -> bool {
        match step.segment {
            Segment::Literal(ref lit) => {
                if let Some((first, rest_topic)) = topic.split_first() {
                    if lit == first {
                        Self::match_steps(rest, rest_topic, msg)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Segment::Plus => {
                if let Some((_first, rest_topic)) = topic.split_first() {
                    Self::match_steps(rest, rest_topic, msg)
                } else {
                    false
                }
            }
            Segment::Hash => {
                // Try zero or more segments
                if Self::match_steps(rest, topic, msg) {
                    return true;
                }
                let mut idx = 0;
                while idx < topic.len() {
                    if Self::match_steps(rest, &topic[idx + 1..], msg) {
                        return true;
                    }
                    idx += 1;
                }
                false
            }
            Segment::Message => Self::match_steps(rest, topic, msg),
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
                let hv = match msg.headers.get(name) {
                    Some(v) => v,
                    None => return false,
                };
                if let Ok(num) = hv.parse::<i64>() {
                    Value::Number(num)
                } else {
                    Value::Bool(hv == "true")
                }
            }
            Field::Json(ref path) => {
                let mut cur = match msg.payload {
                    Some(ref j) => j,
                    None => return false,
                };
                for part in path {
                    cur = match cur.get(part) {
                        Some(v) => v,
                        None => return false,
                    };
                }
                if let Some(b) = cur.as_bool() {
                    Value::Bool(b)
                } else if let Some(n) = cur.as_i64() {
                    Value::Number(n)
                } else {
                    return false;
                }
            }
        };

        Self::compare_values(&left, &pred.value, pred.op)
    }

    fn compare_values(left: &Value, right: &Value, op: Operator) -> bool {
        match (left, right) {
            (Value::Number(l), Value::Number(r)) => match op {
                Operator::Eq => l == r,
                Operator::Lt => l < r,
                Operator::Gt => l > r,
                Operator::Le => l <= r,
                Operator::Ge => l >= r,
            },
            (Value::Bool(l), Value::Bool(r)) => match op {
                Operator::Eq => l == r,
                _ => false,
            },
            _ => false,
        }
    }

    fn extract_field(field: &Field, msg: &Message) -> Option<f64> {
        match field {
            Field::Header(name) => msg.headers.get(name)?.parse::<f64>().ok(),
            Field::Json(path) => {
                let mut cur = msg.payload.as_ref()?;
                for p in path {
                    cur = cur.get(p)?;
                }
                if let Some(v) = cur.as_f64() {
                    Some(v)
                } else {
                    cur.as_i64().map(|i| i as f64)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::compile;
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
}
