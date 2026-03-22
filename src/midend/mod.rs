//! Middle-end compilation stages.
//!
//! The middle end operates on resolved and analyzed HIR from the frontend,
//! performing type checking, type inference, and other semantic transformations
//! before code generation.

pub mod completion;
pub mod type_check;
pub mod type_infer;

pub use completion::{
    CompletionCandidate, CompletionContext, CompletionData, CompletionInput,
    CompletionKind, CompletionMetadata, completion_candidates,
    completion_context_from_hir,
};
pub use type_check::{
    ExprCheckIssue, ExprCheckIssueKind, ExpressionTypeTable, StatementKind,
    StatementTypeEntry, StatementTypeTable, StmtCheckIssue, StmtCheckIssueKind,
    Type, TypedBody, TypedBodyId, TypedBodyIssueKind, TypedBodyIssueMarker,
    TypedBodyTable, TypedBodyTableIssue, TypedBodyTableIssueKind,
    TypedEnumCaseSignature, TypedEnumSignatureData, TypedFunctionSignature,
    TypedImplAttachment, TypedImplSignature, TypedNamedFunctionSignature,
    TypedParamLabel, TypedProtocolProperty, TypedProtocolSignatureData,
    TypedSignatureTable, TypedStructField, TypedStructSignatureData,
    check_expression_types, check_expression_types_with_external_lookup,
    check_statements, check_statements_with_expression_types,
    type_declaration_signatures,
};
pub use type_infer::{
    BodyInferIssue, BodyInferIssueKind, BodyInferenceTable, ConcreteType,
    InferenceConstraint, InferenceContext, InferenceDefaults, InferenceIssue,
    InferenceIssueKind, InferenceType, InferredCallTarget, LiteralDefaultKind,
    TypeVarId, infer_body_types,
};
