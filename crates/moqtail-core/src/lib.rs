//! Core library for MoQtail

pub fn hello() -> &'static str {
    "Hello, MoQtail!"
}

/// Placeholder compile function until the real parser exists.
///
/// Takes a query string and returns a representation of the compiled
/// selector. For now this simply prefixes the query with `"compiled: "`.
pub fn compile(query: &str) -> String {
    format!("compiled: {}", query)
}
