use crate::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Stage, Step, Value};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

pub struct Message<'a> {
    pub topic: &'a str,
    pub headers: HashMap<String, String>,
    pub payload: Option<JsonValue>,
}

pub struct Matcher {
    selector: Selector,
    window: Vec<f64>,
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
            window: Vec::new(),
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
        for stage in self.selector.stages.clone() {
            match stage {
                Stage::Window(_) => {}
                Stage::Sum(field) => {
                    let v = Self::extract_field(&field, msg)?;
                    self.push_value(v);
                    result = Some(self.window.iter().sum());
                }
                Stage::Avg(field) => {
                    let v = Self::extract_field(&field, msg)?;
                    self.push_value(v);
                    let sum: f64 = self.window.iter().sum();
                    result = Some(sum / self.window.len() as f64);
                }
                Stage::Count => {
                    self.push_value(0.0);
                    result = Some(self.window.len() as f64);
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
        match step.axis {
            Axis::Child => Self::match_child(step, &steps[1..], topic, msg),
            Axis::Descendant => {
                // try to match at current or any subsequent position
                for idx in 0..=topic.len() {
                    if Self::match_child(step, &steps[1..], &topic[idx..], msg) {
                        return true;
                    }
                    if idx == topic.len() {
                        break;
                    }
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
                        if Self::predicates_match(&step.predicates, msg) {
                            Self::match_steps(rest, rest_topic, msg)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Segment::Plus => {
                if let Some((_first, rest_topic)) = topic.split_first() {
                    if Self::predicates_match(&step.predicates, msg) {
                        Self::match_steps(rest, rest_topic, msg)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Segment::Hash => {
                // Try zero or more segments
                if Self::predicates_match(&step.predicates, msg)
                    && Self::match_steps(rest, topic, msg)
                {
                    return true;
                }
                for idx in 0..topic.len() {
                    if Self::match_steps(rest, &topic[idx + 1..], msg) {
                        return true;
                    }
                }
                false
            }
            Segment::Message => {
                if Self::predicates_match(&step.predicates, msg) {
                    Self::match_steps(rest, topic, msg)
                } else {
                    false
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
                } else if let Some(i) = cur.as_i64() {
                    Some(i as f64)
                } else {
                    None
                }
            }
        }
    }

    fn push_value(&mut self, v: f64) {
        self.window.push(v);
        if self.window.len() > self.window_size {
            self.window.remove(0);
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
