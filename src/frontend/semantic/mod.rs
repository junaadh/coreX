mod analysis;
mod body_env;
mod control_flow;
mod definition;
mod expr_check;
mod external_lookup;
mod hir_input;
mod item_table;
mod signatures;
mod stmt_check;
mod typed_bodies;
mod types;

pub use analysis::{
    ResolvedHirSemanticInput, SemanticAnalysis, SemanticAnalysisIssues,
    analyze_semantics, analyze_semantics_with_external_lookup,
    resolve_hir_semantic_input,
};
pub use body_env::{
    BodyEnvIssue, BodyEnvIssueKind, BodyLocalBindingInfo, BodyTypeEnvironment,
    BodyTypeEnvironmentTable, build_body_type_environments,
};
pub use control_flow::{
    BodyControlFlowId, BodyControlFlowResult, ControlFlowIssue,
    ControlFlowIssueKind, ControlFlowTable, check_control_flow,
    check_control_flow_with_tables,
};
pub use definition::{
    DefinitionLocation, DefinitionTarget, SemanticCompletionCandidate,
    SemanticCompletionKind, SemanticDefinitionLookup,
    collect_item_definition_locations, completion_candidates_for_file,
    local_binding_type, lookup_definition_target,
};
pub use expr_check::{
    BodyExprId, ExprCheckIssue, ExprCheckIssueKind, ExpressionTypeTable,
    check_expression_types, check_expression_types_with_external_lookup,
};
pub use external_lookup::{
    ExternalDefinitionLocation, ExternalSemanticLookup,
    build_external_semantic_lookup,
};
pub use hir_input::SemanticHirInput;
pub use item_table::{
    TypedImplAttachment, TypedItemData, TypedItemKind, TypedItemTable,
    TypedItemTableIssue, TypedItemTableIssueKind, build_typed_item_table,
};
pub use signatures::{
    SignatureTypingIssue, SignatureTypingIssueKind, TypedAssociatedTypeBounds,
    TypedEnumCaseSignature, TypedEnumSignatureData, TypedFunctionSignature,
    TypedImplSignature, TypedNamedFunctionSignature, TypedProtocolProperty,
    TypedProtocolSignatureData, TypedSignatureTable, TypedStructField,
    TypedStructSignatureData, type_declaration_signatures,
};
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
pub use types::{BuiltinType, Mutability, NamedTypeKind, Type};
