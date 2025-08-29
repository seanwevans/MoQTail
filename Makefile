check:
	cargo fmt --all -- --check
	cargo clippy --workspace --exclude moqtail-python --exclude moqtail-js --all-targets -- -D warnings
	cargo test --workspace --exclude moqtail-python --exclude moqtail-js

.PHONY: check
