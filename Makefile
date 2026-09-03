check:
	cargo fmt --all -- --check
	cargo clippy --workspace --exclude moqtail-js --all-targets -- -D warnings
	cargo test --workspace --exclude moqtail-js
	cargo rustc -p moqtail-mosquitto --crate-type cdylib
	cargo rustc -p moqtail-emqx --crate-type cdylib

.PHONY: check
