mod declaration_resolution;
mod error;
mod import_error;
mod import_resolver;
mod item_ids;
mod item_table;
mod model;
mod scope_resolver;
mod symbols;

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
pub use model::{ResolvedScope, ResolvedScopeKind, ScopeGraph};
pub use scope_resolver::{ScopeResolver, resolve_project_scopes};
pub use symbols::{
    ScopeSymbols, Symbol, SymbolKind, scope_symbols_from_global_item_table,
};
