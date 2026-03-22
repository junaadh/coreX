use super::hir_import_resolution::{HirImportBindingKind, HirImportTables};
use super::hir_item_table::{HirCollectedItemKind, HirItemRef, HirItemTable};
use super::hir_scope_resolution::{
    HirExprRef, HirLocalBindingTable, build_hir_local_binding_table,
};
use super::local_ids::LocalId;
use super::model::ScopeGraph;
use crate::frontend::ast::Span;
use crate::frontend::hir::{HirExprId, HirExprKind, HirFile, HirModule};
use crate::frontend::source::FileId;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

/// Resolution result for one HIR path expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirPathResolution {
    Local(LocalId),
    Item(HirItemRef),
    AssociatedMember {
        type_item_ref: HirItemRef,
        member_name: String,
        member_kind: AssociatedMemberKind,
    },
}

/// Kinds of associated members that can be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociatedMemberKind {
    Method,
    Initializer,
}

/// Key for path-based side tables.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HirPathRef {
    pub file_id: FileId,
    pub expr_id: HirExprId,
    pub segments: Vec<String>,
}

impl HirPathRef {
    #[must_use]
    pub fn new(
        file_id: FileId,
        expr_id: HirExprId,
        segments: Vec<String>,
    ) -> Self {
        Self {
            file_id,
            expr_id,
            segments,
        }
    }
}

/// Unresolved-name diagnostic emitted by HIR path resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirUnresolvedPathDiagnostic {
    pub file_id: FileId,
    pub expr_id: HirExprId,
    pub span: Span,
    pub segments: Vec<String>,
}

/// HIR path-resolution side tables keyed by expression id and path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirPathResolutionTable {
    by_expr: BTreeMap<HirExprRef, HirPathResolution>,
    by_path: BTreeMap<HirPathRef, HirPathResolution>,
    pub unresolved_diagnostics: Vec<HirUnresolvedPathDiagnostic>,
}

