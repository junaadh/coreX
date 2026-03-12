use crate::frontend::ParsedFile;
use crate::frontend::ast::{Item, UseTree};
use crate::frontend::resolver::import_error::ImportResolveError;
use crate::frontend::resolver::symbols::{ScopeSymbols, Symbol, SymbolKind};
use crate::frontend::resolver::{ResolvedScope, ScopeGraph};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

/// Imported binding category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportBindingKind {
    Scope,
    Symbol(SymbolKind),
}

/// One resolved import binding in a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImportBinding {
    pub local_name: String,
    pub kind: ImportBindingKind,
    pub target_file_id: FileId,
    pub target_path: Vec<String>,
}

/// Resolved imports for a single scope file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImports {
    pub file_id: FileId,
    pub bindings: BTreeMap<String, ResolvedImportBinding>,
}

impl ResolvedImports {
    /// Returns a resolved binding by local name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ResolvedImportBinding> {
        self.bindings.get(name)
    }

    /// Returns the number of resolved bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns true when no bindings were resolved.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// Project-local import resolver over a resolved scope graph.
pub struct ImportResolver<'a> {
    graph: &'a ScopeGraph,
    parsed_files: &'a [ParsedFile],
    scope_symbols: &'a BTreeMap<FileId, ScopeSymbols>,
}

impl<'a> ImportResolver<'a> {
    /// Creates an import resolver.
    #[must_use]
    pub fn new(
        graph: &'a ScopeGraph,
        parsed_files: &'a [ParsedFile],
        scope_symbols: &'a BTreeMap<FileId, ScopeSymbols>,
    ) -> Self {
        Self {
            graph,
            parsed_files,
            scope_symbols,
        }
    }

    /// Collects top-level scope symbols for all scopes in the graph.
    pub fn collect_scope_symbols(&self) -> BTreeMap<FileId, ScopeSymbols> {
        let parsed_by_id = self.parsed_file_by_id();
        let mut tables = BTreeMap::new();

        for file_id in self.graph.scopes.keys().copied() {
            let symbols = match parsed_by_id.get(&file_id) {
                Some(parsed) => self.collect_symbols_for_file(parsed),
                None => ScopeSymbols {
                    file_id,
                    symbols: BTreeMap::new(),
                },
            };
            tables.insert(file_id, symbols);
        }

        tables
    }

    /// Resolves all `use` trees for each scope in the graph.
    pub fn resolve_imports(
        &self,
    ) -> Result<BTreeMap<FileId, ResolvedImports>, ImportResolveError> {
        let parsed_by_id = self.parsed_file_by_id();
        let parent_map = self.parent_map();
        let mut all = BTreeMap::new();

        for file_id in self.graph.scopes.keys().copied() {
            let mut bindings = BTreeMap::new();

            if let Some(parsed) = parsed_by_id.get(&file_id) {
                for item in &parsed.ast.items {
                    if let Item::Use(use_item) = &item.node {
                        self.resolve_use_tree_into(
                            file_id,
                            file_id,
                            &[],
                            &use_item.node.tree.node,
                            &parent_map,
                            &mut bindings,
                        )?;
                    }
                }
            }

            all.insert(file_id, ResolvedImports { file_id, bindings });
        }

        Ok(all)
    }

    fn parsed_file_by_id(&self) -> BTreeMap<FileId, &ParsedFile> {
        self.parsed_files
            .iter()
            .map(|parsed| (parsed.file_id, parsed))
            .collect()
    }

