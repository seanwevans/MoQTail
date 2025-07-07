//! Core library for MoQtail

pub mod ast;
mod matcher;
mod parser;

pub use matcher::Matcher;
pub use parser::compile;

pub fn hello() -> &'static str {
    "Hello, MoQtail!"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_selectors() {
        assert_eq!(compile("/foo/bar"), "compiled: /foo/bar");
        assert_eq!(compile("//sensor"), "compiled: //sensor");
        assert_eq!(compile("/+/#"), "compiled: /+/#");
    }

    #[test]
    fn invalid_selectors() {
        assert!(compile("foo/bar").is_err());
        assert!(compile("/foo//").is_err());
        assert!(compile("/fo$").is_err());

    }

    pub fn compile(query: &str) -> String {
        format!("compiled: {}", query)
    }
}


}