impl HirPathResolutionTable {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            by_expr: BTreeMap::new(),
            by_path: BTreeMap::new(),
            unresolved_diagnostics: Vec::new(),
        }
    }

    /// Resolves HIR path expressions against local bindings and file-local
    /// top-level items.
    ///
    /// Does not resolve imports or extern declarations.
    ///
    /// # Errors
    ///
    /// Returns an error when required HIR containers are missing.
    pub fn resolve(
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
        local_bindings: &HirLocalBindingTable,
        item_table: &HirItemTable,
    ) -> Result<Self, HirPathResolutionError> {
        let mut by_expr = BTreeMap::new();
        let mut by_path = BTreeMap::new();
        let mut unresolved_diagnostics = Vec::new();
        let mut top_level_items_by_file: BTreeMap<
            FileId,
            BTreeMap<String, HirItemRef>,
        > = BTreeMap::new();

        for hir_file in hir_files {
            let module = hir_modules.get(&hir_file.file_id).ok_or(
                HirPathResolutionError::MissingModule {
                    file_id: hir_file.file_id,
                },
            )?;
            let mut names = BTreeMap::new();
            for item_ref in item_table.item_refs_in_file(hir_file.file_id) {
                let Some(item) = item_table.get(*item_ref) else {
                    return Err(HirPathResolutionError::MissingItem {
                        item_ref: *item_ref,
                    });
                };
                if !matches!(
                    item.kind,
                    HirCollectedItemKind::Function
                        | HirCollectedItemKind::Struct
                        | HirCollectedItemKind::Enum
                        | HirCollectedItemKind::Protocol
                ) {
                    continue;
                }
                names.entry(item.name.clone()).or_insert(item.item_ref);
            }
            top_level_items_by_file.insert(hir_file.file_id, names);
        }

        // Build the associated member index
        let member_index = HirAssociatedMemberIndex::build(
            hir_files,
            hir_modules,
            item_table,
        )?;

        for hir_file in hir_files {
            let _module = hir_modules.get(&hir_file.file_id).ok_or(
                HirPathResolutionError::MissingModule {
                    file_id: hir_file.file_id,
                },
            )?;

            for (expr_id, expr) in &_module.exprs {
                let HirExprKind::Path(path) = &expr.kind else {
                    continue;
                };

                let expr_ref = HirExprRef::new(hir_file.file_id, *expr_id);
                let path_ref = HirPathRef::new(
                    hir_file.file_id,
                    *expr_id,
                    path.segments.clone(),
                );

                if let Some(binding_id) =
                    local_bindings.binding_for_expr(hir_file.file_id, *expr_id)
                {
                    let resolution = HirPathResolution::Local(binding_id);
                    by_expr.insert(expr_ref, resolution.clone());
                    by_path.insert(path_ref, resolution);
                    continue;
                }

                let global_item = resolve_file_local_top_level_item(
                    &top_level_items_by_file,
                    hir_file.file_id,
                    &path.segments,
                );
                if let Some(item_ref) = global_item {
                    let resolution = HirPathResolution::Item(item_ref);
                    by_expr.insert(expr_ref, resolution.clone());
                    by_path.insert(path_ref, resolution);
                    continue;
                }

                // Try associated member resolution for 2-segment paths
                if path.segments.len() == 2 {
                    if let Some(type_item_ref) = top_level_items_by_file
                        .get(&hir_file.file_id)
                        .and_then(|items| items.get(&path.segments[0]))
                        .copied()
                    {
                        if let Some(member_info) = member_index
                            .lookup(type_item_ref, &path.segments[1])
                        {
                            let resolution =
                                HirPathResolution::AssociatedMember {
                                    type_item_ref,
                                    member_name: path.segments[1].clone(),
                                    member_kind: member_info.member_kind,
                                };
                            by_expr.insert(expr_ref, resolution.clone());
                            by_path.insert(path_ref, resolution);
                            continue;
                        }
                    }
                }

                unresolved_diagnostics.push(HirUnresolvedPathDiagnostic {
                    file_id: hir_file.file_id,
                    expr_id: *expr_id,
                    span: expr.origin.span,
                    segments: path.segments.clone(),
                });
            }
        }

        Ok(Self {
            by_expr,
            by_path,
            unresolved_diagnostics,
        })
    }

    /// Resolves HIR path expressions with scope graph and HIR import context.
    ///
    /// This includes cross-file module path resolution and import aliases.
    ///
    /// Shadowing order:
    /// 1. local bindings
    /// 2. imported names
    /// 3. in-scope top-level/module paths
    ///
    /// # Errors
    ///
    /// Returns an error when required HIR containers are missing.
    pub fn resolve_with_graph_and_imports(
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
        graph: &ScopeGraph,
        imports: Option<&HirImportTables>,
        local_bindings: &HirLocalBindingTable,
        item_table: &HirItemTable,
    ) -> Result<Self, HirPathResolutionError> {
        let mut by_expr = BTreeMap::new();
        let mut by_path = BTreeMap::new();
        let mut unresolved_diagnostics = Vec::new();

        let mut top_level_items_by_file: BTreeMap<
            FileId,
            BTreeMap<String, HirItemRef>,
        > = BTreeMap::new();
        for hir_file in hir_files {
            let mut names = BTreeMap::new();
            for item_ref in item_table.item_refs_in_file(hir_file.file_id) {
                let Some(item) = item_table.get(*item_ref) else {
                    return Err(HirPathResolutionError::MissingItem {
                        item_ref: *item_ref,
                    });
                };
                if !matches!(
                    item.kind,
                    HirCollectedItemKind::Function
                        | HirCollectedItemKind::Struct
                        | HirCollectedItemKind::Enum
                        | HirCollectedItemKind::Protocol
                ) {
                    continue;
                }
                names.entry(item.name.clone()).or_insert(item.item_ref);
            }
            top_level_items_by_file.insert(hir_file.file_id, names);
        }

        let mut current_item_paths = BTreeMap::new();
        for scope in graph.scopes.values() {
            for item_ref in item_table.item_refs_in_file(scope.file_id) {
                let Some(item) = item_table.get(*item_ref) else {
                    return Err(HirPathResolutionError::MissingItem {
                        item_ref: *item_ref,
                    });
                };
                let mut full_path = scope.scope_path.clone();
                full_path.push(item.name.clone());
                current_item_paths.entry(full_path).or_insert(*item_ref);
            }
        }

        let scope_paths_by_file = graph
            .scopes
            .iter()
            .map(|(file_id, scope)| (*file_id, scope.scope_path.clone()))
            .collect::<BTreeMap<_, _>>();

        // Build the associated member index
        let member_index = HirAssociatedMemberIndex::build(
            hir_files,
            hir_modules,
            item_table,
        )?;

        for hir_file in hir_files {
            let module = hir_modules.get(&hir_file.file_id).ok_or(
                HirPathResolutionError::MissingModule {
                    file_id: hir_file.file_id,
                },
            )?;

            let namespace_base_ids = namespace_base_expr_ids(module);
            for (expr_id, expr) in &module.exprs {
                let segments = match &expr.kind {
                    HirExprKind::Path(path) => {
                        if namespace_base_ids.contains(expr_id) {
                            continue;
                        }
                        path.segments.clone()
                    }
                    HirExprKind::NamespaceField { .. } => {
                        let Some(segments) =
                            namespace_segments(module, *expr_id)
                        else {
                            continue;
                        };
                        segments
                    }
                    _ => continue,
                };

                let expr_ref = HirExprRef::new(hir_file.file_id, *expr_id);
                let path_ref = HirPathRef::new(
                    hir_file.file_id,
                    *expr_id,
                    segments.clone(),
                );

                if segments.len() == 1
                    && let Some(binding_id) = local_bindings
                        .binding_for_expr(hir_file.file_id, *expr_id)
                {
                    let resolution = HirPathResolution::Local(binding_id);
                    by_expr.insert(expr_ref, resolution.clone());
                    by_path.insert(path_ref, resolution);
                    continue;
                }

                if let Some(resolution) = resolve_hir_path_with_context(
                    hir_file.file_id,
                    &segments,
                    imports,
                    &scope_paths_by_file,
                    &current_item_paths,
                    &top_level_items_by_file,
                    &member_index,
                ) {
                    by_expr.insert(expr_ref, resolution.clone());
                    by_path.insert(path_ref, resolution);
                    continue;
                }

                unresolved_diagnostics.push(HirUnresolvedPathDiagnostic {
                    file_id: hir_file.file_id,
                    expr_id: *expr_id,
                    span: expr.origin.span,
                    segments,
                });
            }
        }

        Ok(Self {
            by_expr,
            by_path,
            unresolved_diagnostics,
        })
    }

    #[must_use]
    pub fn by_expr(
        &self,
        file_id: FileId,
        expr_id: HirExprId,
    ) -> Option<HirPathResolution> {
        self.by_expr
            .get(&HirExprRef::new(file_id, expr_id))
            .cloned()
    }

    #[must_use]
    pub fn by_path(
        &self,
        file_id: FileId,
        expr_id: HirExprId,
        segments: &[String],
    ) -> Option<HirPathResolution> {
        self.by_path
            .get(&HirPathRef::new(file_id, expr_id, segments.to_vec()))
            .cloned()
    }

    #[must_use]
    pub fn iter_expr(
        &self,
    ) -> impl Iterator<Item = (&HirExprRef, &HirPathResolution)> {
        self.by_expr.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_expr.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_expr.is_empty()
    }
}

