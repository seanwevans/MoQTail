# JSON Payload Selectors

The `json$` axis peeks inside a message payload interpreted as JSON. It uses JSON Pointer style paths so you can match on deeply nested fields.

## Example

```bash
$ moqtail sub "//device[json$.status='online']"
```

This matches messages whose JSON payload has `{ "status": "online" }`.

You can combine multiple expressions:

```bash
$ moqtail sub "//sensor[json$.value > 30][json$.unit='C']"
```

The payload must be valid UTF‑8 JSON for these predicates to apply. The same
predicates work over CBOR payloads — see [CBOR Payload
Selectors](cbor_payload_selectors.md).
