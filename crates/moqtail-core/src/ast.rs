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
    Message,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Field {
    Header(String),
    Json(Vec<String>),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Operator {
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Value {
    Number(i64),
    Bool(bool),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Predicate {
    pub field: Field,
    pub op: Operator,
    pub value: Value,
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
                Segment::Message => write!(f, "msg")?,
            }
        }
        Ok(())
    }
}
