//! Decoding message payloads into the value tree the matcher walks.
//!
//! The dual-phase selector evaluates `json$` predicates against a
//! [`serde_json::Value`]. That type is the engine's generic value tree, not a
//! commitment to the JSON wire format: any payload encoding that can be
//! projected onto it can be queried with the same selectors. This module holds
//! those projections.

use serde_json::Value as JsonValue;
use thiserror::Error;

/// Reasons a payload could not be projected onto the matcher's value tree.
#[derive(Debug, Error)]
pub enum PayloadError {
    /// The bytes were not well-formed JSON.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// The bytes were not well-formed CBOR.
    #[cfg(feature = "cbor")]
    #[error("malformed CBOR payload: {0}")]
    Cbor(String),

    /// A CBOR map used a key that has no textual form, such as an array or a
    /// nested map. Selector paths are strings, so such a key is unaddressable.
    #[cfg(feature = "cbor")]
    #[error("CBOR map key is not addressable by a selector path")]
    UnsupportedKey,

    /// A CBOR integer fell outside the `i64`/`u64` range that
    /// [`serde_json::Number`] can hold exactly. Widening it would silently lose
    /// precision, so it is rejected instead.
    #[cfg(feature = "cbor")]
    #[error("CBOR integer {0} is outside the range representable without loss")]
    IntegerOutOfRange(i128),
}

/// Decodes a JSON payload.
///
/// This is a thin wrapper over [`serde_json::from_slice`]; it exists so callers
/// can treat every payload format the same way.
///
/// ```
/// let value = moqtail_core::payload::from_json(br#"{"status":"online"}"#).unwrap();
/// assert_eq!(value["status"], "online");
/// ```
pub fn from_json(bytes: &[u8]) -> Result<JsonValue, PayloadError> {
    Ok(serde_json::from_slice(bytes)?)
}

/// Decodes a CBOR payload (RFC 8949) into the matcher's value tree, so the same
/// `json$` predicates apply to it.
///
/// The projection is defined as follows:
///
/// * Integers become numbers. One outside the range `serde_json` can hold
///   exactly is an [`PayloadError::IntegerOutOfRange`] rather than a silent
///   rounding.
/// * Floats become numbers; `NaN` and the infinities have no JSON form and
///   become null, which no predicate matches.
/// * Byte strings become arrays of byte values. Selector paths cannot index
///   arrays, so these are carried but not addressable.
/// * Tags are transparent: the tagged value is projected and the tag number
///   discarded.
/// * Map keys become object keys. Text keys are used as-is; integer, boolean
///   and null keys take their obvious textual form, which is what makes the
///   integer-keyed maps common in CBOR (SenML and friends) reachable as
///   `json$.1`. Any other key is an [`PayloadError::UnsupportedKey`]. When a
///   map repeats a key after this projection, the last occurrence wins.
///
/// ```
/// // {"status": "online"} in CBOR
/// let bytes = [
///     0xa1, 0x66, b's', b't', b'a', b't', b'u', b's', 0x66, b'o', b'n', b'l', b'i', b'n', b'e',
/// ];
/// let value = moqtail_core::payload::from_cbor(&bytes).unwrap();
/// assert_eq!(value["status"], "online");
/// ```
#[cfg(feature = "cbor")]
pub fn from_cbor(bytes: &[u8]) -> Result<JsonValue, PayloadError> {
    let value: ciborium::value::Value =
        ciborium::from_reader(bytes).map_err(|e| PayloadError::Cbor(e.to_string()))?;
    cbor_to_json(value)
}

