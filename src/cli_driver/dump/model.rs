use core_x::frontend::source::FileId;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct TokenView {
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug)]
pub struct FileTokenDump {
    pub file_id: FileId,
    pub path: String,
    pub tokens: Vec<TokenView>,
}

#[derive(Debug)]
pub struct FileAstDump {
    pub file_id: FileId,
    pub path: String,
    pub item_count: usize,
    pub ast_debug: String,
    pub diagnostics_count: usize,
    pub ast_json: Value,
}

#[derive(Debug)]
pub struct FileParsedDump {
    pub file_id: FileId,
    pub path: String,
    pub item_count: usize,
    pub diagnostics_count: usize,
    pub parsed_debug: String,
    pub ast_json: Value,
    pub diagnostics_json: Vec<Value>,
}

pub struct ResolvedScopeDump {
    pub target: crate::cli_driver::project::TargetSelection,
    pub graph: core_x::frontend::ScopeGraph,
}

pub struct ResolvedImportDump {
    pub target: crate::cli_driver::project::TargetSelection,
    pub graph: core_x::frontend::ScopeGraph,
    pub symbols: BTreeMap<FileId, core_x::frontend::ScopeSymbols>,
    pub imports: BTreeMap<FileId, core_x::frontend::ResolvedImports>,
}

pub struct ResolvedSemanticDump {
    pub target: crate::cli_driver::project::TargetSelection,
    pub graph: core_x::frontend::ScopeGraph,
    pub symbols: BTreeMap<FileId, core_x::frontend::ScopeSymbols>,
    pub imports: BTreeMap<FileId, core_x::frontend::ResolvedImports>,
    pub semantic: core_x::frontend::SemanticAnalysis,
}