    fn collect_symbols_for_file(&self, parsed: &ParsedFile) -> ScopeSymbols {
        let mut symbols = BTreeMap::new();

        for item in &parsed.ast.items {
            let (name, kind) = match &item.node {
                Item::Scope(scope_decl) => {
                    (scope_decl.node.name.clone(), SymbolKind::Scope)
                }
                Item::Function(function) => {
                    (function.node.name.clone(), SymbolKind::Function)
                }
                Item::Struct(struct_decl) => {
                    (struct_decl.node.name.clone(), SymbolKind::Struct)
                }
                Item::Enum(enum_decl) => {
                    (enum_decl.node.name.clone(), SymbolKind::Enum)
                }
                Item::Protocol(protocol_decl) => {
                    (protocol_decl.node.name.clone(), SymbolKind::Protocol)
                }
                _ => continue,
            };

            symbols.entry(name.clone()).or_insert(Symbol {
                name,
                kind,
                defining_file_id: parsed.file_id,
            });
        }

        ScopeSymbols {
            file_id: parsed.file_id,
            symbols,
        }
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
        tree: &UseTree,
        parent_map: &BTreeMap<FileId, FileId>,
        bindings: &mut BTreeMap<String, ResolvedImportBinding>,
    ) -> Result<(), ImportResolveError> {
        match tree {
            UseTree::Path { path } => {
                let path = self.prefixed_path(prefix, &path.segments);
                self.resolve_and_bind_path(
                    from_file_id,
                    current_scope_id,
                    &path,
                    None,
                    parent_map,
                    bindings,
                )
            }
            UseTree::Alias { path, alias } => {
                let path = self.prefixed_path(prefix, &path.segments);
                self.resolve_and_bind_path(
                    from_file_id,
                    current_scope_id,
                    &path,
                    Some(alias.clone()),
                    parent_map,
                    bindings,
                )
            }
            UseTree::Glob { path } => {
                let path = self.prefixed_path(prefix, &path.segments);
                self.resolve_glob(
                    from_file_id,
                    current_scope_id,
                    &path,
                    parent_map,
                    bindings,
                )
            }
            UseTree::Group { path, items } => {
                let mut next_prefix = prefix.to_vec();
                if let Some(path) = path {
                    next_prefix.extend(path.segments.iter().cloned());
                }

                for item in items {
                    self.resolve_use_tree_into(
                        from_file_id,
                        current_scope_id,
                        &next_prefix,
                        &item.node,
                        parent_map,
                        bindings,
                    )?;
                }
                Ok(())
            }
            UseTree::SelfImport => {
                if prefix.is_empty() {
                    return Err(ImportResolveError::InvalidSelfImport {
                        from_file_id,
                    });
                }
                self.resolve_and_bind_path(
                    from_file_id,
                    current_scope_id,
                    prefix,
                    None,
                    parent_map,
                    bindings,
                )
            }
        }
    }

    fn prefixed_path(&self, prefix: &[String], path: &[String]) -> Vec<String> {
        let mut combined = Vec::with_capacity(prefix.len() + path.len());
        combined.extend(prefix.iter().cloned());
        combined.extend(path.iter().cloned());
        combined
    }

    fn resolve_and_bind_path(
        &self,
        from_file_id: FileId,
        current_scope_id: FileId,
        path: &[String],
        alias: Option<String>,
        parent_map: &BTreeMap<FileId, FileId>,
        bindings: &mut BTreeMap<String, ResolvedImportBinding>,
    ) -> Result<(), ImportResolveError> {
        let resolved = self.resolve_use_path(
            from_file_id,
            current_scope_id,
            path,
            parent_map,
        )?;
        let local_name = match alias {
            Some(alias) => alias,
            None => path.last().cloned().ok_or(
                ImportResolveError::UnresolvedPath {
                    from_file_id,
                    path: path.to_vec(),
                },
            )?,
        };
        let binding = self.binding_from_target(local_name, resolved);
        self.insert_binding_checked(from_file_id, bindings, binding)
    }

