//! HIR-level import resolution.
//!
//! This module mirrors `import_resolver` semantics on lowered HIR structures.
//! It supports grouped/glob/alias imports, module-path traversal across files,
//! and named-root linking (for binary -> library imports).

use super::hir_item_table::{
    HirCollectedItemKind, HirItemRef, HirItemTable, build_hir_item_table,
};
use super::import_resolver::NamedImportRoot;
use super::model::{ResolvedScope, ScopeGraph};
use crate::frontend::hir::{
    HirFile, HirItemKind, HirModule, HirUseTree, lower_to_hir,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

/// HIR import resolution errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirImportError {
    MissingHirFile {
        file_id: FileId,
    },
    MissingModule {
        file_id: FileId,
    },
    MissingItem {
        item_ref: HirItemRef,
    },
    UnknownRoot {
        from_file_id: FileId,
        root: String,
    },
    UnloadedDependencyRoot {
        from_file_id: FileId,
        root: String,
    },
    UnresolvedPath {
        from_file_id: FileId,
        path: Vec<String>,
    },
    InvalidSelfImport {
        from_file_id: FileId,
    },
    InvalidGlobTarget {
        from_file_id: FileId,
        path: Vec<String>,
    },
    DuplicateBinding {
        file_id: FileId,
        binding_name: String,
    },
    NamedRootItemTable {
        root: String,
        error: super::hir_item_table::HirItemTableError,
    },
}

impl Display for HirImportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHirFile { file_id } => {
                write!(f, "missing HIR file for file id {}", file_id.raw())
            }
            Self::MissingModule { file_id } => {
                write!(f, "missing HIR module for file id {}", file_id.raw())
            }
            Self::MissingItem { item_ref } => write!(
                f,
                "missing HIR item {} in file id {}",
                item_ref.item_id.raw(),
                item_ref.file_id.raw()
            ),
            Self::UnknownRoot { from_file_id, root } => write!(
                f,
                "unknown import root '{}' in file id {}",
                root,
                from_file_id.raw()
            ),
            Self::UnloadedDependencyRoot { from_file_id, root } => write!(
                f,
                "import root '{}' is declared but dependency is not loaded (file id {})",
                root,
                from_file_id.raw()
            ),
            Self::UnresolvedPath { from_file_id, path } => write!(
                f,
                "unresolved import path '{}' in file id {}",
                path.join("::"),
                from_file_id.raw()
            ),
            Self::InvalidSelfImport { from_file_id } => write!(
                f,
                "invalid self import form in file id {}",
                from_file_id.raw()
            ),
            Self::InvalidGlobTarget { from_file_id, path } => write!(
                f,
                "invalid glob target '{}' in file id {}",
                path.join("::"),
                from_file_id.raw()
            ),
            Self::DuplicateBinding {
                file_id,
                binding_name,
            } => write!(
                f,
                "duplicate import binding '{}' in file id {}",
                binding_name,
                file_id.raw()
            ),
            Self::NamedRootItemTable { root, error } => write!(
                f,
                "failed to build HIR item table for named root '{}': {}",
                root, error
            ),
        }
    }
}

impl std::error::Error for HirImportError {}

/// Imported binding category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirImportBindingKind {
    Scope,
    Item,
}

/// One resolved import binding in one HIR file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirImportBinding {
    pub local_name: String,
    pub kind: HirImportBindingKind,
    pub target_file_id: FileId,
    pub target_path: Vec<String>,
    pub target_item: Option<HirItemRef>,
    pub source_root: Option<String>,
}

/// HIR imports resolved for one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirImportTable {
    pub file_id: FileId,
    pub bindings: BTreeMap<String, HirImportBinding>,
}

