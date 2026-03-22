//! Declaration-level source AST nodes.

use super::span::{Span, Spanned};
use super::stmt::Block;
use super::ty::Type;
use crate::frontend::lexer::Token;

/// Item/member modifier set currently supported by source grammar.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Async,
    Unsafe,
}

/// Source visibility surface captured by the parser.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    Public,
    PublicSuper,
    PublicProject,
}

/// Source-level attribute.
///
/// Attribute arguments are preserved in source-oriented form and are not
/// expanded or semantically interpreted at AST construction time.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Attribute {
    pub name: String,
    pub args: AttributeArgs,
}

/// Source-preserving attribute argument forms.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttributeArgs {
    None,
    Paren { raw: String },
    Braced { raw: String },
}

/// Source-preserving doc-comment form.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocCommentKind {
    OuterLine,
    OuterBlock,
    InnerLine,
    InnerBlock,
}

/// Source-level doc comment attached to declarations.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocComment {
    pub kind: DocCommentKind,
    pub span: Span,
    pub text: String,
}

/// Receiver syntax for method/initializer declarations.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
/// - `_ x: T` -> None (wildcard prefix, no external label)
/// - `label x: T` -> Explicit(String) (explicit external label)
/// - `x: T` -> FromName (external label derived from parameter name)
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParamLabel {
    None,
    Explicit(String),
    FromName,
}

/// Function/initializer parameter declaration.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamDecl {
    pub label: ParamLabel,
    pub name: String,
    pub ty: Spanned<Type>,
}

/// A generic parameter that can be either a type parameter or a lifetime parameter.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericParam {
    Type { name: String },
    Lifetime { name: String },
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhereClause {
    pub predicates: Vec<Spanned<WherePredicate>>,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WherePredicate {
    pub ty: Spanned<Type>,
    pub bounds: Vec<Spanned<Type>>,
}

/// Source function declaration.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub visibility: Option<Visibility>,
    pub modifiers: Vec<Modifier>,
    pub name: String,
    pub generic_params: Vec<Spanned<GenericParam>>,
    pub receiver: Option<Spanned<ReceiverKind>>,
    pub params: Vec<Spanned<ParamDecl>>,
    pub return_type: Option<Spanned<Type>>,
    pub where_clause: Option<Spanned<WhereClause>>,
    #[serde(skip_serializing)]
    pub init_origin: Option<InitOriginKind>,
    pub body: Block,
}

#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InitKind {
    Plain,
    Optional,
    Fallible,
}

/// Internal origin metadata for function-like declarations lowered from `init`.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InitOriginKind {
    Plain,
    Optional,
    Fallible,
}

/// Source initializer declaration (`init`).
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InitDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub kind: InitKind,
    pub receiver: Option<Spanned<ReceiverKind>>,
    pub params: Vec<Spanned<ParamDecl>>,
    /// Return type annotation (e.g., `-> Option<Self>`, `-> Result<Self, E>`).
    /// If None, defaults to `Self` during desugaring.
    pub return_type: Option<Spanned<Type>>,
    pub body: Block,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructField {
    pub docs: Vec<Spanned<DocComment>>,
    /// Source attributes attached to this field declaration.
    pub attributes: Vec<Spanned<Attribute>>,
    pub name: String,
    pub ty: Spanned<Type>,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum StructMember {
    Field(Spanned<StructField>),
    Init(Spanned<InitDecl>),
    Function(Spanned<FunctionDecl>),
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub visibility: Option<Visibility>,
    pub modifiers: Vec<Modifier>,
    pub name: String,
    pub generic_params: Vec<Spanned<GenericParam>>,
    pub members: Vec<Spanned<StructMember>>,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnumCaseParam {
    Unnamed(Spanned<Type>),
    Named { name: String, ty: Spanned<Type> },
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumCase {
    pub docs: Vec<Spanned<DocComment>>,
    /// Source attributes attached to this enum case declaration.
    pub attributes: Vec<Spanned<Attribute>>,
    pub name: String,
    pub payload: Vec<Spanned<EnumCaseParam>>,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnumMember {
    Case(Spanned<EnumCase>),
    Init(Spanned<InitDecl>),
    Function(Spanned<FunctionDecl>),
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub visibility: Option<Visibility>,
    pub modifiers: Vec<Modifier>,
    pub name: String,
    pub generic_params: Vec<Spanned<GenericParam>>,
    pub members: Vec<Spanned<EnumMember>>,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImplMember {
    Init(Spanned<InitDecl>),
    Function(Spanned<FunctionDecl>),
    AssociatedType(Spanned<AssociatedTypeDecl>),
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImplDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub lifetime_params: Vec<Spanned<GenericParam>>,
    pub target: Spanned<Type>,
    pub conformance: Option<Spanned<Type>>,
    pub members: Vec<Spanned<ImplMember>>,
}

/// Macro declaration input-kind surface.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroInputKind {
    Item,
    Expr,
    Stmt,
    Block,
    Type,
    Pattern,
    Tokens,
    MacroArgs,
}

/// One named macro clause parameter.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroParam {
    pub name: String,
    pub kind: MacroInputKind,
}

/// Macro-body braces are captured as token fragments, not parsed as normal blocks.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroBlock {
    pub tokens: Vec<Token>,
    /// Byte range for source inside `{ ... }` (without braces).
    pub span: Span,
}

/// Unified declarative macro clause kind.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroClauseKind {
    Rule,
    Reflect,
}

/// One macro expansion clause (`rule(...) => { ... };` / `reflect(...) => { ... };`).
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroClause {
    pub kind: MacroClauseKind,
    pub params: Vec<Spanned<MacroParam>>,
    pub body: MacroBlock,
}

/// Unified declarative macro declaration.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub name: String,
    pub clauses: Vec<Spanned<MacroClause>>,
}

