# MoQTail EMQX Plugin

This plugin embeds the MoQTail selector engine in EMQX through the broker's
extension API. `moqtail_init` compiles a list of selectors and registers a
`message_publish` hook; the hook returns success for messages that match a
selector and a non-zero code for the rest.

## Build Dependencies

No broker headers or SDK are required. The two `emqx_extension_*` functions the
plugin calls are declared in `src/lib.rs` and stay undefined until EMQX
`dlopen()`s the library.

## Building

The crate is a member of the workspace in the repository root, so build it by
package name from anywhere in the tree. It declares `crate-type = ["rlib"]`, so
the loadable library is asked for explicitly:

```bash
$ cargo rustc -p moqtail-emqx --release --crate-type cdylib
```

That leaves `libmoqtail_emqx.so` in the workspace's `target/release/`. Because
the broker's symbols are resolved at load time, that step works on Linux and
not on macOS or Windows.

## Entry Points

| Symbol | Called when | Contract |
| --- | --- | --- |
| `moqtail_init(selectors, count)` | The plugin is loaded | Takes `count` NUL-terminated selector strings, registers the publish hook, and returns an owned context pointer (null if `selectors` is null while `count` is non-zero). |
| `moqtail_deinit(ctx)` | The plugin is unloaded | Unregisters the hook and frees the context. Must be called at most once per `moqtail_init`. |

Selectors that fail to compile are reported on stderr and skipped, so the plugin
still loads with whatever selectors did compile.

## Status

This plugin matches on the publish topic only — the `EmqxMessage` shim carries
the topic and ignores the broker's other fields, so header and payload
predicates do not apply to it yet. Broker plugin work is tracked under v0.3 in
[`docs/ROADMAP.md`](../../docs/ROADMAP.md).
