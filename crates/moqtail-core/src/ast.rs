#[derive(Debug, PartialEq, Eq)]
pub enum Axis {
    Child,
    Descendant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Segment {
    Literal(String),
    Plus,
    Hash,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Predicate {
    Equals { name: String, value: String },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Step {
    pub axis: Axis,
    pub segment: Segment,
    pub predicates: Vec<Predicate>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Selector(pub Vec<Step>);

use std::fmt;

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for step in &self.0 {
            match step.axis {
                Axis::Child => write!(f, "/")?,
                Axis::Descendant => write!(f, "//")?,
            }

            match &step.segment {
                Segment::Literal(s) => write!(f, "{}", s)?,
                Segment::Plus => write!(f, "+")?,
                Segment::Hash => write!(f, "#")?,
            }
        }
        Ok(())
    }
}
