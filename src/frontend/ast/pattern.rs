//! Source patterns used by bindings and match arms.
//!
//! Pattern surface is source-oriented and supports binding, literal, tuple,
//! variant, struct, and array pattern forms.

use super::span::Spanned;

/// Source pattern representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern {
    Identifier(String),
    Wildcard,
    IntegerLiteral(String),
    BooleanLiteral(bool),
    CharLiteral(String),
    StringLiteral(String),
    Tuple(Vec<Spanned<Pattern>>),
    Variant {
        path: Vec<String>,
        shorthand: bool,
        args: Vec<Spanned<Pattern>>,
        has_rest: bool,
    },
    Struct {
        path: Vec<String>,
        fields: Vec<StructPatternField>,
        has_rest: bool,
    },
    Array {
        elements: Vec<Spanned<Pattern>>,
        rest: Option<ArrayPatternRest>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructPatternField {
    pub name: String,
    pub pattern: Option<Spanned<Pattern>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArrayPatternRest {
    Ignore,
    Bind(String),
}
