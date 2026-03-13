use crate::frontend::DiagnosticsBag;
use crate::frontend::ParsedFile;
use crate::frontend::ast::{Item, UseTree};
use crate::frontend::diagnostic_from_import_resolve_error;
use crate::frontend::resolver::import_error::ImportResolveError;
use crate::frontend::resolver::symbols::{ScopeSymbols, Symbol, SymbolKind};
use crate::frontend::resolver::{ResolvedScope, ScopeGraph};
use crate::frontend::source::{FileId, SourceDb};
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

/// Named import root context used for cross-target or dependency imports.
#[derive(Debug, Clone)]
pub enum NamedImportRoot {
    LoadedLibrary {
        graph: ScopeGraph,
        parsed_files: Vec<ParsedFile>,
    },
    UnloadedDependency,
}

/// Project-local import resolver over a resolved scope graph.
pub struct ImportResolver<'a> {
    graph: &'a ScopeGraph,
    parsed_files: &'a [ParsedFile],
    scope_symbols: &'a BTreeMap<FileId, ScopeSymbols>,
    named_roots: BTreeMap<String, RegisteredNamedRoot<'a>>,
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
            named_roots: BTreeMap::new(),
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

    pub fn resolve_imports_with_diagnostics(
        &self,
        db: &SourceDb,
    ) -> (BTreeMap<FileId, ResolvedImports>, DiagnosticsBag) {
        let parsed_by_id = self.parsed_file_by_id();
        let parent_map = self.parent_map();
        let mut all = BTreeMap::new();
        let mut diagnostics = DiagnosticsBag::new();

        for file_id in self.graph.scopes.keys().copied() {
            let mut bindings = BTreeMap::new();

            if let Some(parsed) = parsed_by_id.get(&file_id) {
                for item in &parsed.ast.items {
                    if let Item::Use(use_item) = &item.node {
                        if let Err(error) = self.resolve_use_tree_into(
                            file_id,
                            file_id,
                            &[],
                            &use_item.node.tree.node,
                            &parent_map,
                            &mut bindings,
                        ) {
                            diagnostics.push(
                                diagnostic_from_import_resolve_error(
                                    db, &error,
                                ),
                            );
                        }
                    }
                }
            }

            all.insert(file_id, ResolvedImports { file_id, bindings });
        }

        (all, diagnostics)
    }

    fn register_named_loaded_root(
        &mut self,
        name: String,
        graph: &'a ScopeGraph,
        scope_symbols: BTreeMap<FileId, ScopeSymbols>,
    ) {
        self.named_roots.insert(
            name,
            RegisteredNamedRoot::Loaded {
                graph,
                scope_symbols,
            },
        );
    }

    fn register_named_unloaded_root(&mut self, name: String) {
        self.named_roots
            .insert(name, RegisteredNamedRoot::UnloadedDependency);
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

        let (target_scope_id, root_source) = match target {
            ResolvedPathTarget::Scope {
                file_id,
                root_source,
                ..
            } => (file_id, root_source),
            ResolvedPathTarget::Symbol { .. } => {
                return Err(ImportResolveError::InvalidGlobTarget {
                    from_file_id,
                    path: path.to_vec(),
                });
            }
        };

        let context =
            self.context_for_root_source(&root_source).ok_or_else(|| {
                ImportResolveError::UnresolvedPath {
                    from_file_id,
                    path: path.to_vec(),
                }
            })?;
        let target_scope = self
            .scope_by_id(context.graph, target_scope_id)
            .ok_or_else(|| ImportResolveError::UnresolvedPath {
                from_file_id,
                path: path.to_vec(),
            })?;

        let mut child_scopes = BTreeMap::new();
        for child_file_id in &target_scope.child_scope_ids {
            if let Some(child_scope) =
                self.scope_by_id(context.graph, *child_file_id)
            {
                child_scopes
                    .entry(child_scope.name.clone())
                    .or_insert(*child_file_id);
            }
        }

        for (child_name, child_file_id) in child_scopes {
            let child_scope = self
                .scope_by_id(context.graph, child_file_id)
                .ok_or_else(|| ImportResolveError::UnresolvedPath {
                    from_file_id,
                    path: path.to_vec(),
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

        if let Some(scope_symbols) = context.scope_symbols.get(&target_scope_id)
        {
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
        let mut root_source = ResolvedRootSource::Current;
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
                let named_root =
                    self.named_roots.get(other).ok_or_else(|| {
                        ImportResolveError::UnknownRoot {
                            from_file_id,
                            root: other.to_string(),
                        }
                    })?;
                match named_root {
                    RegisteredNamedRoot::Loaded { graph, .. } => {
                        root_source =
                            ResolvedRootSource::Named(other.to_string());
                        graph.root_file_id
                    }
                    RegisteredNamedRoot::UnloadedDependency => {
                        return Err(
                            ImportResolveError::UnloadedDependencyRoot {
                                from_file_id,
                                root: other.to_string(),
                            },
                        );
                    }
                }
            }
        };
        let context =
            self.context_for_root_source(&root_source).ok_or_else(|| {
                ImportResolveError::UnresolvedPath {
                    from_file_id,
                    path: path.to_vec(),
                }
            })?;

        if path.len() == 1 {
            let scope = self
                .scope_by_id(context.graph, current_scope_id)
                .ok_or_else(|| ImportResolveError::UnresolvedPath {
                    from_file_id,
                    path: path.to_vec(),
                })?;
            return Ok(ResolvedPathTarget::Scope {
                file_id: scope.file_id,
                target_path: scope.scope_path.clone(),
                root_source,
            });
        }

        for (idx, segment) in path.iter().enumerate().skip(1) {
            let is_last = idx + 1 == path.len();
            if !is_last {
                current_scope_id = self
                    .child_scope_named(context.graph, current_scope_id, segment)
                    .ok_or_else(|| ImportResolveError::UnresolvedPath {
                        from_file_id,
                        path: path.to_vec(),
                    })?;
                continue;
            }

            if let Some(child_scope_id) =
                self.child_scope_named(context.graph, current_scope_id, segment)
            {
                let child_scope = self
                    .scope_by_id(context.graph, child_scope_id)
                    .ok_or_else(|| ImportResolveError::UnresolvedPath {
                        from_file_id,
                        path: path.to_vec(),
                    })?;
                return Ok(ResolvedPathTarget::Scope {
                    file_id: child_scope_id,
                    target_path: child_scope.scope_path.clone(),
                    root_source: root_source.clone(),
                });
            }

            let symbol = self
                .lookup_symbol_in_scope(
                    context.scope_symbols,
                    current_scope_id,
                    segment,
                )
                .ok_or_else(|| ImportResolveError::UnresolvedPath {
                    from_file_id,
                    path: path.to_vec(),
                })?;
            let scope = self
                .scope_by_id(context.graph, current_scope_id)
                .ok_or_else(|| ImportResolveError::UnresolvedPath {
                    from_file_id,
                    path: path.to_vec(),
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
                ..
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

    fn context_for_root_source(
        &self,
        root_source: &ResolvedRootSource,
    ) -> Option<ResolutionContext<'_>> {
        match root_source {
            ResolvedRootSource::Current => Some(ResolutionContext {
                graph: self.graph,
                scope_symbols: self.scope_symbols,
            }),
            ResolvedRootSource::Named(name) => {
                let root = self.named_roots.get(name)?;
                match root {
                    RegisteredNamedRoot::Loaded {
                        graph,
                        scope_symbols,
                    } => Some(ResolutionContext {
                        graph,
                        scope_symbols,
                    }),
                    RegisteredNamedRoot::UnloadedDependency => None,
                }
            }
        }
    }

    fn scope_by_id<'s>(
        &self,
        graph: &'s ScopeGraph,
        file_id: FileId,
    ) -> Option<&'s ResolvedScope> {
        graph.scope(file_id)
    }

    fn child_scope_named(
        &self,
        graph: &ScopeGraph,
        file_id: FileId,
        name: &str,
    ) -> Option<FileId> {
        let scope = self.scope_by_id(graph, file_id)?;
        for child_file_id in &scope.child_scope_ids {
            let child_scope = self.scope_by_id(graph, *child_file_id)?;
            if child_scope.name == name {
                return Some(*child_file_id);
            }
        }
        None
    }

    fn lookup_symbol_in_scope<'s>(
        &self,
        scope_symbols: &'s BTreeMap<FileId, ScopeSymbols>,
        file_id: FileId,
        name: &str,
    ) -> Option<&'s Symbol> {
        scope_symbols.get(&file_id)?.get(name)
    }
}

