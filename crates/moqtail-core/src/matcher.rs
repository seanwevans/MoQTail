use crate::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Stage, Step, Value};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, VecDeque};

pub struct Message<'a> {
    pub topic: &'a str,
    pub headers: HashMap<String, String>,
    pub payload: Option<JsonValue>,
}

enum StageState {
    Window { size: usize, values: VecDeque<f64> },
    Counter { size: usize, values: VecDeque<()> },
}

pub struct Matcher {
    selector: Selector,
    stage_states: Vec<StageState>,
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
                        values: VecDeque::new(),
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
                    if let StageState::Counter { size, values } = &mut self.stage_states[state_idx]
                    {
                        values.push_back(());
                        if values.len() > *size {
                            values.pop_front();
                        }
                        result = Some(values.len() as f64);
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
                let hv = match msg.headers.get(name) {
                    Some(v) => v,
                    None => return false,
                };
                if let Ok(num) = hv.parse::<i64>() {
                    Value::Number(num)
                } else if hv == "true" || hv == "false" {
                    Value::Bool(hv == "true")
                } else {
                    Value::Str(hv.clone())
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
                } else if let Some(s) = cur.as_str() {
                    Value::Str(s.to_string())
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
            (Value::Str(l), Value::Str(r)) => match op {
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
