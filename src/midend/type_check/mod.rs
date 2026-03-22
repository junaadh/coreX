//! Type checking and type inference.
//!
//! This module performs type checking on resolved HIR, producing typed
//! intermediate representations and reporting type errors.

mod expr_check;
pub mod signatures;
mod stmt_check;
mod typed_bodies;

pub use expr_check::{
    BodyExprId, ExprCheckIssue, ExprCheckIssueKind, ExpressionTypeTable,
    check_expression_types, check_expression_types_with_external_lookup,
};
pub use signatures::{
    SignatureTypingIssue, SignatureTypingIssueKind, TypedAssociatedTypeBounds,
    TypedEnumCaseSignature, TypedEnumSignatureData, TypedFunctionSignature,
    TypedImplSignature, TypedNamedFunctionSignature, TypedParamLabel,
    TypedProtocolProperty, TypedProtocolSignatureData, TypedSignatureTable,
    TypedStructField, TypedStructSignatureData, type_declaration_signatures,
};

// TypedImplAttachment is still in frontend::semantic::item_table
pub use crate::frontend::semantic::item_table::TypedImplAttachment;
pub use stmt_check::{
    BodyStmtId, StatementKind, StatementTypeEntry, StatementTypeTable,
    StmtCheckIssue, StmtCheckIssueKind, check_statements,
    check_statements_with_expression_types,
};
pub use typed_bodies::{
    TypedBody, TypedBodyId, TypedBodyIssueKind, TypedBodyIssueMarker,
    TypedBodyTable, TypedBodyTableIssue, TypedBodyTableIssueKind,
    build_typed_body_table,
};

// Re-export types from frontend::semantic that are still needed
pub use crate::frontend::semantic::types::{
    BuiltinType, Mutability, NamedTypeKind, Type,
};