enum ResolvedPathTarget {
    Scope {
        file_id: FileId,
        target_path: Vec<String>,
        root_source: ResolvedRootSource,
    },
    Symbol {
        kind: SymbolKind,
        file_id: FileId,
        target_path: Vec<String>,
    },
}

enum RegisteredNamedRoot<'a> {
    Loaded {
        graph: &'a ScopeGraph,
        scope_symbols: BTreeMap<FileId, ScopeSymbols>,
    },
    UnloadedDependency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedRootSource {
    Current,
    Named(String),
}

struct ResolutionContext<'a> {
    graph: &'a ScopeGraph,
    scope_symbols: &'a BTreeMap<FileId, ScopeSymbols>,
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
    let named_roots = BTreeMap::new();
    resolve_project_imports_with_named_roots(graph, parsed_files, &named_roots)
}

/// Resolves scope symbols/imports and supports additional named import roots.
pub fn resolve_project_imports_with_named_roots(
    graph: &ScopeGraph,
    parsed_files: &[ParsedFile],
    named_roots: &BTreeMap<String, NamedImportRoot>,
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
    let mut resolver = ImportResolver::new(graph, parsed_files, &scope_symbols);

    for (name, root) in named_roots {
        match root {
            NamedImportRoot::LoadedLibrary {
                graph,
                parsed_files,
            } => {
                let collector =
                    ImportResolver::new(graph, parsed_files, &empty);
                let symbols = collector.collect_scope_symbols();
                resolver.register_named_loaded_root(
                    name.clone(),
                    graph,
                    symbols,
                );
            }
            NamedImportRoot::UnloadedDependency => {
                resolver.register_named_unloaded_root(name.clone());
            }
        }
    }

    let resolved_imports = resolver.resolve_imports()?;
    Ok((scope_symbols, resolved_imports))
}

