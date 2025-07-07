# xtask

This crate contains small developer utilities used while working on MoQTail.

## Running

Invoke commands from the workspace root with `cargo run`:

```bash
cargo run -p xtask -- <command>
```

### Commands

- `repo-graph` – print dependency edges between crates in this workspace.
  External dependencies are filtered out so only workspace relationships are shown.