#[cfg(feature = "cbor")]
fn cbor_to_json(value: ciborium::value::Value) -> Result<JsonValue, PayloadError> {
    use ciborium::value::Value as Cbor;

    Ok(match value {
        Cbor::Null => JsonValue::Null,
        Cbor::Bool(b) => JsonValue::Bool(b),
        Cbor::Integer(i) => JsonValue::Number(integer_to_number(i128::from(i))?),
        Cbor::Float(f) => match serde_json::Number::from_f64(f) {
            Some(n) => JsonValue::Number(n),
            // NaN and the infinities have no JSON representation. Null keeps
            // the rest of the payload queryable and matches no predicate.
            None => JsonValue::Null,
        },
        Cbor::Text(s) => JsonValue::String(s),
        Cbor::Bytes(b) => JsonValue::Array(b.into_iter().map(JsonValue::from).collect()),
        Cbor::Tag(_, inner) => cbor_to_json(*inner)?,
        Cbor::Array(items) => JsonValue::Array(
            items
                .into_iter()
                .map(cbor_to_json)
                .collect::<Result<_, _>>()?,
        ),
        Cbor::Map(entries) => {
            let mut map = serde_json::Map::with_capacity(entries.len());
            for (key, val) in entries {
                map.insert(map_key(key)?, cbor_to_json(val)?);
            }
            JsonValue::Object(map)
        }
        // `ciborium::value::Value` is non-exhaustive.
        _ => return Err(PayloadError::Cbor("unsupported CBOR value".to_owned())),
    })
}

#[cfg(feature = "cbor")]
fn integer_to_number(i: i128) -> Result<serde_json::Number, PayloadError> {
    if let Ok(n) = i64::try_from(i) {
        Ok(serde_json::Number::from(n))
    } else if let Ok(n) = u64::try_from(i) {
        Ok(serde_json::Number::from(n))
    } else {
        Err(PayloadError::IntegerOutOfRange(i))
    }
}

