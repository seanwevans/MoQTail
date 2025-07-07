# Python bindings for MoQTail

This package exposes a thin wrapper around `moqtail-core` using [PyO3](https://pyo3.rs/).

## Building

1. Ensure the Python development headers are installed on your system. On
   Debian/Ubuntu you can install them via:
   ```bash
   sudo apt-get install python3-dev
   ```
   If you have multiple Python versions, specify the interpreter for `pyo3` via
   the `PYO3_PYTHON` environment variable:
   ```bash
   export PYO3_PYTHON=$(which python3)
   ```
2. Install [maturin](https://github.com/PyO3/maturin):
   ```bash
   pip install maturin
   ```
3. Build and install the extension module:
   ```bash
   maturin develop --release
   ```

This will compile the Rust code and make the `moqtail_py` module available in
your current Python environment.
