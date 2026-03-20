mod body_resolution;
mod call_signature;
mod declaration_resolution;
mod error;
mod hir_import_resolution;
mod hir_item_table;
mod hir_path_resolution;
mod hir_scope_resolution;
mod import_error;
mod import_resolver;
mod item_ids;
mod item_table;
mod local_ids;
mod model;
mod scope_resolver;
mod symbols;

pub use body_resolution::{
    BodyKind, LocalKind, LocalMutability, ResolvedBody, ResolvedBodyRef,
    ResolvedBodyReference, ResolvedBodyTable, ResolvedLocalBinding,
    UnresolvedBodyReference, resolve_bodies,
};
pub use call_signature::{CallParam, CallParamLabel, CallSignature};
pub use declaration_resolution::{
    DeclarationOwner, ResolvedDeclaration, ResolvedDeclarationTable,
    ResolvedEnumCaseType, ResolvedEnumDeclaration, ResolvedEnumPayloadType,
    ResolvedFunctionSignature, ResolvedImplDeclaration, ResolvedItemRef,
    ResolvedNamedFunctionSignature, ResolvedParamType,
    ResolvedProtocolDeclaration, ResolvedStructDeclaration,
    ResolvedStructFieldType, ResolvedTypeRef, UnresolvedDeclarationPath,
    resolve_declaration_types,
};
pub use error::ResolveError;
pub use hir_import_resolution::{
    HirImportBinding, HirImportBindingKind, HirImportError, HirImportTable,
    HirImportTables, HirScopeSymbol, HirScopeSymbols,
    hir_scope_symbols_from_hir_item_table,
};
pub use hir_item_table::{
    HirCollectedItem, HirCollectedItemKind, HirItemRef, HirItemTable,
    HirItemTableError, build_hir_item_table,
};
pub use hir_path_resolution::{
    AssociatedMemberKind, HirPathRef, HirPathResolution, HirPathResolutionError,
    HirPathResolutionTable, HirUnresolvedPathDiagnostic,
    build_hir_path_resolution_table,
    build_hir_path_resolution_table_with_graph,
    build_hir_path_resolution_table_with_graph_and_imports,
};
pub use hir_scope_resolution::{
    HirExprRef, HirLocalBinding, HirLocalBindingTable, HirPatRef,
    HirScopeResolutionError, build_hir_local_binding_table,
};
pub use import_error::ImportResolveError;
pub use import_resolver::{
    ImportBindingKind, ImportResolver, NamedImportRoot, ResolvedImportBinding,
    ResolvedImports, resolve_project_imports,
    resolve_project_imports_with_named_roots,
    resolve_project_imports_with_named_roots_and_diagnostics,
};
pub use item_ids::ItemId;
pub use item_table::{
    GlobalItem, GlobalItemTable, ItemKind, build_global_item_table,
};
pub use local_ids::LocalId;
pub use model::{ResolvedScope, ResolvedScopeKind, ScopeGraph};
pub use scope_resolver::{ScopeResolver, resolve_project_scopes};
pub use symbols::{
    ScopeSymbols, Symbol, SymbolKind, scope_symbols_from_global_item_table,
};