/// HIR path-resolution build failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirPathResolutionError {
    MissingModule {
        file_id: FileId,
    },
    MissingItem {
        item_ref: HirItemRef,
    },
    MissingLocalBindingTable(
        super::hir_scope_resolution::HirScopeResolutionError,
    ),
    MissingItemTable(super::hir_item_table::HirItemTableError),
}

impl Display for HirPathResolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingModule { file_id } => {
                write!(f, "missing HIR module for file id {}", file_id.raw())
            }
            Self::MissingItem { item_ref } => write!(
                f,
                "missing HIR item {} in file id {}",
                item_ref.item_id.raw(),
                item_ref.file_id.raw()
            ),
            Self::MissingLocalBindingTable(error) => {
                write!(f, "failed to build HIR local binding table: {error}")
            }
            Self::MissingItemTable(error) => {
                write!(f, "failed to build HIR item table: {error}")
            }
        }
    }
}

impl std::error::Error for HirPathResolutionError {}

/// Index of associated members (methods and initializers) for nominal types.
///
/// This index maps (type_item_ref, member_name) -> member_info for O(log n)
/// lookup of associated members during path resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirAssociatedMemberIndex {
    /// Maps (type_item_ref, member_name) -> member info
    members: BTreeMap<(HirItemRef, String), AssociatedMemberInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssociatedMemberInfo {
    pub member_kind: AssociatedMemberKind,
}

impl HirAssociatedMemberIndex {
    /// Look up an associated member by type and name.
    #[must_use]
    pub fn lookup(
        &self,
        type_item_ref: HirItemRef,
        member_name: &str,
    ) -> Option<AssociatedMemberInfo> {
        self.members
            .get(&(type_item_ref, member_name.to_string()))
            .copied()
    }

