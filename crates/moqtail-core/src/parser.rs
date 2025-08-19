use pest::Parser;
use pest_derive::Parser;

use crate::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Stage, Step, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Pest(#[from] pest::error::Error<Rule>),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    ParseFloat(#[from] std::num::ParseFloatError),
    #[error("missing selector")]
    MissingSelector,
    #[error("missing axis")]
    MissingAxis,
    #[error("missing segment")]
    MissingSegment,
    #[error("unknown axis {0}")]
    UnknownAxis(String),
    #[error("unknown wildcard {0}")]
    UnknownWildcard(String),
    #[error("invalid segment")]
    InvalidSegment,
    #[error("missing field")]
    MissingField,
    #[error("missing operator")]
    MissingOperator,
    #[error("unknown operator {0}")]
    UnknownOperator(String),
    #[error("missing value")]
    MissingValue,
    #[error("invalid value")]
    InvalidValue,
    #[error("missing function")]
    MissingFunction,
    #[error("missing function name")]
    MissingFunctionName,
    #[error("window requires duration")]
    WindowRequiresDuration,
    #[error("sum requires field")]
    SumRequiresField,
    #[error("avg requires field")]
    AvgRequiresField,
    #[error("unknown function {0}")]
    UnknownFunction(String),
}

#[derive(Parser)]
#[grammar = "selector.pest"]
struct SelectorParser;

pub fn compile(input: &str) -> Result<Selector, Error> {
    let mut pairs = SelectorParser::parse(Rule::selector, input)?;
    let pair = pairs.next().ok_or(Error::MissingSelector)?;
    let mut steps = Vec::new();
    let mut stages = Vec::new();

    for seg in pair.into_inner() {
        match seg.as_rule() {
            Rule::path_segment => {
                let mut inner = seg.into_inner();
                let axis_pair = inner.next().ok_or(Error::MissingAxis)?;
                let segment_pair = inner.next().ok_or(Error::MissingSegment)?;
                let segment_inner = segment_pair
                    .into_inner()
                    .next()
                    .ok_or(Error::MissingSegment)?;

                let axis = match axis_pair.as_str() {
                    "/" => Axis::Child,
                    "//" => Axis::Descendant,
                    other => return Err(Error::UnknownAxis(other.to_string())),
                };

                let segment = match segment_inner.as_rule() {
                    Rule::wildcard => match segment_inner.as_str() {
                        "+" => Segment::Plus,
                        "#" => Segment::Hash,
                        other => return Err(Error::UnknownWildcard(other.to_string())),
                    },
                    Rule::ident => {
                        let s = segment_inner.as_str();
                        if s == "msg" {
                            Segment::Message
                        } else {
                            Segment::Literal(s.to_string())
                        }
                    }
                    _ => return Err(Error::InvalidSegment),
                };

                let mut predicates = Vec::new();
                for pred_pair in inner {
                    if pred_pair.as_rule() != Rule::predicate {
                        continue;
                    }
                    let mut pred_inner = pred_pair.into_inner();
                    let field_pair = pred_inner.next().ok_or(Error::MissingField)?;
                    let inner_field = field_pair.into_inner().next().ok_or(Error::MissingField)?;
                    let field = parse_field(inner_field)?;

                    let op_pair = pred_inner.next().ok_or(Error::MissingOperator)?;
                    let op = match op_pair.as_str() {
                        "=" => Operator::Eq,
                        "<" => Operator::Lt,
                        ">" => Operator::Gt,
                        "<=" => Operator::Le,
                        ">=" => Operator::Ge,
                        other => return Err(Error::UnknownOperator(other.to_string())),
                    };

                    let value_pair = pred_inner.next().ok_or(Error::MissingValue)?;
                    let value_inner = value_pair.into_inner().next().ok_or(Error::MissingValue)?;
                    let value = match value_inner.as_rule() {
                        Rule::number => Value::Number(value_inner.as_str().parse::<f64>()?),
                        Rule::boolean => Value::Bool(value_inner.as_str() == "true"),
                        Rule::string => {
                            let s = value_inner.as_str();
                            Value::Str(s[1..s.len() - 1].to_string())
                        }
                        _ => return Err(Error::InvalidValue),
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

fn parse_field(inner_field: pest::iterators::Pair<Rule>) -> Result<Field, Error> {
    match inner_field.as_rule() {
        Rule::ident => Ok(Field::Header(inner_field.as_str().to_string())),
        Rule::json_field => {
            let text = inner_field.as_str();
            let without = match text.strip_prefix("json$") {
                Some(rest) => rest,
                None => {
                    // this should be unreachable as the grammar guarantees the prefix
                    ""
                }
            };
            let parts: Vec<String> = without
                .split('.')
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect();
            if parts.is_empty() {
                Err(Error::MissingField)
            } else {
                Ok(Field::Json(parts))
            }
        }
        _ => unreachable!(),
    }
}

fn parse_stage(pair: pest::iterators::Pair<Rule>) -> Result<Stage, Error> {
    let mut inner = pair.into_inner();
    let func_pair = inner.next().ok_or(Error::MissingFunction)?;
    let mut func_inner = func_pair.into_inner();
    let name = func_inner
        .next()
        .ok_or(Error::MissingFunctionName)?
        .as_str();
    let arg = func_inner.next();
    match name {
        "window" => {
            let a = arg.ok_or(Error::WindowRequiresDuration)?;
            let mut ai = a.into_inner();
            let num_pair = ai.next().ok_or(Error::WindowRequiresDuration)?;
            let num = num_pair.as_str().parse::<u64>()?;
            Ok(Stage::Window(num))
        }
        "sum" => {
            let a = arg.ok_or(Error::SumRequiresField)?;
            let field_inner = a.into_inner().next().ok_or(Error::SumRequiresField)?;
            Ok(Stage::Sum(parse_field(field_inner)?))
        }
        "avg" => {
            let a = arg.ok_or(Error::AvgRequiresField)?;
            let field_inner = a.into_inner().next().ok_or(Error::AvgRequiresField)?;
            Ok(Stage::Avg(parse_field(field_inner)?))
        }
        "count" => Ok(Stage::Count),
        _ => Err(Error::UnknownFunction(name.to_string())),
    }
}
