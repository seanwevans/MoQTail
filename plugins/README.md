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

Both plugins are members of the workspace in the repository root, so they share
its `Cargo.lock` and its `moqtail-core` build, and `cargo clippy --workspace` /
`cargo test --workspace` cover them. They are kept out of `default-members`,
so a bare `cargo build` at the root does not build them.

> **Note:** The plugins are early and their FFI surface is not stable. The
> broker plugin work is tracked under v0.3 in
> [`docs/ROADMAP.md`](../docs/ROADMAP.md); expect breaking changes until then.

## Building

Building the shared libraries needs a working C toolchain (the Mosquitto crate
compiles a small C shim through the `cc` crate) but no broker headers: the FFI
declarations are checked in, and the broker's own symbols stay undefined until
it `dlopen()`s the library.

Both crates declare `crate-type = ["rlib"]`, so an ordinary build or test never
links a shared library. Ask for the loadable artifact explicitly:

```bash
$ cargo rustc -p moqtail-mosquitto --release --crate-type cdylib
$ cargo rustc -p moqtail-emqx --release --crate-type cdylib
```

That leaves `libmoqtail_mosquitto.so` and `libmoqtail_emqx.so` in the workspace's
`target/release/`.

Leaving the broker's symbols undefined is only tolerated by the ELF linker, so
that step works on Linux. macOS and Windows still compile, lint and test the
plugin sources — nothing there links the cdylib — but cannot produce the shared
library.

## Testing

```bash
$ cargo test -p moqtail-mosquitto -p moqtail-emqx
```

The tests stub the broker's registration entry points, so they exercise the real
plugin callbacks without a broker running.

See each plugin's own README for configuration and installation:

* [`mosquitto/README.md`](mosquitto/README.md)
* [`emqx/README.md`](emqx/README.md)
