//! Source-oriented frontend compilation modules.
//!
//! This namespace holds syntax-level compilation structures that operate on
//! source text before semantic analysis, including lexing and later parser/AST
//! layers.

pub mod ast;
pub mod desugar;
pub mod diagnostics;
pub mod expansion;
pub mod hir;
pub mod lexer;
pub mod parse_session;
pub mod parsed_file;
pub mod parser;
pub mod project;
pub mod resolver;
pub mod semantic;
pub mod source;

pub use desugar::{DesugaredFile, desugar_file, desugar_files};
pub use diagnostics::{
    Diagnostic, DiagnosticLabel, DiagnosticLabelKind, DiagnosticRenderer,
    DiagnosticSeverity, DiagnosticsBag, FileSpan,
    diagnostic_from_file_parse_error, diagnostic_from_import_resolve_error,
    diagnostic_from_parse_error, diagnostic_from_resolve_error,
    diagnostics_from_semantic_checks,
};
pub use expansion::{
    ExpandedFile, ExpansionOptions, MacroClause, MacroClauseKind,
    MacroDefinition, MacroInputSignature, MacroInvocation,
    MacroInvocationShape, MacroParam, MacroTable, SelectedMacroClause,
    dispatch_macro, expand_file, expand_parsed_files,
};
pub use hir::{
    HirAssignOp, HirBody, HirBodyId, HirExpr, HirExprId, HirExprKind, HirFile,
    HirFunction, HirInitOrigin, HirItem, HirItemId, HirItemKind, HirModule,
    HirOrigin, HirPat, HirPatId, HirPatKind, HirStmt, HirStmtId, HirStmtKind,
    HirType, HirTypeId, HirTypeKind, lower_to_hir,
};
pub use parse_session::ParseSession;
pub use parsed_file::{FileParseError, ParseSessionError, ParsedFile};
pub use project::{
    BinaryTarget, DependencyKind, DependencySpec, ImportRoot, ImportRootKind,
    LibraryTarget, LoadedDependencyProject, LoadedProject, ProjectGraph,
    ProjectLoadError, ProjectLoader, ProjectManifest, TargetKind, TargetRoots,
    WorkspaceManifest, build_target_roots, load_local_dependency_project_graph,
    load_project_from_dir,
};
pub use resolver::{
    BodyKind, DeclarationOwner, GlobalItem, GlobalItemTable, HirCollectedItem,
    HirCollectedItemKind, HirExprRef, HirItemRef, HirItemTable,
    HirImportBinding, HirImportBindingKind, HirImportError, HirImportTable,
    HirImportTables, HirItemTableError, HirLocalBinding,
    HirLocalBindingTable, HirPatRef, HirPathRef, HirPathResolution,
    HirPathResolutionError, HirPathResolutionTable, HirScopeResolutionError,
    HirScopeSymbol, HirScopeSymbols, HirUnresolvedPathDiagnostic,
    ImportBindingKind, ImportResolveError,
    ImportResolver, ItemId, ItemKind, LocalId, LocalKind, LocalMutability,
    NamedImportRoot, ResolveError, ResolvedBody, ResolvedBodyRef,
    ResolvedBodyReference, ResolvedBodyTable, ResolvedDeclaration,
    ResolvedDeclarationTable, ResolvedEnumCaseType, ResolvedEnumDeclaration,
    ResolvedEnumPayloadType, ResolvedFunctionSignature,
    ResolvedImplDeclaration, ResolvedImportBinding, ResolvedImports,
    ResolvedItemRef, ResolvedLocalBinding, ResolvedNamedFunctionSignature,
    ResolvedParamType, ResolvedProtocolDeclaration, ResolvedScope,
    ResolvedScopeKind, ResolvedStructDeclaration, ResolvedStructFieldType,
    ResolvedTypeRef, ScopeGraph, ScopeResolver, ScopeSymbols, Symbol,
    SymbolKind, UnresolvedBodyReference, UnresolvedDeclarationPath,
    build_global_item_table, build_hir_item_table, build_hir_local_binding_table,
    build_hir_path_resolution_table,
    build_hir_path_resolution_table_with_graph,
    build_hir_path_resolution_table_with_graph_and_imports,
    hir_scope_symbols_from_hir_item_table,
    resolve_bodies, resolve_declaration_types, resolve_project_imports,
    resolve_project_imports_with_named_roots,
    resolve_project_imports_with_named_roots_and_diagnostics,
    resolve_project_scopes, scope_symbols_from_global_item_table,
};
pub use semantic::{
    BodyControlFlowId, BodyControlFlowResult, BodyEnvIssue, BodyEnvIssueKind,
    BodyExprId, BodyLocalBindingInfo, BodyStmtId, BodyTypeEnvironment,
    BodyTypeEnvironmentTable, BuiltinType, ControlFlowIssue,
    ControlFlowIssueKind, ControlFlowTable, DefinitionLocation,
    DefinitionTarget, ExprCheckIssue, ExprCheckIssueKind, ExpressionTypeTable,
    ExternalDefinitionLocation, ExternalSemanticLookup, Mutability,
    NamedTypeKind, SemanticAnalysis, SemanticAnalysisIssues,
    SemanticCompletionCandidate, SemanticCompletionKind,
    SemanticDefinitionLookup, SignatureTypingIssue, SignatureTypingIssueKind,
    StatementKind, StatementTypeEntry, StatementTypeTable, StmtCheckIssue,
    StmtCheckIssueKind, Type, TypedAssociatedTypeBounds, TypedBody,
    TypedBodyId, TypedBodyIssueKind, TypedBodyIssueMarker, TypedBodyTable,
    TypedBodyTableIssue, TypedBodyTableIssueKind, TypedEnumCaseSignature,
    TypedEnumSignatureData, TypedFunctionSignature, TypedImplAttachment,
    TypedImplSignature, TypedItemData, TypedItemKind, TypedItemTable,
    TypedItemTableIssue, TypedItemTableIssueKind, TypedNamedFunctionSignature,
    TypedProtocolProperty, TypedProtocolSignatureData, TypedSignatureTable,
    TypedStructField, TypedStructSignatureData, analyze_semantics,
    analyze_semantics_with_external_lookup, build_body_type_environments,
    build_external_semantic_lookup, build_typed_body_table,
    build_typed_item_table, check_control_flow, check_control_flow_with_tables,
    check_expression_types, check_expression_types_with_external_lookup,
    check_statements, check_statements_with_expression_types,
    collect_item_definition_locations, completion_candidates_for_file,
    local_binding_type, lookup_definition_target, type_declaration_signatures,
};
