check:
	cargo fmt --all -- --check
	cargo clippy --workspace --exclude moqtail-python --all-targets -- -D warnings
	cargo test --workspace --exclude moqtail-python

.PHONY: check
