# moqtail-core

The selector engine behind [MoQTail](https://github.com/seanwevans/MoQTail) —
an XPath-inspired DSL for querying, filtering and transforming MQTT traffic
without touching the MQTT wire protocol.

This crate is the DSL parser, the AST, and the matcher. It has no MQTT client
and no I/O: you hand it a compiled selector and a message, and it tells you
whether the message matches. That makes it usable from a client, from a broker
plugin, or from anything else that can produce a topic, some headers and a
payload.

```toml
[dependencies]
moqtail-core = "0.1"
```

## Matching a message

```rust
use moqtail_core::{compile, Matcher, Message};
use std::borrow::Cow;
use std::collections::HashMap;

let selector = compile("//sensor[json$.value>30]")?;
let matcher = Matcher::new(selector);

let mut headers = HashMap::new();
headers.insert(Cow::Borrowed("qos"), Cow::Borrowed("1"));

let msg = Message {
    topic: "building/e/sensor",
    headers,
    payload: Some(serde_json::json!({ "value": 31.5 })),
};

assert!(matcher.matches(&msg));
# Ok::<(), Box<dyn std::error::Error>>(())
```

## What the DSL covers today

* Topic steps with the child (`/`) and descendant (`//`) axes, literal
  segments, and the MQTT wildcards `+` and `#`.
* Header and property predicates: `/msg[qos<=1][retained=true]`.
* Payload predicates over a `json$` path: `/device[json$.status="online"]`.
  Repeated predicates conjoin.
* Post-match pipeline stages — `window`, `sum`, `avg`, `count` — evaluated by
  `Matcher::process`.

Numeric comparisons use a hybrid absolute/relative tolerance, so values that
survived a float round-trip still compare equal.

The grammar is still moving. See
[`SPEC.md`](https://github.com/seanwevans/MoQTail/blob/main/SPEC.md) for the
current definition and [`docs/`](https://github.com/seanwevans/MoQTail/tree/main/docs)
for the user guide.

## Licence

Dual-licensed under MIT or Apache-2.0, at your option.
