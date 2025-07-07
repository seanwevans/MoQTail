use pest::Parser;
use pest_derive::Parser;

use crate::ast::{Axis, Predicate, Segment, Selector, Step};

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
            Rule::ident => Segment::Literal(segment_inner.as_str().to_string()),
            _ => unreachable!(),
        };

        let mut predicates = Vec::new();
        for pred_pair in inner {
            if pred_pair.as_rule() != Rule::predicate {
                continue;
            }
            let mut pred_inner = pred_pair.into_inner();
            let name = pred_inner.next().unwrap().as_str().to_string();
            let value = pred_inner.next().unwrap().as_str().to_string();
            predicates.push(Predicate::Equals { name, value });
        }

        steps.push(Step {
            axis,
            segment,
            predicates,
        });
    }

    Ok(Selector(steps))
}
