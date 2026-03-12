//! Declaration-level source AST nodes.

use super::span::{Span, Spanned};
use super::stmt::Block;
use super::ty::Type;

/// Item/member modifier set currently supported by source grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Pub,
    Async,
}

/// Source-level attribute.
///
/// Attribute arguments are preserved in source-oriented form and are not
/// expanded or semantically interpreted at AST construction time.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Attribute {
    pub name: String,
    pub args: AttributeArgs,
}

/// Source-preserving attribute argument forms.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttributeArgs {
    None,
    Paren { raw: String },
    Braced { raw: String },
}

/// Source-preserving doc-comment form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocCommentKind {
    OuterLine,
    OuterBlock,
    InnerLine,
    InnerBlock,
}

/// Source-level doc comment attached to declarations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocComment {
    pub kind: DocCommentKind,
    pub span: Span,
    pub text: String,
}

/// Receiver syntax for method/initializer declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReceiverKind {
    /// `self`
    Owned,
    /// `&self`
    Ref,
    /// `&mut self`
    MutRef,
}

/// External label form for parameter declarations.
///
/// This preserves source shape for:
/// - `x: T`
/// - `_ x: T`
/// - `label x: T`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParamLabel {
    None,
    Underscore,
    Named(String),
}

/// Function/initializer parameter declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamDecl {
    pub label: ParamLabel,
    pub name: String,
    pub ty: Spanned<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParam {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhereClause {
    pub predicates: Vec<Spanned<WherePredicate>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WherePredicate {
    pub ty: Spanned<Type>,
    pub bounds: Vec<Spanned<Type>>,
}

/// Source function declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub name: String,
    pub generic_params: Vec<Spanned<GenericParam>>,
    pub receiver: Option<Spanned<ReceiverKind>>,
    pub params: Vec<Spanned<ParamDecl>>,
    pub return_type: Option<Spanned<Type>>,
    pub where_clause: Option<Spanned<WhereClause>>,
    pub body: Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InitKind {
    Plain,
    Optional,
    Fallible,
}

/// Source initializer declaration (`init`, `init?`, `init!`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InitDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub kind: InitKind,
    pub receiver: Option<Spanned<ReceiverKind>>,
    pub params: Vec<Spanned<ParamDecl>>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructField {
    pub docs: Vec<Spanned<DocComment>>,
    /// Source attributes attached to this field declaration.
    pub attributes: Vec<Spanned<Attribute>>,
    pub name: String,
    pub ty: Spanned<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StructMember {
    Field(Spanned<StructField>),
    Init(Spanned<InitDecl>),
    Function(Spanned<FunctionDecl>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub name: String,
    pub generic_params: Vec<Spanned<GenericParam>>,
    pub members: Vec<Spanned<StructMember>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnumCaseParam {
    Unnamed(Spanned<Type>),
    Named { name: String, ty: Spanned<Type> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumCase {
    pub docs: Vec<Spanned<DocComment>>,
    /// Source attributes attached to this enum case declaration.
    pub attributes: Vec<Spanned<Attribute>>,
    pub name: String,
    pub payload: Vec<Spanned<EnumCaseParam>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnumMember {
    Case(Spanned<EnumCase>),
    Init(Spanned<InitDecl>),
    Function(Spanned<FunctionDecl>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub name: String,
    pub generic_params: Vec<Spanned<GenericParam>>,
    pub members: Vec<Spanned<EnumMember>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImplMember {
    Init(Spanned<InitDecl>),
    Function(Spanned<FunctionDecl>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImplDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub target: Spanned<Type>,
    pub conformance: Option<Spanned<Type>>,
    pub members: Vec<Spanned<ImplMember>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingKind {
    Let,
    Var,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessorRequirement {
    Get,
    Set,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssociatedTypeDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub name: String,
    pub bounds: Vec<Spanned<Type>>,
}

/// Protocol property requirement preserving `let`/`var` and accessor contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolPropertyRequirement {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub binding: BindingKind,
    pub name: String,
    pub ty: Spanned<Type>,
    pub accessors: Vec<AccessorRequirement>,
}

/// Protocol function requirement with optional default implementation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolFunctionMember {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub name: String,
    pub generic_params: Vec<Spanned<GenericParam>>,
    pub receiver: Option<Spanned<ReceiverKind>>,
    pub params: Vec<Spanned<ParamDecl>>,
    pub return_type: Option<Spanned<Type>>,
    pub where_clause: Option<Spanned<WhereClause>>,
    /// `None` means requirement-only (`;` form).
    pub default_body: Option<Block>,
}

/// Protocol initializer requirement with optional default implementation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolInitMember {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub kind: InitKind,
    pub receiver: Option<Spanned<ReceiverKind>>,
    pub params: Vec<Spanned<ParamDecl>>,
    /// `None` means requirement-only (`;` form).
    pub default_body: Option<Block>,
}

/// Protocol member variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProtocolMember {
    Function(Spanned<ProtocolFunctionMember>),
    Initializer(Spanned<ProtocolInitMember>),
    AssociatedType(Spanned<AssociatedTypeDecl>),
    Property(Spanned<ProtocolPropertyRequirement>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub name: String,
    pub generic_params: Vec<Spanned<GenericParam>>,
    pub inheritance: Vec<Spanned<Type>>,
    pub members: Vec<Spanned<ProtocolMember>>,
}

/// Foreign import block with symbolic library name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternBlock {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub library_name: String,
    pub members: Vec<Spanned<ExternMember>>,
}

/// Foreign function import declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternFunctionDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub local_name: String,
    /// When present, this is the native symbol name used for resolution.
    /// When absent, native symbol name is `local_name`.
    pub native_symbol: Option<String>,
    pub params: Vec<Spanned<ParamDecl>>,
    pub return_type: Option<Spanned<Type>>,
}

/// Extern members are currently restricted to foreign function declarations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternMember {
    Function(Spanned<ExternFunctionDecl>),
}

/// Source use declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UseItem {
    pub tree: Spanned<UseTree>,
}

/// Source use-tree representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UseTree {
    Name(String),
    SelfValue,
    Group(Vec<Spanned<UseTree>>),
    Path {
        head: String,
        tail: Box<Spanned<UseTree>>,
    },
}
