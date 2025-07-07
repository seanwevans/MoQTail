# MoQTail Mosquitto Plugin

This plugin integrates the MoQTail selector engine into the Mosquitto broker. It parses one or more `plugin_opt_selector` options and filters publish events before they reach subscribing clients.

## Building

```bash
$ cargo build --manifest-path plugins/mosquitto/Cargo.toml --release
```

The resulting `libmoqtail_mosquitto.so` can be loaded by Mosquitto.

## Example Configuration

```conf
# mosquitto.conf
plugin /path/to/libmoqtail_mosquitto.so
plugin_opt_selector /foo/+
plugin_opt_selector //sensor/#
```

Each `plugin_opt_selector` entry is compiled using `moqtail-core`. Messages that do not match any selector are dropped before being routed to clients.
