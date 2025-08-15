use moqtail_core::compile as core_compile;
use pyo3::prelude::*;

#[pyfunction]
fn compile(query: &str) -> PyResult<String> {
    core_compile(query)
        .map(|sel| sel.to_string())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pymodule]
fn moqtail_py(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_returns_string() {
        assert_eq!(compile("/foo").unwrap(), "/foo");
    }
}
