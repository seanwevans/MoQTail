use moqtail_core::compile as core_compile;
use napi::Error;
use napi_derive::napi;

#[napi]
fn compile(query: String) -> Result<String, Error> {
    core_compile(&query)
        .map(|sel| sel.to_string())
        .map_err(|e| Error::from_reason(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_returns_string() {
        assert_eq!(compile("/foo".into()).unwrap(), "/foo");
    }
}
