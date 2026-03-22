use core_x::frontend::source::FileId;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct TokenView {
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct FileTokenDump {
    pub file_id: FileId,
    pub path: String,
    pub tokens: Vec<TokenView>,
}

#[derive(Debug, Clone)]
pub struct FileAstDump {
    pub file_id: FileId,
    pub path: String,
    pub item_count: usize,
    pub ast_debug: String,
    pub diagnostics_count: usize,
    pub ast_json: Value,
}

#[derive(Debug, Clone)]
pub struct FileParsedDump {
    pub file_id: FileId,
    pub path: String,
    pub item_count: usize,
    pub diagnostics_count: usize,
    pub parsed_debug: String,
    pub ast_json: Value,
    pub diagnostics_json: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct FileExpandedDump {
    pub file_id: FileId,
    pub path: String,
    pub item_count: usize,
    pub diagnostics_count: usize,
    pub expanded_debug: String,
    pub ast_json: Value,
    pub diagnostics_json: Vec<Value>,
    pub provenance_summary: Option<String>,
    pub provenance_summary_json: Value,
}

#[derive(Debug, Clone)]
pub struct FileDesugaredDump {
    pub file_id: FileId,
    pub path: String,
    pub item_count: usize,
    pub diagnostics_count: usize,
    pub desugared_debug: String,
    pub ast_json: Value,
    pub diagnostics_json: Vec<Value>,
    pub normalized_forms_summary: Option<String>,
    pub normalized_forms_json: Value,
}

#[derive(Debug, Clone)]
pub struct FileHirDump {
    pub file_id: FileId,
    pub path: String,
    pub root_items_count: usize,
    pub bodies_count: usize,
    pub exprs_count: usize,
    pub stmts_count: usize,
    pub types_count: usize,
    pub patterns_count: usize,
    pub hir_debug: String,
    pub diagnostics_count: usize,
    pub diagnostics_json: Vec<Value>,
    pub file_structure_json: Value,
    pub items_json: Vec<Value>,
    pub bodies_json: Vec<Value>,
    pub expr_table_json: Vec<Value>,
    pub stmt_table_json: Vec<Value>,
    pub type_table_json: Vec<Value>,
    pub pattern_table_json: Vec<Value>,
    pub origin_summary_json: Value,
}

#[derive(Debug, Clone)]
pub struct FileResolvedDump {
    pub file_id: FileId,
    pub path: String,
    pub global_items_count: usize,
    pub local_bindings_count: usize,
    pub path_resolutions_count: usize,
    pub import_bindings_count: usize,
    pub associated_member_resolutions_count: usize,
    pub resolved_bodies_count: usize,
    pub diagnostics_count: usize,
    pub diagnostics_json: Vec<Value>,
    pub item_table_json: Value,
    pub local_bindings_json: Vec<Value>,
    pub path_resolutions_json: Vec<Value>,
    pub import_bindings_json: Vec<Value>,
    pub named_root_resolutions_json: Vec<Value>,
    pub associated_member_resolutions_json: Vec<Value>,
    pub scope_symbols_json: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct FileTypedDump {
    pub file_id: FileId,
    pub path: String,
    pub typed_items_count: usize,
    pub typed_impls_count: usize,
    pub expr_types_count: usize,
    pub local_types_count: usize,
    pub selected_call_targets_count: usize,
    pub diagnostics_count: usize,
    pub diagnostics_json: Vec<Value>,
    pub typed_signatures_json: Value,
    pub inferred_expr_types_json: Vec<Value>,
    pub inferred_local_types_json: Vec<Value>,
    pub call_targets_json: Vec<Value>,
}

#[derive(Clone)]
pub struct ResolvedScopeDump {
    pub target: crate::cli_driver::project::TargetSelection,
    pub graph: core_x::frontend::ScopeGraph,
}

#[derive(Clone)]
pub struct ResolvedImportDump {
    pub target: crate::cli_driver::project::TargetSelection,
    pub graph: core_x::frontend::ScopeGraph,
    pub symbols: BTreeMap<FileId, core_x::frontend::ScopeSymbols>,
    pub imports: BTreeMap<FileId, core_x::frontend::ResolvedImports>,
}

#[derive(Clone)]
pub struct ResolvedSemanticDump {
    pub target: crate::cli_driver::project::TargetSelection,
    pub graph: core_x::frontend::ScopeGraph,
    pub symbols: BTreeMap<FileId, core_x::frontend::ScopeSymbols>,
    pub imports: BTreeMap<FileId, core_x::frontend::ResolvedImports>,
    pub semantic: core_x::frontend::SemanticAnalysis,
    pub inference: core_x::midend::BodyInferenceTable,
}

#[derive(Debug, Clone)]
pub struct PipelineDump {
    pub files: Vec<FilePipelineDump>,
}

#[derive(Debug, Clone)]
pub struct FilePipelineDump {
    pub file_id: FileId,
    pub path: String,
    pub parsed: Option<FileParsedDump>,
    pub expanded: Option<FileExpandedDump>,
    pub desugared: Option<FileDesugaredDump>,
    pub hir: Option<FileHirDump>,
    pub resolved: Option<FileResolvedDump>,
    pub typed: Option<FileTypedDump>,
}
