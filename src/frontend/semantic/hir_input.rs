use crate::frontend::hir::{HirFile, HirModule, lower_to_hir};
use crate::frontend::resolver::{
    GlobalItemTable, HirImportTables, HirItemRef, HirItemTable,
    build_hir_item_table,
    build_hir_path_resolution_table_with_graph_and_imports,
};
use crate::frontend::source::FileId;
use crate::frontend::{DesugaredFile, ItemId, ScopeGraph};
use std::collections::BTreeMap;

/// HIR-backed resolver side tables consumed by semantic passes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticHirInput {
    pub hir_files: Vec<HirFile>,
    pub hir_modules: BTreeMap<FileId, HirModule>,
    pub hir_item_table: HirItemTable,
    pub hir_imports: HirImportTables,
    pub hir_path_table: crate::frontend::resolver::HirPathResolutionTable,
    pub item_id_by_hir_item_ref: BTreeMap<HirItemRef, ItemId>,
}

impl SemanticHirInput {
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
                    .expect("fallback HIR path table should build")
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

        Self {
            hir_files,
            hir_modules,
            hir_item_table,
            hir_imports,
            hir_path_table,
            item_id_by_hir_item_ref,
        }
    }
}
