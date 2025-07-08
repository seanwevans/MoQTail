# MoQTail Mosquitto Plugin

This plugin integrates the MoQTail selector engine into the Mosquitto broker. It parses one or more `plugin_opt_selector` options and filters publish events before they reach subscribing clients.

## System Dependencies

The build links against Mosquitto's C library. Make sure the development
headers are installed before compiling:

```bash
sudo apt-get install libmosquitto-dev
```

Without these headers the plugin cannot be built.

## Building

```bash
$ cargo build --manifest-path plugins/mosquitto/Cargo.toml --release
```

The resulting `libmoqtail_mosquitto.so` can be loaded by Mosquitto.
For example:

```bash
$ cargo build --manifest-path plugins/mosquitto/Cargo.toml --release
$ sudo cp plugins/mosquitto/target/release/libmoqtail_mosquitto.so /usr/lib/
```

## Example Configuration

```conf
# mosquitto.conf
plugin /usr/lib/libmoqtail_mosquitto.so
plugin_opt_selector /foo/+
plugin_opt_selector //sensor/#
```

Each `plugin_opt_selector` entry is compiled using `moqtail-core`. Messages that do not match any selector are dropped before being routed to clients.