#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingKind {
    Let,
    Var,
}

#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessorRequirement {
    Get,
    Set,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssociatedTypeDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub name: String,
    pub bounds: Vec<Spanned<Type>>,
}

/// Protocol property requirement preserving `let`/`var` and accessor contract.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
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
    #[serde(skip_serializing)]
    pub init_origin: Option<InitOriginKind>,
    /// `None` means requirement-only (`;` form).
    pub default_body: Option<Block>,
}

/// Protocol initializer requirement with optional default implementation.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolInitMember {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub modifiers: Vec<Modifier>,
    pub kind: InitKind,
    pub receiver: Option<Spanned<ReceiverKind>>,
    pub params: Vec<Spanned<ParamDecl>>,
    /// Return type annotation (e.g., `-> Option<Self>`, `-> Result<Self, E>`).
    /// If None, defaults to `Self` during desugaring.
    pub return_type: Option<Spanned<Type>>,
    /// `None` means requirement-only (`;` form).
    pub default_body: Option<Block>,
}

/// Protocol member variants.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProtocolMember {
    Function(Spanned<ProtocolFunctionMember>),
    Initializer(Spanned<ProtocolInitMember>),
    AssociatedType(Spanned<AssociatedTypeDecl>),
    Property(Spanned<ProtocolPropertyRequirement>),
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolDecl {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub visibility: Option<Visibility>,
    pub modifiers: Vec<Modifier>,
    pub name: String,
    pub generic_params: Vec<Spanned<GenericParam>>,
    pub inheritance: Vec<Spanned<Type>>,
    pub members: Vec<Spanned<ProtocolMember>>,
}

/// Foreign import block with symbolic library name.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternBlock {
    pub docs: Vec<Spanned<DocComment>>,
    pub attributes: Vec<Spanned<Attribute>>,
    pub library_name: String,
    pub members: Vec<Spanned<ExternMember>>,
}

/// Foreign function import declaration.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternMember {
    Function(Spanned<ExternFunctionDecl>),
}

/// Source use declaration.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct UseItem {
    pub visibility: Option<Visibility>,
    pub tree: Spanned<UseTree>,
}

/// Source scope declaration (`scope foo;`).
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopeDecl {
    pub visibility: Option<Visibility>,
    pub name: String,
}

/// Source use path representation.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct UsePath {
    pub segments: Vec<String>,
}

/// Source use-tree representation.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum UseTree {
    Path {
        path: UsePath,
    },
    Glob {
        path: UsePath,
    },
    Alias {
        path: UsePath,
        alias: String,
    },
    Group {
        path: Option<UsePath>,
        items: Vec<Spanned<UseTree>>,
    },
    SelfImport,
    SelfAlias {
        alias: String,
    },
}
