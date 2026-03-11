//! Source type AST nodes.

use super::span::Spanned;

/// Source type syntax.
///
/// Builtin primitive names are represented by `Type::Named` and recognized as
/// builtins during semantic analysis rather than by dedicated AST variants.
///
/// Raw pointer source syntax is `*T` and `*mut T` (no `*const T` surface).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Named {
        segments: Vec<String>,
    },
    GenericApplication {
        base: Box<Spanned<Type>>,
        args: Vec<Spanned<Type>>,
    },
    SelfType,
    Reference(Box<Spanned<Type>>),
    MutableReference(Box<Spanned<Type>>),
    /// Pointer from source `*T` (immutable/read-only pointee form).
    ConstPointer(Box<Spanned<Type>>),
    MutablePointer(Box<Spanned<Type>>),
    Array(Box<Spanned<Type>>),
    Optional(Box<Spanned<Type>>),
    Result {
        ok: Box<Spanned<Type>>,
        err: Box<Spanned<Type>>,
    },
    Grouped(Box<Spanned<Type>>),
}
