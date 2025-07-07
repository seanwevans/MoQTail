use crate::ast::{Axis, Segment, Selector, Step};

pub struct Matcher {
    selector: Selector,
}

impl Matcher {
    pub fn new(selector: Selector) -> Self {
        Self { selector }
    }

    pub fn matches(&self, topic: &str) -> bool {
        let segments: Vec<&str> = if topic.is_empty() {
            Vec::new()
        } else {
            topic.split('/').collect()
        };
        Self::match_steps(&self.selector.0, &segments)
    }

    fn match_steps(steps: &[Step], topic: &[&str]) -> bool {
        if steps.is_empty() {
            return topic.is_empty();
        }
        let step = &steps[0];
        match step.axis {
            Axis::Child => Self::match_child(step, &steps[1..], topic),
            Axis::Descendant => {
                // try to match at current or any subsequent position
                for idx in 0..=topic.len() {
                    if Self::match_child(step, &steps[1..], &topic[idx..]) {
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

    fn match_child(step: &Step, rest: &[Step], topic: &[&str]) -> bool {
        match step.segment {
            Segment::Literal(ref lit) => {
                if let Some((first, rest_topic)) = topic.split_first() {
                    if lit == first {
                        Self::match_steps(rest, rest_topic)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Segment::Plus => {
                if let Some((_first, rest_topic)) = topic.split_first() {
                    Self::match_steps(rest, rest_topic)
                } else {
                    false
                }
            }
            Segment::Hash => {
                // Try zero or more segments
                if Self::match_steps(rest, topic) {
                    return true;
                }
                for idx in 0..topic.len() {
                    if Self::match_steps(rest, &topic[idx + 1..]) {
                        return true;
                    }
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::compile;

    #[test]
    fn simple_match() {
        let sel = compile("/foo/bar").unwrap();
        let m = Matcher::new(sel);
        assert!(m.matches("foo/bar"));
        assert!(!m.matches("foo/baz"));
    }

    #[test]
    fn plus_wildcard() {
        let sel = compile("/foo/+").unwrap();
        let m = Matcher::new(sel);
        assert!(m.matches("foo/bar"));
        assert!(m.matches("foo/baz"));
        assert!(!m.matches("foo"));
    }

    #[test]
    fn hash_wildcard() {
        let sel = compile("/foo/#").unwrap();
        let m = Matcher::new(sel);
        assert!(m.matches("foo"));
        assert!(m.matches("foo/bar/baz"));
    }

    #[test]
    fn descendant_axis() {
        let sel = compile("//sensor").unwrap();
        let m = Matcher::new(sel);
        assert!(m.matches("building/floor/sensor"));
        assert!(!m.matches("building/floor/actuator"));
    }
}