impl HirImportTable {
    #[must_use]
    pub fn new(file_id: FileId) -> Self {
        Self {
            file_id,
            bindings: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&HirImportBinding> {
        self.bindings.get(name)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// One top-level symbol collected from HIR for a scope file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirScopeSymbol {
    pub name: String,
    pub kind: HirCollectedItemKind,
    pub item_ref: HirItemRef,
}

/// Symbol table for one scope file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirScopeSymbols {
    pub file_id: FileId,
    pub symbols: BTreeMap<String, HirScopeSymbol>,
}

impl HirScopeSymbols {
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&HirScopeSymbol> {
        self.symbols.get(name)
    }
}

/// Builds HIR scope symbol tables from a collected HIR item table.
#[must_use]
pub fn hir_scope_symbols_from_hir_item_table(
    table: &HirItemTable,
) -> BTreeMap<FileId, HirScopeSymbols> {
    let mut by_scope = BTreeMap::new();

    for item in table.iter() {
        let scope_symbols =
            by_scope
                .entry(item.file_id)
                .or_insert_with(|| HirScopeSymbols {
                    file_id: item.file_id,
                    symbols: BTreeMap::new(),
                });

        scope_symbols.symbols.entry(item.name.clone()).or_insert(
            HirScopeSymbol {
                name: item.name.clone(),
                kind: item.kind,
                item_ref: item.item_ref,
            },
        );
    }

    by_scope
}

/// Multi-file HIR import tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirImportTables {
    by_file: BTreeMap<FileId, HirImportTable>,
    scope_paths_by_file: BTreeMap<FileId, Vec<String>>,
    item_paths_by_root:
        BTreeMap<Option<String>, BTreeMap<Vec<String>, HirItemRef>>,
}

impl HirImportTables {
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_file: BTreeMap::new(),
            scope_paths_by_file: BTreeMap::new(),
            item_paths_by_root: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn get(&self, file_id: FileId) -> Option<&HirImportTable> {
        self.by_file.get(&file_id)
    }

    #[must_use]
    pub fn scope_path_for_file(&self, file_id: FileId) -> Option<&[String]> {
        self.scope_paths_by_file.get(&file_id).map(Vec::as_slice)
    }

    #[must_use]
    pub fn item_paths_for_root(
        &self,
        root: Option<&str>,
    ) -> Option<&BTreeMap<Vec<String>, HirItemRef>> {
        self.item_paths_by_root.get(&root.map(ToString::to_string))
    }

    fn get_or_create(&mut self, file_id: FileId) -> &mut HirImportTable {
        self.by_file
            .entry(file_id)
            .or_insert_with(|| HirImportTable::new(file_id))
    }

    /// Legacy HIR import resolution without a scope graph.
    ///
    /// This keeps backwards compatibility and only resolves single-segment
    /// imports to same-file top-level declarations.
    ///
    /// # Errors
    ///
    /// Returns an error when HIR modules/items are missing or a local import
    /// cannot be resolved.
    pub fn resolve(
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
        item_table: &HirItemTable,
    ) -> Result<Self, HirImportError> {
        let mut tables = Self::new();

        for hir_file in hir_files {
            let module = hir_modules.get(&hir_file.file_id).ok_or(
                HirImportError::MissingModule {
                    file_id: hir_file.file_id,
                },
            )?;

            let table = tables.get_or_create(hir_file.file_id);
            for item_id in &hir_file.root_items {
                let item_ref = HirItemRef::new(hir_file.file_id, *item_id);
                let item = module
                    .items
                    .get(item_id)
                    .ok_or(HirImportError::MissingItem { item_ref })?;
                let HirItemKind::Use(hir_use) = &item.kind else {
                    continue;
                };
                resolve_legacy_use_tree(
                    hir_file.file_id,
                    &hir_use.tree,
                    item_table,
                    table,
                )?;
            }
        }

        Ok(tables)
    }

