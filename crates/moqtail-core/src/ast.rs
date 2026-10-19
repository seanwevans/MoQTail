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
    Window(Duration),
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
use std::time::Duration;

impl Selector {
    /// Every field the selector reads, from step predicates and pipeline
    /// stages alike.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        let predicates = self
            .steps
            .iter()
            .flat_map(|step| step.predicates.iter().map(|p| &p.field));
        let stages = self.stages.iter().filter_map(|stage| match stage {
            Stage::Sum(field) | Stage::Avg(field) => Some(field),
            Stage::Window(_) | Stage::Count => None,
        });
        predicates.chain(stages)
    }

    /// Whether evaluating this selector reads the message payload. When it
    /// does not, callers can skip decoding the payload entirely.
    pub fn needs_payload(&self) -> bool {
        self.fields().any(|f| matches!(f, Field::Json(_)))
    }

    /// The narrowest plain MQTT topic filter that receives every message this
    /// selector can match, for subscribing through a broker that does not
    /// understand selectors and then filtering client-side.
    ///
    /// Leading child steps carry over level by level (`+` stays `+`, `/msg`
    /// consumes no level). The first `//` or `#` ends the filter with `#`,
    /// since everything after it can span any number of levels. Predicates
    /// have no MQTT equivalent and are left to the client.
    ///
    /// ```
    /// use moqtail_core::compile;
    /// let filter = |s| compile(s).unwrap().subscription_filter();
    /// assert_eq!(filter("/building/+/sensor[json$.temp>30]"), "building/+/sensor");
    /// assert_eq!(filter("/msg[qos<=1]/building/#"), "building/#");
    /// assert_eq!(filter("/ship//engine"), "ship/#");
    /// assert_eq!(filter("//sensor"), "#");
    /// ```
    pub fn subscription_filter(&self) -> String {
        let mut levels: Vec<&str> = Vec::new();
        for step in &self.steps {
            if step.axis == Axis::Descendant {
                levels.push("#");
                break;
            }
            match &step.segment {
                Segment::Literal(lit) => levels.push(lit),
                Segment::Plus => levels.push("+"),
                Segment::Hash => {
                    levels.push("#");
                    break;
                }
                Segment::Message => {}
            }
        }
        if levels.is_empty() {
            // Only `/msg` steps: the selector matches the empty topic, which
            // MQTT has no filter for. Receive everything; nothing will match.
            return "#".to_owned();
        }
        levels.join("/")
    }
}

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
                Stage::Window(duration) => write!(f, " |> window({}s)", duration.as_secs())?,
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
        Value::Str(s) => serde_json::to_string(s).expect("string serialization cannot fail"),
    }
}
