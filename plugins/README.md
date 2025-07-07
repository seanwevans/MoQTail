# MoQTail Broker Plugins

This directory contains broker-specific integration crates. Each subfolder
implements the glue code needed to embed MoQTail's query engine into a
particular MQTT broker.

MoQTail is broker-agnostic at its core, but most brokers offer a plugin or
extension API that lets us intercept publish/subscribe flows. Plugins compile the
DSL to an efficient matcher and apply it server-side so that legacy clients see
only standard MQTT traffic.

```
plugins/
├── mosquitto/  # C-based loadable module written in Rust
└── emqx/       # Rust NIF wrapping the Erlang extension API
```

The layout is intentionally similar across brokers:

1. **`Cargo.toml`** – Rust crate manifest. Dependencies and build scripts vary per
   broker.
2. **`src/`** – Plugin entry points and any shim code bridging to the broker SDK.
3. **`build.rs`** (optional) – Generates FFI bindings or performs extra steps.

Each plugin is versioned independently but shares the core crates via the
workspace in the repository root (coming in later milestones).

> **Note:** The plugins are currently placeholders pending the v0.2 milestone.
> Expect breaking changes as the APIs stabilise.