    /// Resolves HIR imports using project scope graph semantics.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid or unresolved import trees.
    pub fn resolve_with_graph(
        graph: &ScopeGraph,
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
        item_table: &HirItemTable,
    ) -> Result<Self, HirImportError> {
        Self::resolve_with_graph_and_named_roots(
            graph,
            hir_files,
            hir_modules,
            item_table,
            &BTreeMap::new(),
        )
    }

    /// Resolves HIR imports with named roots (binary -> library linking).
    ///
    /// # Errors
    ///
    /// Returns an error for invalid or unresolved import trees.
    pub fn resolve_with_graph_and_named_roots(
        graph: &ScopeGraph,
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
        item_table: &HirItemTable,
        named_roots: &BTreeMap<String, NamedImportRoot>,
    ) -> Result<Self, HirImportError> {
        let (tables, diagnostics) =
            Self::resolve_with_graph_and_named_roots_and_diagnostics(
                graph,
                hir_files,
                hir_modules,
                item_table,
                named_roots,
            )?;
        if let Some(first) = diagnostics.into_iter().next() {
            return Err(first);
        }
        Ok(tables)
    }

    /// Resolves HIR imports and accumulates all structural errors as
    /// diagnostics while producing partial tables.
    ///
    /// # Errors
    ///
    /// Returns an error only when a named-root context cannot be prepared.
    pub fn resolve_with_graph_and_named_roots_and_diagnostics(
        graph: &ScopeGraph,
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
        item_table: &HirItemTable,
        named_roots: &BTreeMap<String, NamedImportRoot>,
    ) -> Result<(Self, Vec<HirImportError>), HirImportError> {
        let scope_symbols = hir_scope_symbols_from_hir_item_table(item_table);
        let current_item_paths = build_item_path_index(graph, item_table);
        let named_root_contexts = build_named_root_contexts(named_roots)?;

        let resolver = HirImportResolver {
            graph,
            scope_symbols: &scope_symbols,
            current_item_paths: &current_item_paths,
            named_roots: named_root_contexts,
        };

        let (by_file, diagnostics) =
            resolver.resolve_imports_with_diagnostics(hir_files, hir_modules);

        let mut scope_paths_by_file = BTreeMap::new();
        for (file_id, scope) in &graph.scopes {
            scope_paths_by_file.insert(*file_id, scope.scope_path.clone());
        }

        let mut item_paths_by_root = BTreeMap::new();
        item_paths_by_root.insert(None, current_item_paths.clone());
        for (name, root) in &resolver.named_roots {
            let RegisteredNamedRoot::Loaded(context) = root else {
                continue;
            };
            item_paths_by_root
                .insert(Some(name.clone()), context.item_paths.clone());
        }

        Ok((
            Self {
                by_file,
                scope_paths_by_file,
                item_paths_by_root,
            },
            diagnostics,
        ))
    }
}

fn resolve_legacy_use_tree(
    file_id: FileId,
    tree: &HirUseTree,
    item_table: &HirItemTable,
    table: &mut HirImportTable,
) -> Result<(), HirImportError> {
    let mut stack = vec![(Vec::<String>::new(), tree)];
    while let Some((prefix, current)) = stack.pop() {
        match current {
            HirUseTree::Path { path } => {
                let full_path = prefixed_path(&prefix, &path.segments);
                bind_legacy_item(file_id, &full_path, None, item_table, table)?;
            }
            HirUseTree::Alias { path, alias } => {
                let full_path = prefixed_path(&prefix, &path.segments);
                bind_legacy_item(
                    file_id,
                    &full_path,
                    Some(alias.clone()),
                    item_table,
                    table,
                )?;
            }
            HirUseTree::Glob { .. } => {
                return Err(HirImportError::InvalidGlobTarget {
                    from_file_id: file_id,
                    path: prefix,
                });
            }
            HirUseTree::Group { path, items } => {
                let mut next_prefix = prefix;
                if let Some(path) = path {
                    next_prefix.extend(path.segments.iter().cloned());
                }
                for item in items.iter().rev() {
                    stack.push((next_prefix.clone(), item));
                }
            }
            HirUseTree::SelfImport | HirUseTree::SelfAlias { .. } => {
                return Err(HirImportError::InvalidSelfImport {
                    from_file_id: file_id,
                });
            }
        }
    }

    Ok(())
}

fn bind_legacy_item(
    file_id: FileId,
    path: &[String],
    alias: Option<String>,
    item_table: &HirItemTable,
    table: &mut HirImportTable,
) -> Result<(), HirImportError> {
    let Some(name) = path.last().cloned() else {
        return Err(HirImportError::UnresolvedPath {
            from_file_id: file_id,
            path: path.to_vec(),
        });
    };
    if path.len() != 1 {
        return Err(HirImportError::UnresolvedPath {
            from_file_id: file_id,
            path: path.to_vec(),
        });
    }
    let Some(item_ref) = item_table.item_ref_by_name(&name) else {
        return Err(HirImportError::UnresolvedPath {
            from_file_id: file_id,
            path: path.to_vec(),
        });
    };
    if item_ref.file_id != file_id {
        return Err(HirImportError::UnresolvedPath {
            from_file_id: file_id,
            path: path.to_vec(),
        });
    }
    insert_binding_checked(
        table,
        HirImportBinding {
            local_name: alias.unwrap_or(name.clone()),
            kind: HirImportBindingKind::Item,
            target_file_id: item_ref.file_id,
            target_path: vec![name],
            target_item: Some(item_ref),
            source_root: None,
        },
    )
}

fn build_named_root_contexts(
    named_roots: &BTreeMap<String, NamedImportRoot>,
) -> Result<BTreeMap<String, RegisteredNamedRoot>, HirImportError> {
    let mut resolved = BTreeMap::new();

    for (name, root) in named_roots {
        match root {
            NamedImportRoot::UnloadedDependency => {
                resolved.insert(
                    name.clone(),
                    RegisteredNamedRoot::UnloadedDependency,
                );
            }
            NamedImportRoot::LoadedLibrary {
                graph,
                parsed_files,
                ..
            } => {
                let mut hir_files = Vec::new();
                let mut hir_modules = BTreeMap::new();
                for parsed in parsed_files {
                    let (hir_file, hir_module) = lower_to_hir(parsed);
                    hir_modules.insert(hir_file.file_id, hir_module);
                    hir_files.push(hir_file);
                }

                let item_table = build_hir_item_table(&hir_files, &hir_modules)
                    .map_err(|error| HirImportError::NamedRootItemTable {
                        root: name.clone(),
                        error,
                    })?;
                let scope_symbols =
                    hir_scope_symbols_from_hir_item_table(&item_table);
                let item_paths = build_item_path_index(graph, &item_table);
                resolved.insert(
                    name.clone(),
                    RegisteredNamedRoot::Loaded(HirRootContext {
                        graph: graph.clone(),
                        scope_symbols,
                        item_paths,
                    }),
                );
            }
        }
    }

    Ok(resolved)
}

fn build_item_path_index(
    graph: &ScopeGraph,
    item_table: &HirItemTable,
) -> BTreeMap<Vec<String>, HirItemRef> {
    let mut by_path = BTreeMap::new();

    for scope in graph.scopes.values() {
        for item_ref in item_table.item_refs_in_file(scope.file_id) {
            let Some(item) = item_table.get(*item_ref) else {
                continue;
            };
            let mut full_path = scope.scope_path.clone();
            full_path.push(item.name.clone());
            by_path.entry(full_path).or_insert(*item_ref);
        }
    }

    by_path
}

struct HirImportResolver<'a> {
    graph: &'a ScopeGraph,
    scope_symbols: &'a BTreeMap<FileId, HirScopeSymbols>,
    current_item_paths: &'a BTreeMap<Vec<String>, HirItemRef>,
    named_roots: BTreeMap<String, RegisteredNamedRoot>,
}

