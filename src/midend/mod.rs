//! Middle-end compilation stages.
//!
//! The middle end operates on resolved and analyzed HIR from the frontend,
//! performing type checking, type inference, and other semantic transformations
//! before code generation.

pub mod type_check;

pub use type_check::{
    ExprCheckIssue, ExprCheckIssueKind, ExpressionTypeTable, StmtCheckIssue,
    StmtCheckIssueKind, StatementKind, StatementTypeEntry, StatementTypeTable,
    Type, TypedBody, TypedBodyId, TypedBodyIssueKind, TypedBodyIssueMarker,
    TypedBodyTable, TypedBodyTableIssue, TypedBodyTableIssueKind,
    TypedEnumCaseSignature, TypedEnumSignatureData, TypedFunctionSignature,
    TypedImplAttachment, TypedImplSignature, TypedNamedFunctionSignature,
    TypedProtocolProperty, TypedProtocolSignatureData, TypedSignatureTable,
    TypedStructField, TypedStructSignatureData,
    check_expression_types, check_expression_types_with_external_lookup,
    check_statements, check_statements_with_expression_types,
    type_declaration_signatures,
};
