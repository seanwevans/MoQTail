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
pub struct Step {
    pub axis: Axis,
    pub segment: Segment,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Selector(pub Vec<Step>);
