use moqtail_core::{compile as core_compile, hello as core_hello};
use napi::Error;
use napi_derive::napi;

#[napi]
fn compile(query: String) -> Result<String, Error> {
    core_compile(&query)
        .map(|sel| sel.to_string())
        .map_err(|e| Error::from_reason(e.to_string()))
}

#[napi]
fn hello() -> &'static str {
    core_hello()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_returns_string() {
        assert_eq!(compile("/foo".into()).unwrap(), "/foo");
    }

    #[test]
    fn hello_returns_greeting() {
        assert_eq!(hello(), "Hello, MoQtail!");
    }
}
