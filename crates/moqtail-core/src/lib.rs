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

/// Placeholder compile function until the real parser exists.
///
/// Takes a query string and returns a representation of the compiled
/// selector. For now this simply prefixes the query with `"compiled: "`.
pub fn compile(query: &str) -> String {
    format!("compiled: {}", query)
}

}