/// Resolves imports with named roots and accumulates diagnostics.
pub fn resolve_project_imports_with_named_roots_and_diagnostics(
    graph: &ScopeGraph,
    parsed_files: &[ParsedFile],
    named_roots: &BTreeMap<String, NamedImportRoot>,
    db: &SourceDb,
) -> (
    BTreeMap<FileId, ScopeSymbols>,
    BTreeMap<FileId, ResolvedImports>,
    DiagnosticsBag,
) {
    let empty = BTreeMap::new();
    let collector = ImportResolver::new(graph, parsed_files, &empty);
    let scope_symbols = collector.collect_scope_symbols();
    let mut resolver = ImportResolver::new(graph, parsed_files, &scope_symbols);

    for (name, root) in named_roots {
        match root {
            NamedImportRoot::LoadedLibrary {
                graph,
                parsed_files,
            } => {
                let collector =
                    ImportResolver::new(graph, parsed_files, &empty);
                let symbols = collector.collect_scope_symbols();
                resolver.register_named_loaded_root(
                    name.clone(),
                    graph,
                    symbols,
                );
            }
            NamedImportRoot::UnloadedDependency => {
                resolver.register_named_unloaded_root(name.clone());
            }
        }
    }

    let (resolved_imports, diagnostics) =
        resolver.resolve_imports_with_diagnostics(db);
    (scope_symbols, resolved_imports, diagnostics)
}
