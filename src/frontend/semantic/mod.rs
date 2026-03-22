mod analysis;
pub mod body_env;
pub mod control_flow;
mod definition;
pub mod external_lookup;
pub mod hir_input;
pub mod item_table;
pub mod types;

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
pub use external_lookup::{
    ExternalDefinitionLocation, ExternalSemanticLookup,
    build_external_semantic_lookup,
};
pub use hir_input::SemanticHirInput;
pub use item_table::{
    TypedImplAttachment, TypedItemData, TypedItemKind, TypedItemTable,
    TypedItemTableIssue, TypedItemTableIssueKind, build_typed_item_table,
};
pub use types::{BuiltinType, Mutability, NamedTypeKind, Type};

// Re-export from midend for backward compatibility
pub use crate::midend::type_check::{
    ExprCheckIssue, ExprCheckIssueKind, ExpressionTypeTable,
    SignatureTypingIssue, SignatureTypingIssueKind, StatementKind,
    StatementTypeEntry, StatementTypeTable, StmtCheckIssue, StmtCheckIssueKind,
    TypedBody, TypedBodyId, TypedBodyIssueKind, TypedBodyIssueMarker,
    TypedBodyTable, TypedBodyTableIssue, TypedBodyTableIssueKind,
    TypedEnumCaseSignature, TypedEnumSignatureData, TypedFunctionSignature,
    TypedImplSignature, TypedNamedFunctionSignature, TypedProtocolProperty,
    TypedProtocolSignatureData, TypedSignatureTable, TypedStructSignatureData,
    build_typed_body_table, check_expression_types,
    check_expression_types_with_external_lookup, check_statements,
    check_statements_with_expression_types, type_declaration_signatures,
};
