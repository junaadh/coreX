use crate::frontend::hir::{
    HirBodyId, HirFile, HirItemKind, HirModule, lower_to_hir,
};
use crate::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, HirImportTables, HirItemRef,
    HirItemTable, HirLocalBindingTable, LocalId, build_hir_item_table,
    build_hir_local_binding_table,
    build_hir_path_resolution_table_with_graph_and_imports,
};
use crate::frontend::source::FileId;
use crate::frontend::{DesugaredFile, ItemId, ScopeGraph};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticBodyRef {
    pub file_id: FileId,
    pub body_id: HirBodyId,
}

/// HIR-backed resolver side tables consumed by semantic passes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticHirInput {
    pub hir_files: Vec<HirFile>,
    pub hir_modules: BTreeMap<FileId, HirModule>,
    pub hir_item_table: HirItemTable,
    pub hir_local_bindings: HirLocalBindingTable,
    pub hir_imports: HirImportTables,
    pub hir_path_table: crate::frontend::resolver::HirPathResolutionTable,
    pub item_id_by_hir_item_ref: BTreeMap<HirItemRef, ItemId>,
    body_refs_by_owner: BTreeMap<DeclarationOwner, Vec<SemanticBodyRef>>,
    local_binding_ids_by_body: BTreeMap<(FileId, HirBodyId), Vec<LocalId>>,
}

