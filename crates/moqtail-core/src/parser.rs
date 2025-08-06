use pest::Parser;
use pest_derive::Parser;

use crate::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Stage, Step, Value};

#[derive(Debug)]
pub enum Error {
    Pest(pest::error::Error<Rule>),
    Message(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Pest(e) => write!(f, "{e}"),
            Error::Message(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<pest::error::Error<Rule>> for Error {
    fn from(e: pest::error::Error<Rule>) -> Self {
        Error::Pest(e)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Self {
        Error::Message(e.to_string())
    }
}

#[derive(Parser)]
#[grammar = "selector.pest"]
struct SelectorParser;

pub fn compile(input: &str) -> Result<Selector, Error> {
    let mut pairs = SelectorParser::parse(Rule::selector, input)?;
    let pair = pairs
        .next()
        .ok_or_else(|| Error::Message("missing selector".into()))?;
    let mut steps = Vec::new();
    let mut stages = Vec::new();

    for seg in pair.into_inner() {
        match seg.as_rule() {
            Rule::path_segment => {
                let mut inner = seg.into_inner();
                let axis_pair = inner
                    .next()
                    .ok_or_else(|| Error::Message("missing axis".into()))?;
                let segment_pair = inner
                    .next()
                    .ok_or_else(|| Error::Message("missing segment".into()))?;
                let segment_inner = segment_pair
                    .into_inner()
                    .next()
                    .ok_or_else(|| Error::Message("missing segment".into()))?;

                let axis = match axis_pair.as_str() {
                    "/" => Axis::Child,
                    "//" => Axis::Descendant,
                    other => return Err(Error::Message(format!("unknown axis {other}"))),
                };

                let segment = match segment_inner.as_rule() {
                    Rule::wildcard => match segment_inner.as_str() {
                        "+" => Segment::Plus,
                        "#" => Segment::Hash,
                        other => return Err(Error::Message(format!("unknown wildcard {other}"))),
                    },
                    Rule::ident => {
                        let s = segment_inner.as_str();
                        if s == "msg" {
                            Segment::Message
                        } else {
                            Segment::Literal(s.to_string())
                        }
                    }
                    _ => return Err(Error::Message("invalid segment".into())),
                };

                let mut predicates = Vec::new();
                for pred_pair in inner {
                    if pred_pair.as_rule() != Rule::predicate {
                        continue;
                    }
                    let mut pred_inner = pred_pair.into_inner();
                    let field_pair = pred_inner
                        .next()
                        .ok_or_else(|| Error::Message("missing field".into()))?;
                    let inner_field = field_pair
                        .into_inner()
                        .next()
                        .ok_or_else(|| Error::Message("missing field".into()))?;
                    let field = parse_field(inner_field);

                    let op_pair = pred_inner
                        .next()
                        .ok_or_else(|| Error::Message("missing operator".into()))?;
                    let op = match op_pair.as_str() {
                        "=" => Operator::Eq,
                        "<" => Operator::Lt,
                        ">" => Operator::Gt,
                        "<=" => Operator::Le,
                        ">=" => Operator::Ge,
                        other => return Err(Error::Message(format!("unknown operator {other}"))),
                    };

                    let value_pair = pred_inner
                        .next()
                        .ok_or_else(|| Error::Message("missing value".into()))?;
                    let value_inner = value_pair
                        .into_inner()
                        .next()
                        .ok_or_else(|| Error::Message("missing value".into()))?;
                    let value = match value_inner.as_rule() {
                        Rule::number => Value::Number(value_inner.as_str().parse::<i64>()?),
                        Rule::boolean => Value::Bool(value_inner.as_str() == "true"),
                        _ => return Err(Error::Message("invalid value".into())),
                    };

                    predicates.push(Predicate { field, op, value });
                }

                steps.push(Step {
                    axis,
                    segment,
                    predicates,
                });
            }
            Rule::stage => {
                stages.push(parse_stage(seg)?);
            }
            _ => {}
        }
    }

    Ok(Selector { steps, stages })
}

fn parse_field(inner_field: pest::iterators::Pair<Rule>) -> Field {
    match inner_field.as_rule() {
        Rule::ident => Field::Header(inner_field.as_str().to_string()),
        Rule::json_field => {
            let text = inner_field.as_str();
            let without = text.trim_start_matches("json$");
            let parts: Vec<String> = without
                .split('.')
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect();
            Field::Json(parts)
        }
        _ => unreachable!(),
    }
}

fn parse_stage(pair: pest::iterators::Pair<Rule>) -> Result<Stage, Error> {
    let mut inner = pair.into_inner();
    let func_pair = inner
        .next()
        .ok_or_else(|| Error::Message("missing function".into()))?;
    let mut func_inner = func_pair.into_inner();
    let name = func_inner
        .next()
        .ok_or_else(|| Error::Message("missing function name".into()))?
        .as_str();
    let arg = func_inner.next();
    match name {
        "window" => {
            let a = arg.ok_or_else(|| Error::Message("window requires duration".into()))?;
            let mut ai = a.into_inner();
            let num_pair = ai
                .next()
                .ok_or_else(|| Error::Message("window requires duration".into()))?;
            let num = num_pair.as_str().parse::<u64>()?;
            Ok(Stage::Window(num))
        }
        "sum" => {
            let a = arg.ok_or_else(|| Error::Message("sum requires field".into()))?;
            let field_inner = a
                .into_inner()
                .next()
                .ok_or_else(|| Error::Message("sum requires field".into()))?;
            Ok(Stage::Sum(parse_field(field_inner)))
        }
        "avg" => {
            let a = arg.ok_or_else(|| Error::Message("avg requires field".into()))?;
            let field_inner = a
                .into_inner()
                .next()
                .ok_or_else(|| Error::Message("avg requires field".into()))?;
            Ok(Stage::Avg(parse_field(field_inner)))
        }
        "count" => Ok(Stage::Count),
        _ => Err(Error::Message(format!("unknown function {name}"))),
    }
}
