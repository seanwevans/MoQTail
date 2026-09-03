# MoQTail Mosquitto Plugin

This plugin integrates the MoQTail selector engine into the Mosquitto broker. It parses one or more `plugin_opt_selector` options and filters publish events before they reach subscribing clients.

## Build Dependencies

A C toolchain is needed — `build.rs` compiles a small shim through the `cc`
crate. Mosquitto's development headers are **not** required: the handful of
`mosquitto_*` declarations the plugin uses are checked in as
`src/bindings.rs`, and the broker's own symbols stay undefined until it
`dlopen()`s the library.

## Building

The crate is a member of the workspace in the repository root, so build it by
package name from anywhere in the tree. It declares `crate-type = ["rlib"]`, so
the loadable library is asked for explicitly:

```bash
$ cargo rustc -p moqtail-mosquitto --release --crate-type cdylib
```

The resulting `libmoqtail_mosquitto.so` lands in the workspace's
`target/release/` and can be loaded by Mosquitto:

```bash
$ sudo cp target/release/libmoqtail_mosquitto.so /usr/lib/
```

Because the broker's symbols are left to be resolved at load time, that step
works on Linux and not on macOS or Windows.

## Example Configuration

```conf
# mosquitto.conf
plugin /usr/lib/libmoqtail_mosquitto.so
plugin_opt_selector /foo/+
plugin_opt_selector //sensor/#
```

Each `plugin_opt_selector` entry is compiled using `moqtail-core`. Messages that do not match any selector are dropped before being routed to clients.

Selectors that fail to compile are reported on stderr and skipped; the plugin
still loads with whatever selectors did compile. A message whose payload is not
valid JSON is matched on its topic and headers alone.
