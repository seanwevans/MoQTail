//! Core library for MoQtail

pub mod ast;
mod parser;

pub use parser::compile;

pub fn hello() -> &'static str {
    "Hello, MoQtail!"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_selectors() {
        assert!(compile("/foo/bar").is_ok());
        assert!(compile("//sensor").is_ok());
        assert!(compile("/+/#").is_ok());
    }

    #[test]
    fn invalid_selectors() {
        assert!(compile("foo/bar").is_err());
        assert!(compile("/foo//").is_err());
        assert!(compile("/fo$" ).is_err());
    }
}
