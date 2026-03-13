//! File and top-level item nodes.

use super::decl::{
    EnumDecl, ExternBlock, FunctionDecl, ImplDecl, ProtocolDecl, ScopeDecl,
    StructDecl, UseItem,
};
use super::span::Spanned;

/// Parsed source file.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct File {
    pub items: Vec<Spanned<Item>>,
}

/// Top-level source items.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    Use(Spanned<UseItem>),
    Scope(Spanned<ScopeDecl>),
    Struct(Spanned<StructDecl>),
    Enum(Spanned<EnumDecl>),
    Impl(Spanned<ImplDecl>),
    Protocol(Spanned<ProtocolDecl>),
    Function(Spanned<FunctionDecl>),
    ExternBlock(Spanned<ExternBlock>),
}
