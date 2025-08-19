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

#[derive(Debug, PartialEq, Eq, Clone)]
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

// Value can represent numbers, booleans or strings.  The `Str` variant owns a
// `String`, which cannot implement the `Copy` trait.  Deriving `Copy` for this
// enum therefore causes compilation to fail.  We only derive `Clone` to allow
// duplication when needed while keeping the type non-`Copy`.
#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    Number(f64),
    Bool(bool),
    Str(String),
}

#[derive(Debug, PartialEq)]
pub struct Predicate {
    pub field: Field,
    pub op: Operator,
    pub value: Value,
}

#[derive(Debug, PartialEq)]
pub struct Step {
    pub axis: Axis,
    pub segment: Segment,
    pub predicates: Vec<Predicate>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Stage {
    Window(u64),
    Sum(Field),
    Avg(Field),
    Count,
}

#[derive(Debug, PartialEq)]
pub struct Selector {
    pub steps: Vec<Step>,
    pub stages: Vec<Stage>,
}

use std::fmt;

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for step in &self.steps {
            match step.axis {
                Axis::Child => write!(f, "/")?,
                Axis::Descendant => write!(f, "//")?,
            }

            match &step.segment {
                Segment::Literal(s) => write!(f, "{s}")?,
                Segment::Plus => write!(f, "+")?,
                Segment::Hash => write!(f, "#")?,
                Segment::Message => write!(f, "msg")?,
            }

            for pred in &step.predicates {
                write!(
                    f,
                    "[{}{}{}]",
                    display_field(&pred.field),
                    display_op(pred.op),
                    display_value(&pred.value)
                )?;
            }
        }
        for stage in &self.stages {
            match stage {
                Stage::Window(s) => write!(f, " |> window({s}s)")?,
                Stage::Sum(field) => write!(f, " |> sum({})", display_field(field))?,
                Stage::Avg(field) => write!(f, " |> avg({})", display_field(field))?,
                Stage::Count => write!(f, " |> count()")?,
            }
        }
        Ok(())
    }
}

fn display_field(fld: &Field) -> String {
    match fld {
        Field::Header(s) => s.clone(),
        Field::Json(parts) => format!(
            "json${}",
            parts.iter().map(|p| format!(".{p}")).collect::<String>()
        ),
    }
}

fn display_op(op: Operator) -> &'static str {
    match op {
        Operator::Eq => "=",
        Operator::Lt => "<",
        Operator::Gt => ">",
        Operator::Le => "<=",
        Operator::Ge => ">=",
    }
}

fn display_value(val: &Value) -> String {
    match val {
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => serde_json::to_string(s).unwrap(),
    }
}
