check:
	cargo fmt --all -- --check
	cargo clippy --workspace --exclude moqtail-js --all-targets -- -D warnings
	cargo test --workspace --exclude moqtail-js

.PHONY: check
