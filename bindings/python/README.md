# Python bindings for MoQTail

This package exposes a thin wrapper around `moqtail-core` using [PyO3](https://pyo3.rs/).

## Building

1. Install [maturin](https://github.com/PyO3/maturin):
   ```bash
   pip install maturin
   ```
2. Build and install the extension module:
   ```bash
   maturin develop --release
   ```

This will compile the Rust code and make the `moqtail_py` module available in your current Python environment.