#[cfg(feature = "cbor")]
fn map_key(key: ciborium::value::Value) -> Result<String, PayloadError> {
    use ciborium::value::Value as Cbor;

    Ok(match key {
        Cbor::Text(s) => s,
        Cbor::Integer(i) => i128::from(i).to_string(),
        Cbor::Bool(b) => b.to_string(),
        Cbor::Null => "null".to_owned(),
        Cbor::Tag(_, inner) => map_key(*inner)?,
        _ => return Err(PayloadError::UnsupportedKey),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip() {
        let value = from_json(br#"{"status":"online","value":31.5}"#).unwrap();
        assert_eq!(value["status"], "online");
        assert_eq!(value["value"], 31.5);
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(from_json(b"{status:}").is_err());
    }

    #[cfg(feature = "cbor")]
    mod cbor {
        use super::*;
        use crate::ast::{Field, Operator, Predicate, Value};
        use crate::{compile, Matcher, Message};
        use ciborium::value::Value as Cbor;
        use std::collections::HashMap;

        fn encode(value: Cbor) -> Vec<u8> {
            let mut bytes = Vec::new();
            ciborium::into_writer(&value, &mut bytes).unwrap();
            bytes
        }

        #[test]
        fn text_keys_and_scalars() {
            let bytes = encode(Cbor::Map(vec![
                (Cbor::Text("status".into()), Cbor::Text("online".into())),
                (Cbor::Text("value".into()), Cbor::Integer(31.into())),
                (Cbor::Text("ok".into()), Cbor::Bool(true)),
                (Cbor::Text("missing".into()), Cbor::Null),
            ]));

            let value = from_cbor(&bytes).unwrap();
            assert_eq!(value["status"], "online");
            assert_eq!(value["value"], 31);
            assert_eq!(value["ok"], true);
            assert!(value["missing"].is_null());
        }

        #[test]
        fn integer_keys_become_addressable_strings() {
            let bytes = encode(Cbor::Map(vec![
                (Cbor::Integer(1.into()), Cbor::Text("urn:dev:ow:1".into())),
                (Cbor::Integer((-2).into()), Cbor::Float(23.5)),
            ]));

            let value = from_cbor(&bytes).unwrap();
            assert_eq!(value["1"], "urn:dev:ow:1");
            assert_eq!(value["-2"], 23.5);
        }

        #[test]
        fn nested_maps_and_arrays() {
            let bytes = encode(Cbor::Map(vec![(
                Cbor::Text("sensor".into()),
                Cbor::Map(vec![(
                    Cbor::Text("readings".into()),
                    Cbor::Array(vec![Cbor::Integer(1.into()), Cbor::Integer(2.into())]),
                )]),
            )]));

            let value = from_cbor(&bytes).unwrap();
            assert_eq!(value["sensor"]["readings"], serde_json::json!([1, 2]));
        }

        #[test]
        fn tags_are_transparent() {
            let bytes = encode(Cbor::Map(vec![(
                Cbor::Text("at".into()),
                // Tag 1: epoch-based date/time.
                Cbor::Tag(1, Box::new(Cbor::Integer(1_700_000_000.into()))),
            )]));

            let value = from_cbor(&bytes).unwrap();
            assert_eq!(value["at"], 1_700_000_000_i64);
        }

        #[test]
        fn byte_strings_become_arrays() {
            let bytes = encode(Cbor::Map(vec![(
                Cbor::Text("raw".into()),
                Cbor::Bytes(vec![0xde, 0xad]),
            )]));

            let value = from_cbor(&bytes).unwrap();
            assert_eq!(value["raw"], serde_json::json!([222, 173]));
        }

        #[test]
        fn non_finite_floats_become_null() {
            let bytes = encode(Cbor::Map(vec![(
                Cbor::Text("value".into()),
                Cbor::Float(f64::NAN),
            )]));

            let value = from_cbor(&bytes).unwrap();
            assert!(value["value"].is_null());
        }

        #[test]
        fn oversized_integers_are_rejected_rather_than_rounded() {
            let bytes = encode(Cbor::Map(vec![(
                Cbor::Text("value".into()),
                // CBOR's integer range runs to -2^64, well past `i64::MIN`.
                Cbor::Integer(ciborium::value::Integer::try_from(-i128::from(u64::MAX)).unwrap()),
            )]));

            assert!(matches!(
                from_cbor(&bytes),
                Err(PayloadError::IntegerOutOfRange(_))
            ));
        }

        #[test]
        fn unaddressable_keys_are_rejected() {
            let bytes = encode(Cbor::Map(vec![(
                Cbor::Array(vec![Cbor::Integer(1.into())]),
                Cbor::Bool(true),
            )]));

            assert!(matches!(
                from_cbor(&bytes),
                Err(PayloadError::UnsupportedKey)
            ));
        }

        #[test]
        fn malformed_cbor_is_an_error() {
            // 0xa1 announces a one-entry map, then the input stops.
            assert!(matches!(from_cbor(&[0xa1]), Err(PayloadError::Cbor(_))));
        }

        #[test]
        fn selectors_match_cbor_payloads() {
            let bytes = encode(Cbor::Map(vec![
                (Cbor::Text("status".into()), Cbor::Text("online".into())),
                (Cbor::Text("value".into()), Cbor::Float(31.5)),
            ]));

            let matcher = Matcher::new(compile("//sensor[json$.value>30]").unwrap());
            let msg = Message {
                topic: "site/sensor",
                headers: HashMap::new(),
                payload: Some(from_cbor(&bytes).unwrap()),
            };
            assert!(matcher.matches(&msg));

            let matcher = Matcher::new(compile("//sensor[json$.value>40]").unwrap());
            assert!(!matcher.matches(&msg));
        }

        #[test]
        fn integer_keyed_cbor_is_reachable_from_a_predicate() {
            let bytes = encode(Cbor::Map(vec![(
                Cbor::Integer(1.into()),
                Cbor::Text("urn:dev:ow:1".into()),
            )]));

            // `json$.1` is not spellable in the surface grammar yet, so build
            // the predicate directly to pin the projection down.
            let selector = crate::ast::Selector {
                steps: vec![crate::ast::Step {
                    axis: crate::ast::Axis::Descendant,
                    segment: crate::ast::Segment::Literal("sensor".to_owned()),
                    predicates: vec![Predicate {
                        field: Field::Json(vec!["1".to_owned()]),
                        op: Operator::Eq,
                        value: Value::Str("urn:dev:ow:1".to_owned()),
                    }],
                }],
                stages: Vec::new(),
            };

            let matcher = Matcher::new(selector);
            let msg = Message {
                topic: "site/sensor",
                headers: HashMap::new(),
                payload: Some(from_cbor(&bytes).unwrap()),
            };
            assert!(matcher.matches(&msg));
        }
    }
}
