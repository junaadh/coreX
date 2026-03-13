//! Source-oriented frontend compilation modules.
//!
//! This namespace holds syntax-level compilation structures that operate on
//! source text before semantic analysis, including lexing and later parser/AST
//! layers.

pub mod ast;
pub mod diagnostics;
pub mod lexer;
pub mod parse_session;
pub mod parsed_file;
pub mod parser;
pub mod project;
pub mod resolver;
pub mod source;

pub use diagnostics::{
    Diagnostic, DiagnosticLabel, DiagnosticLabelKind, DiagnosticRenderer,
    DiagnosticSeverity, DiagnosticsBag, FileSpan,
    diagnostic_from_file_parse_error, diagnostic_from_import_resolve_error,
    diagnostic_from_parse_error, diagnostic_from_resolve_error,
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
    BodyKind, DeclarationOwner, GlobalItem, GlobalItemTable, ImportBindingKind,
    ImportResolveError, ImportResolver, ItemId, ItemKind, LocalId, LocalKind,
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
    build_global_item_table, resolve_bodies, resolve_declaration_types,
    resolve_project_imports, resolve_project_imports_with_named_roots,
    resolve_project_imports_with_named_roots_and_diagnostics,
    resolve_project_scopes, scope_symbols_from_global_item_table,
};