    fn resolve_glob(
        &self,
        from_file_id: FileId,
        current_scope_id: FileId,
        path: &[String],
        parent_map: &BTreeMap<FileId, FileId>,
        bindings: &mut BTreeMap<String, ResolvedImportBinding>,
    ) -> Result<(), ImportResolveError> {
        let target = self.resolve_use_path(
            from_file_id,
            current_scope_id,
            path,
            parent_map,
        )?;

        let target_scope_id = match target {
            ResolvedPathTarget::Scope { file_id, .. } => file_id,
            ResolvedPathTarget::Symbol { .. } => {
                return Err(ImportResolveError::InvalidGlobTarget {
                    from_file_id,
                    path: path.to_vec(),
                });
            }
        };

        let target_scope =
            self.scope_by_id(target_scope_id).ok_or_else(|| {
                ImportResolveError::UnresolvedPath {
                    from_file_id,
                    path: path.to_vec(),
                }
            })?;

        let mut child_scopes = BTreeMap::new();
        for child_file_id in &target_scope.child_scope_ids {
            if let Some(child_scope) = self.scope_by_id(*child_file_id) {
                child_scopes
                    .entry(child_scope.name.clone())
                    .or_insert(*child_file_id);
            }
        }

        for (child_name, child_file_id) in child_scopes {
            let child_scope =
                self.scope_by_id(child_file_id).ok_or_else(|| {
                    ImportResolveError::UnresolvedPath {
                        from_file_id,
                        path: path.to_vec(),
                    }
                })?;
            self.insert_binding_checked(
                from_file_id,
                bindings,
                ResolvedImportBinding {
                    local_name: child_name.clone(),
                    kind: ImportBindingKind::Scope,
                    target_file_id: child_file_id,
                    target_path: child_scope.scope_path.clone(),
                },
            )?;
        }

        if let Some(scope_symbols) = self.scope_symbols.get(&target_scope_id) {
            for (name, symbol) in &scope_symbols.symbols {
                if symbol.kind == SymbolKind::Scope {
                    continue;
                }

                let mut target_path = target_scope.scope_path.clone();
                target_path.push(name.clone());
                self.insert_binding_checked(
                    from_file_id,
                    bindings,
                    ResolvedImportBinding {
                        local_name: name.clone(),
                        kind: ImportBindingKind::Symbol(symbol.kind),
                        target_file_id: symbol.defining_file_id,
                        target_path,
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
    ) -> Result<ResolvedPathTarget, ImportResolveError> {
        if path.is_empty() {
            return Err(ImportResolveError::UnresolvedPath {
                from_file_id,
                path: path.to_vec(),
            });
        }

        if path.len() > 1 && path[1..].iter().any(|segment| segment == "self") {
            return Err(ImportResolveError::InvalidSelfImport { from_file_id });
        }

        let first = &path[0];
        let mut current_scope_id = match first.as_str() {
            "root" => self.graph.root_file_id,
            "self" => current_scope_id,
            "super" => {
                parent_map.get(&current_scope_id).copied().ok_or_else(|| {
                    ImportResolveError::UnresolvedPath {
                        from_file_id,
                        path: path.to_vec(),
                    }
                })?
            }
            other => {
                return Err(ImportResolveError::UnknownRoot {
                    from_file_id,
                    root: other.to_string(),
                });
            }
        };

        if path.len() == 1 {
            let scope =
                self.scope_by_id(current_scope_id).ok_or_else(|| {
                    ImportResolveError::UnresolvedPath {
                        from_file_id,
                        path: path.to_vec(),
                    }
                })?;
            return Ok(ResolvedPathTarget::Scope {
                file_id: scope.file_id,
                target_path: scope.scope_path.clone(),
            });
        }

        for (idx, segment) in path.iter().enumerate().skip(1) {
            let is_last = idx + 1 == path.len();
            if !is_last {
                current_scope_id = self
                    .child_scope_named(current_scope_id, segment)
                    .ok_or_else(|| ImportResolveError::UnresolvedPath {
                        from_file_id,
                        path: path.to_vec(),
                    })?;
                continue;
            }

            if let Some(child_scope_id) =
                self.child_scope_named(current_scope_id, segment)
            {
                let child_scope =
                    self.scope_by_id(child_scope_id).ok_or_else(|| {
                        ImportResolveError::UnresolvedPath {
                            from_file_id,
                            path: path.to_vec(),
                        }
                    })?;
                return Ok(ResolvedPathTarget::Scope {
                    file_id: child_scope_id,
                    target_path: child_scope.scope_path.clone(),
                });
            }

            let symbol = self
                .lookup_symbol_in_scope(current_scope_id, segment)
                .ok_or_else(|| ImportResolveError::UnresolvedPath {
                    from_file_id,
                    path: path.to_vec(),
                })?;
            let scope =
                self.scope_by_id(current_scope_id).ok_or_else(|| {
                    ImportResolveError::UnresolvedPath {
                        from_file_id,
                        path: path.to_vec(),
                    }
                })?;
            let mut target_path = scope.scope_path.clone();
            target_path.push(segment.clone());
            return Ok(ResolvedPathTarget::Symbol {
                kind: symbol.kind,
                file_id: symbol.defining_file_id,
                target_path,
            });
        }

        Err(ImportResolveError::UnresolvedPath {
            from_file_id,
            path: path.to_vec(),
        })
    }

    fn binding_from_target(
        &self,
        local_name: String,
        target: ResolvedPathTarget,
    ) -> ResolvedImportBinding {
        match target {
            ResolvedPathTarget::Scope {
                file_id,
                target_path,
            } => ResolvedImportBinding {
                local_name,
                kind: ImportBindingKind::Scope,
                target_file_id: file_id,
                target_path,
            },
            ResolvedPathTarget::Symbol {
                kind,
                file_id,
                target_path,
            } => ResolvedImportBinding {
                local_name,
                kind: ImportBindingKind::Symbol(kind),
                target_file_id: file_id,
                target_path,
            },
        }
    }

    fn insert_binding_checked(
        &self,
        file_id: FileId,
        bindings: &mut BTreeMap<String, ResolvedImportBinding>,
        binding: ResolvedImportBinding,
    ) -> Result<(), ImportResolveError> {
        if bindings.contains_key(&binding.local_name) {
            return Err(ImportResolveError::DuplicateBinding {
                file_id,
                binding_name: binding.local_name,
            });
        }

        bindings.insert(binding.local_name.clone(), binding);
        Ok(())
    }

    fn scope_by_id(&self, file_id: FileId) -> Option<&ResolvedScope> {
        self.graph.scope(file_id)
    }

    fn child_scope_named(&self, file_id: FileId, name: &str) -> Option<FileId> {
        let scope = self.scope_by_id(file_id)?;
        for child_file_id in &scope.child_scope_ids {
            let child_scope = self.scope_by_id(*child_file_id)?;
            if child_scope.name == name {
                return Some(*child_file_id);
            }
        }
        None
    }

    fn lookup_symbol_in_scope(
        &self,
        file_id: FileId,
        name: &str,
    ) -> Option<&Symbol> {
        self.scope_symbols.get(&file_id)?.get(name)
    }
}

enum ResolvedPathTarget {
    Scope {
        file_id: FileId,
        target_path: Vec<String>,
    },
    Symbol {
        kind: SymbolKind,
        file_id: FileId,
        target_path: Vec<String>,
    },
}

/// Resolves scope symbols and project-local imports for a resolved scope graph.
pub fn resolve_project_imports(
    graph: &ScopeGraph,
    parsed_files: &[ParsedFile],
) -> Result<
    (
        BTreeMap<FileId, ScopeSymbols>,
        BTreeMap<FileId, ResolvedImports>,
    ),
    ImportResolveError,
> {
    let empty = BTreeMap::new();
    let collector = ImportResolver::new(graph, parsed_files, &empty);
    let scope_symbols = collector.collect_scope_symbols();
    let resolver = ImportResolver::new(graph, parsed_files, &scope_symbols);
    let resolved_imports = resolver.resolve_imports()?;
    Ok((scope_symbols, resolved_imports))
}
