use pest::Parser;
use pest_derive::Parser;

use crate::ast::{Axis, Field, Operator, Predicate, Segment, Selector, Step, Value};

#[derive(Parser)]
#[grammar = "selector.pest"]
struct SelectorParser;

pub fn compile(input: &str) -> Result<Selector, String> {
    let mut pairs = SelectorParser::parse(Rule::selector, input).map_err(|e| e.to_string())?;
    let pair = pairs.next().unwrap();
    let mut steps = Vec::new();

    for seg in pair.into_inner() {
        if seg.as_rule() != Rule::path_segment {
            continue;
        }
        let mut inner = seg.into_inner();
        let axis_pair = inner.next().unwrap();
        let segment_pair = inner.next().unwrap();
        let segment_inner = segment_pair.into_inner().next().unwrap();

        let axis = match axis_pair.as_str() {
            "/" => Axis::Child,
            "//" => Axis::Descendant,
            _ => unreachable!(),
        };

        let segment = match segment_inner.as_rule() {
            Rule::wildcard => match segment_inner.as_str() {
                "+" => Segment::Plus,
                "#" => Segment::Hash,
                _ => unreachable!(),
            },
            Rule::ident => {
                let s = segment_inner.as_str();
                if s == "msg" {
                    Segment::Message
                } else {
                    Segment::Literal(s.to_string())
                }
            }
            _ => unreachable!(),
        };

        let mut predicates = Vec::new();
        for pred_pair in inner {
            if pred_pair.as_rule() != Rule::predicate {
                continue;
            }
            let mut pred_inner = pred_pair.into_inner();
            let field_pair = pred_inner.next().unwrap();
            let inner_field = field_pair.into_inner().next().unwrap();
            let field = match inner_field.as_rule() {
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
            };

            let op_pair = pred_inner.next().unwrap();
            let op = match op_pair.as_str() {
                "=" => Operator::Eq,
                "<" => Operator::Lt,
                ">" => Operator::Gt,
                "<=" => Operator::Le,
                ">=" => Operator::Ge,
                _ => unreachable!(),
            };

            let value_pair = pred_inner.next().unwrap();
            let value_inner = value_pair.into_inner().next().unwrap();
            let value = match value_inner.as_rule() {
                Rule::number => Value::Number(value_inner.as_str().parse().unwrap()),
                Rule::boolean => Value::Bool(value_inner.as_str() == "true"),
                _ => unreachable!(),
            };

            predicates.push(Predicate { field, op, value });
        }

        steps.push(Step {
            axis,
            segment,
            predicates,
        });
    }

    Ok(Selector(steps))
}
