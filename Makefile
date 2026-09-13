check: rust sim

rust:
	cargo fmt --all -- --check
	cargo clippy --workspace --exclude moqtail-js --all-targets -- -D warnings
	cargo test --workspace --exclude moqtail-js
	cargo rustc -p moqtail-mosquitto --crate-type cdylib
	cargo rustc -p moqtail-emqx --crate-type cdylib

# Mirrors the `simulator` job in CI. Node resolves a bare directory argument as
# a module rather than a test root, so this runs from `sim/` and lets the test
# runner discover `tests/` itself.
sim:
	cd sim && node --test

.PHONY: check rust sim