impl HirImportResolver<'_> {
    fn resolve_imports_with_diagnostics(
        &self,
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
    ) -> (BTreeMap<FileId, HirImportTable>, Vec<HirImportError>) {
        let hir_file_by_id = hir_files
            .iter()
            .map(|file| (file.file_id, file))
            .collect::<BTreeMap<_, _>>();
        let parent_map = self.parent_map();
        let mut by_file = BTreeMap::new();
        let mut diagnostics = Vec::new();

        for file_id in self.graph.scopes.keys().copied() {
            let mut table = HirImportTable::new(file_id);
            let Some(hir_file) = hir_file_by_id.get(&file_id).copied() else {
                diagnostics.push(HirImportError::MissingHirFile { file_id });
                by_file.insert(file_id, table);
                continue;
            };
            let Some(module) = hir_modules.get(&file_id) else {
                diagnostics.push(HirImportError::MissingModule { file_id });
                by_file.insert(file_id, table);
                continue;
            };

            for item_id in &hir_file.root_items {
                let item_ref = HirItemRef::new(file_id, *item_id);
                let Some(item) = module.items.get(item_id) else {
                    diagnostics.push(HirImportError::MissingItem { item_ref });
                    continue;
                };
                let HirItemKind::Use(hir_use) = &item.kind else {
                    continue;
                };
                if let Err(error) = self.resolve_use_tree_into(
                    file_id,
                    file_id,
                    &[],
                    &hir_use.tree,
                    &parent_map,
                    &mut table,
                ) {
                    diagnostics.push(error);
                }
            }

            by_file.insert(file_id, table);
        }

        (by_file, diagnostics)
    }

    fn parent_map(&self) -> BTreeMap<FileId, FileId> {
        let mut parents = BTreeMap::new();
        for (file_id, scope) in &self.graph.scopes {
            for child_file_id in &scope.child_scope_ids {
                parents.entry(*child_file_id).or_insert(*file_id);
            }
        }
        parents
    }

    fn resolve_use_tree_into(
        &self,
        from_file_id: FileId,
        current_scope_id: FileId,
        prefix: &[String],
        tree: &HirUseTree,
        parent_map: &BTreeMap<FileId, FileId>,
        table: &mut HirImportTable,
    ) -> Result<(), HirImportError> {
        match tree {
            HirUseTree::Path { path } => {
                let path = prefixed_path(prefix, &path.segments);
                self.resolve_and_bind_path(
                    from_file_id,
                    current_scope_id,
                    &path,
                    None,
                    parent_map,
                    table,
                )
            }
            HirUseTree::Alias { path, alias } => {
                let path = prefixed_path(prefix, &path.segments);
                self.resolve_and_bind_path(
                    from_file_id,
                    current_scope_id,
                    &path,
                    Some(alias.clone()),
                    parent_map,
                    table,
                )
            }
            HirUseTree::Glob { path } => {
                let path = prefixed_path(prefix, &path.segments);
                self.resolve_glob(
                    from_file_id,
                    current_scope_id,
                    &path,
                    parent_map,
                    table,
                )
            }
            HirUseTree::Group { path, items } => {
                let mut next_prefix = prefix.to_vec();
                if let Some(path) = path {
                    next_prefix.extend(path.segments.iter().cloned());
                }
                for item in items {
                    self.resolve_use_tree_into(
                        from_file_id,
                        current_scope_id,
                        &next_prefix,
                        item,
                        parent_map,
                        table,
                    )?;
                }
                Ok(())
            }
            HirUseTree::SelfImport => {
                if prefix.is_empty() {
                    return Err(HirImportError::InvalidSelfImport {
                        from_file_id,
                    });
                }
                self.resolve_and_bind_path(
                    from_file_id,
                    current_scope_id,
                    prefix,
                    None,
                    parent_map,
                    table,
                )
            }
            HirUseTree::SelfAlias { alias } => {
                if prefix.is_empty() {
                    return Err(HirImportError::InvalidSelfImport {
                        from_file_id,
                    });
                }
                let target = self.resolve_use_path(
                    from_file_id,
                    current_scope_id,
                    prefix,
                    parent_map,
                )?;
                let binding = binding_from_target(alias.clone(), target);
                insert_binding_checked(table, binding)
            }
        }
    }

    fn resolve_and_bind_path(
        &self,
        from_file_id: FileId,
        current_scope_id: FileId,
        path: &[String],
        alias: Option<String>,
        parent_map: &BTreeMap<FileId, FileId>,
        table: &mut HirImportTable,
    ) -> Result<(), HirImportError> {
        let target = self.resolve_use_path(
            from_file_id,
            current_scope_id,
            path,
            parent_map,
        )?;
        let local_name = match alias {
            Some(alias) => alias,
            None => path
                .last()
                .cloned()
                .ok_or_else(|| unresolved_path(from_file_id, path))?,
        };
        let binding = binding_from_target(local_name, target);
        insert_binding_checked(table, binding)
    }

    fn resolve_glob(
        &self,
        from_file_id: FileId,
        current_scope_id: FileId,
        path: &[String],
        parent_map: &BTreeMap<FileId, FileId>,
        table: &mut HirImportTable,
    ) -> Result<(), HirImportError> {
        let target = self.resolve_use_path(
            from_file_id,
            current_scope_id,
            path,
            parent_map,
        )?;
        let (target_scope_id, root_source) = match target {
            ResolvedPathTarget::Scope {
                file_id,
                root_source,
                ..
            } => (file_id, root_source),
            ResolvedPathTarget::Item { .. } => {
                return Err(HirImportError::InvalidGlobTarget {
                    from_file_id,
                    path: path.to_vec(),
                });
            }
        };

        let context = self
            .context_for_root_source(&root_source)
            .ok_or_else(|| unresolved_path(from_file_id, path))?;
        let target_scope = scope_by_id(context.graph, target_scope_id)
            .ok_or_else(|| unresolved_path(from_file_id, path))?;

        let mut child_scopes = BTreeMap::new();
        for child_file_id in &target_scope.child_scope_ids {
            if let Some(child_scope) =
                scope_by_id(context.graph, *child_file_id)
            {
                child_scopes
                    .entry(child_scope.name.clone())
                    .or_insert(*child_file_id);
            }
        }

        for (name, child_file_id) in child_scopes {
            let child_scope = scope_by_id(context.graph, child_file_id)
                .ok_or_else(|| unresolved_path(from_file_id, path))?;
            insert_binding_checked(
                table,
                HirImportBinding {
                    local_name: name,
                    kind: HirImportBindingKind::Scope,
                    target_file_id: child_file_id,
                    target_path: child_scope.scope_path.clone(),
                    target_item: None,
                    source_root: named_root_name(&root_source),
                },
            )?;
        }

        if let Some(scope_symbols) = context.scope_symbols.get(&target_scope_id)
        {
            for (name, symbol) in &scope_symbols.symbols {
                let mut target_path = target_scope.scope_path.clone();
                target_path.push(name.clone());
                insert_binding_checked(
                    table,
                    HirImportBinding {
                        local_name: name.clone(),
                        kind: HirImportBindingKind::Item,
                        target_file_id: symbol.item_ref.file_id,
                        target_path,
                        target_item: Some(symbol.item_ref),
                        source_root: named_root_name(&root_source),
                    },
                )?;
            }
        }

        Ok(())
    }

    fn resolve_use_path(
        &self,
        from_file_id: FileId,
        current_scope_id: FileId,
        path: &[String],
        parent_map: &BTreeMap<FileId, FileId>,
    ) -> Result<ResolvedPathTarget, HirImportError> {
        if path.is_empty() {
            return Err(unresolved_path(from_file_id, path));
        }

        if path.len() > 1 && path[1..].iter().any(|segment| segment == "self") {
            return Err(HirImportError::InvalidSelfImport { from_file_id });
        }

        let (root_source, mut scope_id) = self.resolve_import_root(
            from_file_id,
            current_scope_id,
            path,
            parent_map,
        )?;
        let context = self
            .context_for_root_source(&root_source)
            .ok_or_else(|| unresolved_path(from_file_id, path))?;

        if path.len() == 1 {
            let scope = scope_by_id(context.graph, scope_id)
                .ok_or_else(|| unresolved_path(from_file_id, path))?;
            return Ok(ResolvedPathTarget::Scope {
                file_id: scope.file_id,
                target_path: scope.scope_path.clone(),
                root_source,
            });
        }

        for (index, segment) in path.iter().enumerate().skip(1) {
            if index + 1 != path.len() {
                scope_id = child_scope_named(context.graph, scope_id, segment)
                    .ok_or_else(|| unresolved_path(from_file_id, path))?;
                continue;
            }

            if let Some(child_scope_id) =
                child_scope_named(context.graph, scope_id, segment)
            {
                let child_scope = scope_by_id(context.graph, child_scope_id)
                    .ok_or_else(|| unresolved_path(from_file_id, path))?;
                return Ok(ResolvedPathTarget::Scope {
                    file_id: child_scope_id,
                    target_path: child_scope.scope_path.clone(),
                    root_source,
                });
            }

            let symbol = lookup_symbol_in_scope(
                context.scope_symbols,
                scope_id,
                segment,
            )
            .ok_or_else(|| unresolved_path(from_file_id, path))?;
            let scope = scope_by_id(context.graph, scope_id)
                .ok_or_else(|| unresolved_path(from_file_id, path))?;
            let mut target_path = scope.scope_path.clone();
            target_path.push(segment.clone());
            return Ok(ResolvedPathTarget::Item {
                item_ref: symbol.item_ref,
                target_path,
                root_source,
            });
        }

        Err(unresolved_path(from_file_id, path))
    }

    fn resolve_import_root(
        &self,
        from_file_id: FileId,
        current_scope_id: FileId,
        path: &[String],
        parent_map: &BTreeMap<FileId, FileId>,
    ) -> Result<(ResolvedRootSource, FileId), HirImportError> {
        let first = &path[0];
        match first.as_str() {
            "root" => {
                Ok((ResolvedRootSource::Current, self.graph.root_file_id))
            }
            "super" => parent_map
                .get(&current_scope_id)
                .copied()
                .map(|scope_id| (ResolvedRootSource::Current, scope_id))
                .ok_or_else(|| unresolved_path(from_file_id, path)),
            other => {
                let named = self.named_roots.get(other).ok_or_else(|| {
                    HirImportError::UnknownRoot {
                        from_file_id,
                        root: other.to_string(),
                    }
                })?;
                match named {
                    RegisteredNamedRoot::Loaded(context) => Ok((
                        ResolvedRootSource::Named(other.to_string()),
                        context.graph.root_file_id,
                    )),
                    RegisteredNamedRoot::UnloadedDependency => {
                        Err(HirImportError::UnloadedDependencyRoot {
                            from_file_id,
                            root: other.to_string(),
                        })
                    }
                }
            }
        }
    }

    fn context_for_root_source(
        &self,
        root_source: &ResolvedRootSource,
    ) -> Option<ResolutionContext<'_>> {
        match root_source {
            ResolvedRootSource::Current => Some(ResolutionContext {
                graph: self.graph,
                scope_symbols: self.scope_symbols,
                item_paths: self.current_item_paths,
            }),
            ResolvedRootSource::Named(name) => {
                let root = self.named_roots.get(name)?;
                match root {
                    RegisteredNamedRoot::Loaded(context) => {
                        Some(ResolutionContext {
                            graph: &context.graph,
                            scope_symbols: &context.scope_symbols,
                            item_paths: &context.item_paths,
                        })
                    }
                    RegisteredNamedRoot::UnloadedDependency => None,
                }
            }
        }
    }
}

