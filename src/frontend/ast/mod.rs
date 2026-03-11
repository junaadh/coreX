//! Source AST specification scaffold for `coreX`.
//!
//! This module defines the parser-target AST surface for source syntax.
//! It is intentionally source-preserving and intentionally *not* a typed IR.
//!
//! Goals:
//! - preserve language structure needed by the parser
//! - carry spans for diagnostics and recovery
//! - avoid semantic overfitting at syntax stage
//! - preserve literal spellings where parser/semantic stages may need exact text
//!
//! Non-goals:
//! - parsing implementation
//! - name/type resolution
//! - typed lowering / HIR / MIR
//!
//! Comments are lexed separately (trivia-side). Doc comment attachment to
//! declarations is a later frontend pass and is not modeled as finalized
//! behavior here.
//!
//! Builtin primitive type names (`u8`, `u16`, `u32`, `u64`, `usize`, `i8`,
//! `i16`, `i32`, `i64`, `isize`, `f32`, `f64`, `bool`, `char`, `string`,
//! `void`) are represented through normal source named-type nodes and resolved
//! as builtins in later semantic analysis.

mod decl;
mod expr;
mod item;
mod pattern;
mod span;
mod stmt;
mod ty;

pub use decl::{
    AccessorRequirement, AssociatedTypeDecl, Attribute, AttributeArgs,
    BindingKind, EnumCase, EnumCaseParam, EnumDecl, EnumMember, ExternBlock,
    ExternFunctionDecl, ExternMember, FunctionDecl, GenericParam, ImplDecl,
    ImplMember, InitDecl, InitKind, Modifier, ParamDecl, ParamLabel,
    ProtocolDecl, ProtocolFunctionMember, ProtocolInitMember, ProtocolMember,
    ProtocolPropertyRequirement, ReceiverKind, StructDecl, StructField,
    StructMember, UseItem, UseTree, WhereClause, WherePredicate,
};
pub use expr::{
    ArrayElement, AssignOp, BinaryOp, CallArg, ClosureParam, Expr,
    MacroExprArgs, MatchArm, MatchArmBody, StringLiteral, StringPart,
    StructLiteralField, TypeExpr, UnaryOp,
};
pub use item::{File, Item};
pub use pattern::Pattern;
pub use span::{Span, Spanned};
pub use stmt::{
    BindingClause, Block, Clause, ClauseList, ForStmt, GuardStmt, LetStmt,
    Stmt, VarStmt, WhileStmt,
};
pub use ty::Type;
