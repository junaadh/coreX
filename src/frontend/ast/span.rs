//! Span-aware wrappers for source AST nodes.

pub use crate::frontend::lexer::Span;

/// AST wrapper that attaches source span information to a node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    #[must_use]
    pub const fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}
