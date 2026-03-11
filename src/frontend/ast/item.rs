//! File and top-level item nodes.

use super::decl::{
    EnumDecl, ExternBlock, FunctionDecl, ImplDecl, ProtocolDecl, StructDecl,
    UseItem,
};
use super::span::Spanned;

/// Parsed source file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct File {
    pub items: Vec<Spanned<Item>>,
}

/// Top-level source items.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    Use(Spanned<UseItem>),
    Struct(Spanned<StructDecl>),
    Enum(Spanned<EnumDecl>),
    Impl(Spanned<ImplDecl>),
    Protocol(Spanned<ProtocolDecl>),
    Function(Spanned<FunctionDecl>),
    ExternBlock(Spanned<ExternBlock>),
}