    /// Build the associated member index from HIR files and modules.
    ///
    /// This iterates over all structs, enums, and impl blocks to collect
    /// their methods and initializers.
    ///
    /// # Errors
    ///
    /// Returns an error when required HIR containers are missing.
    pub fn build(
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
        item_table: &HirItemTable,
    ) -> Result<Self, HirPathResolutionError> {
        let mut members = BTreeMap::new();

        for hir_file in hir_files {
            let module = hir_modules.get(&hir_file.file_id).ok_or(
                HirPathResolutionError::MissingModule {
                    file_id: hir_file.file_id,
                },
            )?;

            // Process each item in the file
            for item_ref in item_table.item_refs_in_file(hir_file.file_id) {
                let Some(item) = item_table.get(*item_ref) else {
                    return Err(HirPathResolutionError::MissingItem {
                        item_ref: *item_ref,
                    });
                };

                match item.kind {
                    HirCollectedItemKind::Struct
                    | HirCollectedItemKind::Enum => {
                        // Index methods and initializers defined directly on the type
                        if let Some(hir_item) =
                            module.items.get(&item_ref.item_id)
                        {
                            let functions = match &hir_item.kind {
                                crate::frontend::hir::HirItemKind::Struct(
                                    s,
                                ) => &s.functions,
                                crate::frontend::hir::HirItemKind::Enum(e) => {
                                    &e.functions
                                }
                                _ => continue,
                            };

                            for function in functions {
                                let member_kind =
                                    if function.init_origin.is_some() {
                                        AssociatedMemberKind::Initializer
                                    } else {
                                        AssociatedMemberKind::Method
                                    };
                                members.insert(
                                    (*item_ref, function.name.clone()),
                                    AssociatedMemberInfo { member_kind },
                                );
                            }
                        }
                    }
                    HirCollectedItemKind::Impl => {
                        // For impl blocks, we need to resolve the target type
                        // and index members under that type
                        if let Some(hir_item) =
                            module.items.get(&item_ref.item_id)
                        {
                            if let crate::frontend::hir::HirItemKind::Impl(
                                impl_block,
                            ) = &hir_item.kind
                            {
                                // Try to resolve the impl target type to an item reference
                                // For now, we'll handle simple named types
                                let target_item_ref = Self::resolve_impl_target(
                                    impl_block,
                                    hir_file.file_id,
                                    item_table,
                                );

                                if let Some(target_ref) = target_item_ref {
                                    for function in &impl_block.functions {
                                        let member_kind = if function
                                            .init_origin
                                            .is_some()
                                        {
                                            AssociatedMemberKind::Initializer
                                        } else {
                                            AssociatedMemberKind::Method
                                        };
                                        members.insert(
                                            (target_ref, function.name.clone()),
                                            AssociatedMemberInfo {
                                                member_kind,
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(Self { members })
    }

    /// Try to resolve an impl block's target type to an item reference.
    ///
    /// This handles simple named types like `Point` but not complex types like `Vec<T>`.
    fn resolve_impl_target(
        _impl_block: &crate::frontend::hir::HirImpl,
        _file_id: FileId,
        _item_table: &HirItemTable,
    ) -> Option<HirItemRef> {
        // For now, we need to look at the HIR type and try to find the corresponding item
        // This is a simplified version that handles the common case
        // A full implementation would walk the HIR type structure

        // Look for items with matching names in the same file
        // This is a placeholder - the real implementation needs to:
        // 1. Extract the type name from the HirTypeId
        // 2. Look it up in the item table
        // For now, return None to skip impl block members
        None
    }
}

/// Builds HIR path-resolution side tables.
///
/// # Errors
///
/// Propagates HIR local/item table construction failures and path-walk errors.
pub fn build_hir_path_resolution_table(
    hir_files: &[HirFile],
    hir_modules: &BTreeMap<FileId, HirModule>,
) -> Result<HirPathResolutionTable, HirPathResolutionError> {
    let local_bindings = build_hir_local_binding_table(hir_files, hir_modules)
        .map_err(HirPathResolutionError::MissingLocalBindingTable)?;
    let item_table =
        super::hir_item_table::build_hir_item_table(hir_files, hir_modules)
            .map_err(HirPathResolutionError::MissingItemTable)?;
    HirPathResolutionTable::resolve(
        hir_files,
        hir_modules,
        &local_bindings,
        &item_table,
    )
}

fn resolve_file_local_top_level_item(
    items_by_file: &BTreeMap<FileId, BTreeMap<String, HirItemRef>>,
    file_id: FileId,
    segments: &[String],
) -> Option<HirItemRef> {
    if segments.len() != 1 {
        return None;
    }

    items_by_file
        .get(&file_id)
        .and_then(|items| items.get(&segments[0]))
        .copied()
}

fn namespace_base_expr_ids(module: &HirModule) -> BTreeSet<HirExprId> {
    module
        .exprs
        .values()
        .filter_map(|expr| match expr.kind {
            HirExprKind::NamespaceField { base, .. } => Some(base),
            _ => None,
        })
        .collect()
}

fn namespace_segments(
    module: &HirModule,
    expr_id: HirExprId,
) -> Option<Vec<String>> {
    let expr = module.exprs.get(&expr_id)?;
    match &expr.kind {
        HirExprKind::Path(path) => Some(path.segments.clone()),
        HirExprKind::NamespaceField { base, name, .. } => {
            let mut segments = namespace_segments(module, *base)?;
            segments.push(name.clone());
            Some(segments)
        }
        _ => None,
    }
}

fn resolve_hir_path_with_context(
    file_id: FileId,
    segments: &[String],
    imports: Option<&HirImportTables>,
    scope_paths_by_file: &BTreeMap<FileId, Vec<String>>,
    current_item_paths: &BTreeMap<Vec<String>, HirItemRef>,
    top_level_items_by_file: &BTreeMap<FileId, BTreeMap<String, HirItemRef>>,
    member_index: &HirAssociatedMemberIndex,
) -> Option<HirPathResolution> {
    let first = segments.first()?;

    if let Some(imports) = imports
        && let Some(binding) =
            imports.get(file_id).and_then(|table| table.get(first))
    {
        if segments.len() == 1 {
            if binding.kind == HirImportBindingKind::Item {
                return binding.target_item.map(HirPathResolution::Item);
            }
        } else if binding.kind == HirImportBindingKind::Scope {
            let mut full_path = binding.target_path.clone();
            full_path.extend(segments.iter().skip(1).cloned());
            let root_key = binding.source_root.as_deref();
            if let Some(item_ref) = imports
                .item_paths_for_root(root_key)
                .and_then(|paths| paths.get(&full_path))
                .copied()
            {
                return Some(HirPathResolution::Item(item_ref));
            }
        }
    }

    if let Some(scope_path) = scope_paths_by_file.get(&file_id) {
        let mut local_full_path = scope_path.clone();
        local_full_path.extend(segments.iter().cloned());
        if let Some(item_ref) =
            current_item_paths.get(&local_full_path).copied()
        {
            return Some(HirPathResolution::Item(item_ref));
        }
    }

    if segments.len() == 1 {
        if let Some(item_ref) = top_level_items_by_file
            .get(&file_id)
            .and_then(|items| items.get(first))
            .copied()
        {
            return Some(HirPathResolution::Item(item_ref));
        }
    }

    // Try associated member resolution for 2-segment paths like Point::init
    if segments.len() == 2 {
        if let Some(type_item_ref) = top_level_items_by_file
            .get(&file_id)
            .and_then(|items| items.get(&segments[0]))
            .copied()
        {
            if let Some(member_info) =
                member_index.lookup(type_item_ref, &segments[1])
            {
                return Some(HirPathResolution::AssociatedMember {
                    type_item_ref,
                    member_name: segments[1].clone(),
                    member_kind: member_info.member_kind,
                });
            }
        }
    }

    None
}

/// Builds HIR path-resolution side tables with scope-graph context.
///
/// Resolves module paths across files but does not apply imports.
///
/// # Errors
///
/// Propagates HIR local/item table construction failures and path-walk errors.
pub fn build_hir_path_resolution_table_with_graph(
    hir_files: &[HirFile],
    hir_modules: &BTreeMap<FileId, HirModule>,
    graph: &ScopeGraph,
) -> Result<HirPathResolutionTable, HirPathResolutionError> {
    build_hir_path_resolution_table_with_graph_and_imports(
        hir_files,
        hir_modules,
        graph,
        None,
    )
}

/// Builds HIR path-resolution side tables with scope graph and resolved imports.
///
/// # Errors
///
/// Propagates HIR local/item table construction failures and path-walk errors.
pub fn build_hir_path_resolution_table_with_graph_and_imports(
    hir_files: &[HirFile],
    hir_modules: &BTreeMap<FileId, HirModule>,
    graph: &ScopeGraph,
    imports: Option<&HirImportTables>,
) -> Result<HirPathResolutionTable, HirPathResolutionError> {
    let local_bindings = build_hir_local_binding_table(hir_files, hir_modules)
        .map_err(HirPathResolutionError::MissingLocalBindingTable)?;
    let item_table =
        super::hir_item_table::build_hir_item_table(hir_files, hir_modules)
            .map_err(HirPathResolutionError::MissingItemTable)?;
    HirPathResolutionTable::resolve_with_graph_and_imports(
        hir_files,
        hir_modules,
        graph,
        imports,
        &local_bindings,
        &item_table,
    )
}