fn scope_by_id(graph: &ScopeGraph, file_id: FileId) -> Option<&ResolvedScope> {
    graph.scope(file_id)
}

fn child_scope_named(
    graph: &ScopeGraph,
    file_id: FileId,
    name: &str,
) -> Option<FileId> {
    let scope = scope_by_id(graph, file_id)?;
    for child_file_id in &scope.child_scope_ids {
        let child_scope = scope_by_id(graph, *child_file_id)?;
        if child_scope.name == name {
            return Some(*child_file_id);
        }
    }
    None
}

fn lookup_symbol_in_scope<'a>(
    symbols: &'a BTreeMap<FileId, HirScopeSymbols>,
    file_id: FileId,
    name: &str,
) -> Option<&'a HirScopeSymbol> {
    symbols.get(&file_id)?.get(name)
}

fn insert_binding_checked(
    table: &mut HirImportTable,
    binding: HirImportBinding,
) -> Result<(), HirImportError> {
    if table.bindings.contains_key(&binding.local_name) {
        return Err(HirImportError::DuplicateBinding {
            file_id: table.file_id,
            binding_name: binding.local_name,
        });
    }
    table.bindings.insert(binding.local_name.clone(), binding);
    Ok(())
}

fn binding_from_target(
    local_name: String,
    target: ResolvedPathTarget,
) -> HirImportBinding {
    match target {
        ResolvedPathTarget::Scope {
            file_id,
            target_path,
            root_source,
        } => HirImportBinding {
            local_name,
            kind: HirImportBindingKind::Scope,
            target_file_id: file_id,
            target_path,
            target_item: None,
            source_root: named_root_name(&root_source),
        },
        ResolvedPathTarget::Item {
            item_ref,
            target_path,
            root_source,
        } => HirImportBinding {
            local_name,
            kind: HirImportBindingKind::Item,
            target_file_id: item_ref.file_id,
            target_path,
            target_item: Some(item_ref),
            source_root: named_root_name(&root_source),
        },
    }
}

