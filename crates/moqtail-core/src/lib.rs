//! Core library for MoQtail

pub mod ast;
mod parser;



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
        assert_eq!(compile("foo/bar"), "compiled: foo/bar");
        assert_eq!(compile("/foo//"), "compiled: /foo//");
        assert_eq!(compile("/fo$" ), "compiled: /fo$");
    }
}

/// Placeholder compile function until the real parser exists.
///
/// Takes a query string and returns a representation of the compiled
/// selector. For now this simply prefixes the query with `"compiled: "`.
pub fn compile(query: &str) -> String {
    format!("compiled: {}", query)
}
