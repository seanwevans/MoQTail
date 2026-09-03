# CBOR Payload Selectors

`json$` predicates are not tied to the JSON wire format. The matcher evaluates
them against a generic value tree, and `moqtail_core::payload` projects a
payload onto that tree. CBOR (RFC 8949) payloads are therefore queried with the
selectors you already know:

```rust
use moqtail_core::{compile, payload, Matcher, Message};
use std::collections::HashMap;

let matcher = Matcher::new(compile("//sensor[json$.value>30]")?);
let msg = Message {
    topic: "site/sensor",
    headers: HashMap::new(),
    payload: Some(payload::from_cbor(bytes)?),
};
assert!(matcher.matches(&msg));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Decoding lives behind the `cbor` Cargo feature, which is on by default. Turn it
off with `--no-default-features` if you only handle JSON and want to drop the
`ciborium` dependency.

## How CBOR maps onto the value tree

| CBOR | Value tree | Notes |
| --- | --- | --- |
| Integer | Number | Rejected rather than rounded if it falls outside what can be held exactly. CBOR's range runs to −2^64, past `i64::MIN`. |
| Float | Number | `NaN` and the infinities have no JSON form and become null, which no predicate matches. |
| Text string | String | |
| Byte string | Array of byte values | Carried, but selector paths cannot index arrays, so not addressable. |
| Boolean, null | Boolean, null | |
| Tag | The tagged value | The tag number is discarded. |
| Array | Array | |
| Map | Object | See below. |

## Map keys

CBOR allows any value as a map key, while a selector path is a string. Text keys
are used as-is; integer, boolean and null keys take their obvious textual form.
That is what makes the integer-keyed maps common in CBOR profiles reachable — a
SenML record keyed `1` is `json$.1`.

A key with no textual form (an array or a nested map) is an error, and the
payload is rejected rather than silently losing the entry. If a map repeats a key
once projected, the last occurrence wins.
