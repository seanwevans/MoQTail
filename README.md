# MoQTail

> **MoQTail** — *MQTT‑optimized Query* **Tail**: bring XPath‑level precision to your MQTT streams.

MoQTail is a free, open‑source extension layer that lets publishers and subscribers use a rich, XPath‑inspired DSL to **query, filter, and transform MQTT traffic** — all without changing the core MQTT wire protocol.

---

## Why MoQTail?

| Problem with vanilla MQTT | How MoQTail helps |
| --- | --- |
| Topic filters have only two wildcards (`+`, `#`). Complex hierarchies become unwieldy. | `/building[wing="E" and floor>3]//sensor[@type='temp']` — expressive, readable selectors. |
| Brokers can’t route on message metadata (retained flag, QoS, properties). | Predicate axes over headers & properties: `/msg[retained=true and qos<=1]`. |
| Payload‑aware routing requires an external pipeline. | Dual‑phase selector lets the broker peek into JSON / CBOR / ProtoBuf payload fields. |
| Edge analytics needs separate tooling (Node‑RED/NiFi). | Built‑in functional pipeline (`|> window(60s) |> avg(json$.value)`). |

MoQTail keeps MQTT’s 2‑byte fixed header intact — **zero protocol bloat** — but adds a powerful, broker‑side query engine that low‑power clients can opt into with a single subscription string.

---

## Key Design Principles

1. **Backwards‑compatible** – A MoQTail‑aware client talks to any MQTT 3.1.1/5.0 broker. A legacy client can still subscribe to raw topics.
2. **Minimal overhead** – The query grammar is compiled once and cached; runtime matching adds O(1) per message in common cases.
3. **Modular architecture** – Separate crates / packages:

   * `moqtail-core` – DSL parser, AST, matcher engine
   * `moqtail-broker` – pluggable adapter layer for Mosquitto, EMQX, HiveMQ (others welcome!)
   * `moqtail-cli` – *tail -f* style command‑line client
   * `moqtail-js` / `moqtail-py` – thin client helpers for web & Python apps
4. **FOSS‑friendly** – Dual‑licensed under MIT / Apache 2.0 to play nicely with both hobby and commercial adopters.

---

## Quick Start (work‑in‑progress)

```bash
# 1. Install CLI (placeholder — crates.io / PyPI coming soon)
$ cargo install moqtail-cli

# 2. Subscribe to high‑temperature alerts
$ moqtail sub "//sensor[@type='temp'][json$.value > 30]"
```

> **Note:** The DSL and tooling are still in early design. Expect syntax tweaks!

---

## Roadmap

* [ ] **v0.1**: Minimal XPath‑style selector → topic matcher (no payload introspection).
* [ ] **v0.2**: Header / property predicates, JSON payload introspection.
* [ ] **v0.3**: Transform pipeline (`|>`), aggregation windows, broker plugin for Mosquitto.
* [ ] **v1.0**: Stable grammar spec, full conformance test‑suite, production hardening.

See [`docs/ROADMAP.md`](docs/ROADMAP.md) for granular tasks.

---

## Contributing

We welcome issues, pull requests, and design discussions!  Start with [`CONTRIBUTING.md`](CONTRIBUTING.md) and join the chat on Matrix `#moqtail:matrix.org`.
This project adheres to the [Contributor Covenant](CODE_OF_CONDUCT.md). By participating,
you agree to uphold its guidelines.

---

## License

**MoQTail** is dual‑licensed under either

* MIT License (see `LICENSE`)

---

© 2025 MoQTail Project Authors