impl SemanticHirInput {
    #[must_use]
    pub fn body_refs_for_owner(
        &self,
        owner: &DeclarationOwner,
    ) -> &[SemanticBodyRef] {
        self.body_refs_by_owner
            .get(owner)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn body_ref(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Option<SemanticBodyRef> {
        self.body_refs_for_owner(owner).get(body_index).copied()
    }

    #[must_use]
    pub fn local_binding_ids_for_body(
        &self,
        body_ref: SemanticBodyRef,
    ) -> &[LocalId] {
        self.local_binding_ids_by_body
            .get(&(body_ref.file_id, body_ref.body_id))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn map_hir_local_ids_to_resolved(
        &self,
        module: &HirModule,
        body: &crate::frontend::resolver::ResolvedBody,
        body_ref: SemanticBodyRef,
    ) -> BTreeMap<LocalId, LocalId> {
        let mut mapping = BTreeMap::new();
        let mut consumed = vec![false; body.locals.len()];

        for hir_local_id in self.local_binding_ids_for_body(body_ref) {
            let Some(hir_binding) =
                self.hir_local_bindings.binding(*hir_local_id)
            else {
                continue;
            };

            let declared_span = hir_binding.declared_pat.and_then(|pat_id| {
                module.patterns.get(&pat_id).map(|pat| pat.origin.span)
            });

            let mut matched_index = declared_span.and_then(|span| {
                body.locals.iter().enumerate().find_map(|(index, local)| {
                    (!consumed[index]
                        && local.declared_span == span
                        && local.name == hir_binding.name
                        && local.kind == hir_binding.kind
                        && local.mutability == hir_binding.mutability)
                        .then_some(index)
                })
            });

            if matched_index.is_none() {
                matched_index = body.locals.iter().enumerate().find_map(
                    |(index, local)| {
                        (!consumed[index]
                            && local.name == hir_binding.name
                            && local.kind == hir_binding.kind
                            && local.mutability == hir_binding.mutability)
                            .then_some(index)
                    },
                );
            }

            if let Some(index) = matched_index {
                consumed[index] = true;
                mapping.insert(*hir_local_id, body.locals[index].id);
            }
        }

        mapping
    }

    /// Builds HIR artifacts and resolver tables for semantic analysis.
    ///
    /// This is intentionally best-effort for import resolution: unresolved
    /// imports are captured in diagnostics by resolver stages, while semantic
    /// type checking still runs on available data.
    #[must_use]
    pub fn build(
        graph: &ScopeGraph,
        parsed_files: &[DesugaredFile],
        global_items: &GlobalItemTable,
    ) -> Self {
        let mut hir_files = Vec::new();
        let mut hir_modules = BTreeMap::new();

        for parsed in parsed_files {
            let (hir_file, hir_module) = lower_to_hir(parsed);
            hir_modules.insert(hir_file.file_id, hir_module);
            hir_files.push(hir_file);
        }

        let hir_item_table = build_hir_item_table(&hir_files, &hir_modules)
            .unwrap_or_else(|_| {
                HirItemTable::collect(&[], &BTreeMap::new())
                    .expect("empty hir item table should collect")
            });
        let hir_local_bindings =
            build_hir_local_binding_table(&hir_files, &hir_modules)
                .unwrap_or_else(|_| {
                    build_hir_local_binding_table(&[], &BTreeMap::new())
                        .expect("empty hir local binding table should collect")
                });

        let hir_imports = HirImportTables::resolve_with_graph(
            graph,
            &hir_files,
            &hir_modules,
            &hir_item_table,
        )
        .or_else(|_| {
            HirImportTables::resolve_with_graph_and_named_roots_and_diagnostics(
                graph,
                &hir_files,
                &hir_modules,
                &hir_item_table,
                &BTreeMap::new(),
            )
            .map(|(tables, _)| tables)
        })
        .unwrap_or_else(|_| HirImportTables::new());

        let hir_path_table =
            build_hir_path_resolution_table_with_graph_and_imports(
                &hir_files,
                &hir_modules,
                graph,
                Some(&hir_imports),
            )
            .unwrap_or_else(|_| {
                build_hir_path_resolution_table_with_graph_and_imports(
                    &hir_files,
                    &hir_modules,
                    graph,
                    None,
                )
                .unwrap_or_else(|_| {
                    crate::frontend::resolver::build_hir_path_resolution_table(
                        &hir_files,
                        &hir_modules,
                    )
                    .unwrap_or_else(|_| {
                        crate::frontend::resolver::HirPathResolutionTable::empty()
                    })
                })
            });

        let mut item_id_by_hir_item_ref = BTreeMap::new();
        if let Some(item_paths) = hir_imports.item_paths_for_root(None) {
            for (full_path, item_ref) in item_paths {
                if let Some(item_id) =
                    global_items.item_id_by_full_path(full_path)
                {
                    item_id_by_hir_item_ref.insert(*item_ref, item_id);
                }
            }
        }
        let body_refs_by_owner = collect_body_refs_by_owner(
            graph,
            global_items,
            &hir_files,
            &hir_modules,
            &item_id_by_hir_item_ref,
        );
        let mut local_binding_ids_by_body = BTreeMap::new();
        for binding in hir_local_bindings.iter_bindings() {
            local_binding_ids_by_body
                .entry((binding.file_id, binding.body_id))
                .or_insert_with(Vec::new)
                .push(binding.id);
        }

        Self {
            hir_files,
            hir_modules,
            hir_item_table,
            hir_local_bindings,
            hir_imports,
            hir_path_table,
            item_id_by_hir_item_ref,
            body_refs_by_owner,
            local_binding_ids_by_body,
        }
    }
}

fn collect_body_refs_by_owner(
    graph: &ScopeGraph,
    global_items: &GlobalItemTable,
    hir_files: &[HirFile],
    hir_modules: &BTreeMap<FileId, HirModule>,
    item_id_by_hir_item_ref: &BTreeMap<HirItemRef, ItemId>,
) -> BTreeMap<DeclarationOwner, Vec<SemanticBodyRef>> {
    let mut body_refs_by_owner: BTreeMap<
        DeclarationOwner,
        Vec<SemanticBodyRef>,
    > = BTreeMap::new();

    for hir_file in hir_files {
        let Some(module) = hir_modules.get(&hir_file.file_id) else {
            continue;
        };
        let mut impl_index = 0usize;

        for item_id in &hir_file.root_items {
            let item_ref = HirItemRef::new(hir_file.file_id, *item_id);
            let Some(item) = module.items.get(item_id) else {
                continue;
            };

            match &item.kind {
                HirItemKind::Function(function) => {
                    if let Some(owner_item_id) = owner_item_id_for_hir_item(
                        graph,
                        global_items,
                        hir_file.file_id,
                        &item.kind,
                        &item_ref,
                        item_id_by_hir_item_ref,
                    ) {
                        body_refs_by_owner
                            .entry(DeclarationOwner::Item(owner_item_id))
                            .or_default()
                            .push(SemanticBodyRef {
                                file_id: hir_file.file_id,
                                body_id: function.body,
                            });
                    }
                }
                HirItemKind::Struct(struct_decl) => {
                    if let Some(owner_item_id) = owner_item_id_for_hir_item(
                        graph,
                        global_items,
                        hir_file.file_id,
                        &item.kind,
                        &item_ref,
                        item_id_by_hir_item_ref,
                    ) {
                        let owner = DeclarationOwner::Item(owner_item_id);
                        for function in &struct_decl.functions {
                            body_refs_by_owner
                                .entry(owner.clone())
                                .or_default()
                                .push(SemanticBodyRef {
                                    file_id: hir_file.file_id,
                                    body_id: function.body,
                                });
                        }
                    }
                }
                HirItemKind::Enum(enum_decl) => {
                    if let Some(owner_item_id) = owner_item_id_for_hir_item(
                        graph,
                        global_items,
                        hir_file.file_id,
                        &item.kind,
                        &item_ref,
                        item_id_by_hir_item_ref,
                    ) {
                        let owner = DeclarationOwner::Item(owner_item_id);
                        for function in &enum_decl.functions {
                            body_refs_by_owner
                                .entry(owner.clone())
                                .or_default()
                                .push(SemanticBodyRef {
                                    file_id: hir_file.file_id,
                                    body_id: function.body,
                                });
                        }
                    }
                }
                HirItemKind::Protocol(protocol_decl) => {
                    if let Some(owner_item_id) = owner_item_id_for_hir_item(
                        graph,
                        global_items,
                        hir_file.file_id,
                        &item.kind,
                        &item_ref,
                        item_id_by_hir_item_ref,
                    ) {
                        let owner = DeclarationOwner::Item(owner_item_id);
                        for function in &protocol_decl.functions {
                            if let Some(default_body) = function.default_body {
                                body_refs_by_owner
                                    .entry(owner.clone())
                                    .or_default()
                                    .push(SemanticBodyRef {
                                        file_id: hir_file.file_id,
                                        body_id: default_body,
                                    });
                            }
                        }
                    }
                }
                HirItemKind::Impl(impl_decl) => {
                    let owner = DeclarationOwner::Impl {
                        scope_file_id: hir_file.file_id,
                        impl_index,
                    };
                    impl_index = impl_index.saturating_add(1);
                    for function in &impl_decl.functions {
                        body_refs_by_owner
                            .entry(owner.clone())
                            .or_default()
                            .push(SemanticBodyRef {
                                file_id: hir_file.file_id,
                                body_id: function.body,
                            });
                    }
                }
                HirItemKind::Extern(_) | HirItemKind::Use(_) => {}
            }
        }
    }

    body_refs_by_owner
}

fn owner_item_id_for_hir_item(
    graph: &ScopeGraph,
    global_items: &GlobalItemTable,
    file_id: FileId,
    item_kind: &HirItemKind,
    item_ref: &HirItemRef,
    item_id_by_hir_item_ref: &BTreeMap<HirItemRef, ItemId>,
) -> Option<ItemId> {
    if let Some(item_id) = item_id_by_hir_item_ref.get(item_ref).copied() {
        return Some(item_id);
    }

    let name = match item_kind {
        HirItemKind::Function(function) => &function.name,
        HirItemKind::Struct(struct_decl) => &struct_decl.name,
        HirItemKind::Enum(enum_decl) => &enum_decl.name,
        HirItemKind::Protocol(protocol_decl) => &protocol_decl.name,
        HirItemKind::Impl(_) | HirItemKind::Extern(_) | HirItemKind::Use(_) => {
            return None;
        }
    };

    let scope = graph.scope(file_id)?;
    let mut full_path = scope.scope_path.clone();
    full_path.push(name.clone());
    let item_id = global_items.item_id_by_full_path(&full_path)?;
    let item = global_items.get(item_id)?;
    (item.containing_scope_file_id == file_id).then_some(item_id)
}
