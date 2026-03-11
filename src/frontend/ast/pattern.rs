//! Source patterns used by bindings and match arms.
//!
//! Pattern surface is intentionally narrow in this scaffold.

/// Source pattern representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern {
    Identifier(String),
    Wildcard,
}
