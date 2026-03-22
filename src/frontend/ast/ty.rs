//! Source type AST nodes.

use super::span::{Span, Spanned};

/// A lifetime annotation like `'a` or `'static`.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Lifetime {
    pub name: String,
    pub span: Span,
}

/// Source type syntax.
///
/// Builtin primitive names are represented by `Type::Named` and recognized as
/// builtins during semantic analysis rather than by dedicated AST variants.
///
/// Raw pointer source syntax is `*T` and `*mut T` (no `*const T` surface).
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Named {
        segments: Vec<String>,
    },
    Lifetime(Lifetime),
    GenericApplication {
        base: Box<Spanned<Type>>,
        args: Vec<Spanned<Type>>,
    },
    SelfType,
    Reference {
        lifetime: Option<Lifetime>,
        inner: Box<Spanned<Type>>,
    },
    MutableReference {
        lifetime: Option<Lifetime>,
        inner: Box<Spanned<Type>>,
    },
    /// Pointer from source `*T` (immutable/read-only pointee form).
    ///
    /// Variant name is legacy and corresponds to source `*T`, not `*const T`.
    ConstPointer(Box<Spanned<Type>>),
    MutablePointer(Box<Spanned<Type>>),
    Array(Box<Spanned<Type>>),
    Optional(Box<Spanned<Type>>),
    Result {
        ok: Box<Spanned<Type>>,
        err: Box<Spanned<Type>>,
    },
    Tuple(Vec<Spanned<Type>>),
}