fn prefixed_path(prefix: &[String], path: &[String]) -> Vec<String> {
    let mut combined = Vec::with_capacity(prefix.len() + path.len());
    combined.extend(prefix.iter().cloned());
    combined.extend(path.iter().cloned());
    combined
}

fn unresolved_path(from_file_id: FileId, path: &[String]) -> HirImportError {
    HirImportError::UnresolvedPath {
        from_file_id,
        path: path.to_vec(),
    }
}

fn named_root_name(root_source: &ResolvedRootSource) -> Option<String> {
    match root_source {
        ResolvedRootSource::Current => None,
        ResolvedRootSource::Named(name) => Some(name.clone()),
    }
}

enum ResolvedPathTarget {
    Scope {
        file_id: FileId,
        target_path: Vec<String>,
        root_source: ResolvedRootSource,
    },
    Item {
        item_ref: HirItemRef,
        target_path: Vec<String>,
        root_source: ResolvedRootSource,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedRootSource {
    Current,
    Named(String),
}

struct ResolutionContext<'a> {
    graph: &'a ScopeGraph,
    scope_symbols: &'a BTreeMap<FileId, HirScopeSymbols>,
    #[allow(dead_code)]
    item_paths: &'a BTreeMap<Vec<String>, HirItemRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HirRootContext {
    graph: ScopeGraph,
    scope_symbols: BTreeMap<FileId, HirScopeSymbols>,
    item_paths: BTreeMap<Vec<String>, HirItemRef>,
}

enum RegisteredNamedRoot {
    Loaded(HirRootContext),
    UnloadedDependency,
}
