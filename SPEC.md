# MoQTail Roadmap

> **Status:** Living document – milestones evolve with community input

MoQTail’s development is organised into iterative milestones.  Each milestone focuses on a cohesive feature set rather than a specific calendar date.  Work proceeds when previous stages reach maturity and contributor bandwidth allows.  See [`docs/ROADMAP.md`](docs/ROADMAP.md) for the latest timeline.

| Milestone                               | Core theme                                                                   |                                                         |
| --------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------- |
| **Bootstrap (Phase 0)**                 | Namespace, governance, CI, core crates scaffolding                           |                                                         |
| **v0.1 – Topic Selectors**              | Minimal DSL → topic-only matching & CLI demo                                 |                                                         |
| **v0.2 – Header + JSON Predicates**     | Message‑metadata predicates, JSON payload introspection, first broker plugin |                                                         |
| **v0.3 – Transforms & Aggregations**    | Pipeline operator (`|>`), windows, built‑in functions, multi‑broker support |
| **v1.0 – Stable Spec**                  | Grammar freeze, cross‑language libs, production hardening |

---

## Phase 0 — Project Bootstrap *(In progress)*

* **Namespace & Branding**

  * Secure `moqtail` on GitHub, crates.io, npm, Docker Hub.
  * Register `moqtail.dev` (and `.io` if budget permits).
* **Governance**

  * Adopt the Contributor Covenant v2.1 (`CODE_OF_CONDUCT.md`).
  * Create `CONTRIBUTING.md`, `SECURITY.md`, and PR workflow guidelines.
* **Tooling**

  * GitHub Actions CI matrix (Linux/macOS/Windows) with Rust `stable` & `nightly`.
  * Automatic `cargo fmt clippy` gate.
* **Scaffolding**

  * Crate `moqtail-core` (empty lib) + MIT/Apache dual licence.
  * Docs folder with mdBook skeleton.
* **Community**

  * Matrix room `#moqtail:matrix.org` and Discord bridge.
  * Public roadmap (this file) + issue labels (`good-first-issue`, `RFC`, etc.).

---

## v0.1 — Core Topic Selector

### Major Features

1. **DSL Grammar Draft**

   * EBNF covering root step (`/` or `//`), names, `+` & `#` wildcards, and bracket predicates on *topic segments only*.
2. **Parser & AST**

   * Hand‑rolled or `pest`‑based parser that produces a minimal, zero‑alloc AST.
3. **Matcher Engine**

   * Streaming evaluator over MQTT topic strings (UTF‑8).
   * O(1) per‑segment advance for common paths; cache compiled regex‑style NFAs.
4. **CLI Prototype (`moqtail sub`)**

   * Connect to broker, subscribe using computed legacy wildcard(s), post‑filter in client.
5. **Test Suite**

   * Golden‑file corpus of selector → match/no‑match cases.
   * Fuzz harness via `cargo‑fuzz`.

### Deliverables

* `moqtail-core` crate published on crates.io.
* Blog post: “Introducing MoQTail – XPath‑style queries for MQTT topics.”

---

## v0.2 — Header & JSON Predicates

### Major Features

1. **Header / Property Axes**

   * Extend grammar: `/msg[qos <= 1][retained=true]`.
2. **JSON Payload Introspection**

   * Shorthand `json$` pointer axis; simple operators (`=`, `>`, `<`, `contains`).
3. **Mosquitto Plugin (Proof‑of‑Concept)**

   * Rust → C FFI shim; broker performs server‑side filtering for MoQTail‑annotated subscriptions.
4. **Performance Benchmarks**

   * Baseline vs. vanilla Mosquitto with `mosquitto‑pub/sub` driving 100k msg/s.
5. **Docs & Examples**

   * Cookbook recipes (`docs/cookbook/*.md`).

---

## v0.3 — Transform Pipeline & Aggregations

### Major Features

1. **Pipeline Operator (`|>`)**

   * Unix‑inspired chaining: `... |> window(60s) |> avg(json$.value)`.
2. **Windowing**

   * Sliding and tumbling windows; integral state per subscription.
3. **Built‑in Functions**

   * `sum`, `avg`, `min`, `max`, `regex`, `count`, `first`, `last`.
4. **Broker Support Expansion**

   * Plugins for EMQX & HiveMQ (leveraging respective extension SDKs).
5. **Edge Benchmark**

   * Run on Raspberry Pi 5 and ESP32‑S3 (client‑side path only) to measure overhead.

---


## v1.0 — Stable Spec & Cross‑Language Release

### Graduation Criteria

* **Grammar Freeze** — EBNF locked; selector behaviour guaranteed by conformance tests.
* **Cross‑Language Bindings**

  * `moqtail-py`, `moqtail-js` NPM, `moqtail-go`.
* **Hardening & Security**

  * Static analysis (`cargo‑audit`, `rust‑sec‑checker`).
  * Broker plugin sandboxing review.
* **Load Testing**

  * Sustained 1 M msgs/s on 8‑core x86 broker, 5 µs median match latency.
* **Documentation & DX**

  * mdBook site pushed to `docs.moqtail.dev` with search & examples.
  * VS Code extension for syntax highlighting & IntelliSense.
* **Community Expansion**

  * Governance RFC 001 (steering committee).
  * Monthly community calls.

---

## Stretch Goals (Post‑1.0)

| Area                           | Idea                                                                                       |
| ------------------------------ | ------------------------------------------------------------------------------------------ |
| **WASM Runtime**               | Ship filters to constrained edge devices via WebAssembly modules.                          |
| **Grafana Plugin**             | Native MoQTail data source for dashboards.                                                 |
| **Learn‑mode Tooling**         | CLI wizard that suggests selectors by sampling live traffic.                               |
| **GraphQL Bridge**             | Map selectors to GraphQL subscriptions for frontend apps.                                  |
| **Home Assistant Integration** | Auto‑generate selectors for common smart‑home sensor topics.                               |
| **Query Planner**              | Compile multi‑stage selectors into efficient broker push‑down + client post‑filter graphs. |

---

*We’re tracking issues and discussion for each bullet under the corresponding GitHub milestone. Feel free to propose tweaks or new items via PRs or the `#roadmap` channel!*
