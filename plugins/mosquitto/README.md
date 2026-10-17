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

Each `plugin_opt_selector` entry is compiled using `moqtail-core`. Messages that do not match any selector are denied (`MOSQ_ERR_ACL_DENIED`): Mosquitto drops them without disconnecting the publisher, and MQTT 5 publishers see reason code 0x87 (Not authorized) on QoS 1/2. The filter runs once per incoming PUBLISH, so it applies broker-wide, not per subscriber.

The selectors are an allowlist, so the plugin refuses to load (and Mosquitto
refuses to start) if any selector fails to compile or none is configured; the
reason is printed on stderr. Loading anyway would silently drop the traffic the
broken selector was meant to admit.

## What selectors can see

| Field | Source |
| --- | --- |
| Topic | The PUBLISH topic. |
| `qos`, `retained` | The PUBLISH header. |
| `json$…` | The payload, decoded as JSON, or as CBOR when it is not JSON. |

The payload is only decoded when some selector has a `json$` predicate, and the
headers are only collected when some selector reads one, so topic-only
selectors cost nothing beyond the topic match. A payload that is neither JSON
nor CBOR simply fails any `json$` predicate; the first such payload is reported
on stderr, later ones are not.

`dup` and MQTT 5 user properties (`prop.<name>`) are not available here:
Mosquitto 2.0 does not pass the DUP flag to plugins and does not export the
functions for reading message properties.
