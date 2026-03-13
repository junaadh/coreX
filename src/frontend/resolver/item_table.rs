use super::item_ids::ItemId;
use super::model::ScopeGraph;
use crate::frontend::ParsedFile;
use crate::frontend::ast::Item;
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

/// Canonical top-level item kind tracked by semantic item tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Scope,
    Function,
    Struct,
    Enum,
    Protocol,
}

/// Canonical semantic item entry for one top-level declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalItem {
    pub id: ItemId,
    pub kind: ItemKind,
    pub name: String,
    pub defining_file_id: FileId,
    pub containing_scope_file_id: FileId,
    pub scope_path: Vec<String>,
    pub full_path: Vec<String>,
}

/// Deterministic canonical table of top-level declarations for a scope graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalItemTable {
    items_by_id: BTreeMap<ItemId, GlobalItem>,
    by_full_path: BTreeMap<Vec<String>, ItemId>,
    by_scope_file_id: BTreeMap<FileId, Vec<ItemId>>,
}

impl GlobalItemTable {
    /// Collects all top-level named items across resolved scopes.
    #[must_use]
    pub fn collect(graph: &ScopeGraph, parsed_files: &[ParsedFile]) -> Self {
        let parsed_by_id: BTreeMap<FileId, &ParsedFile> = parsed_files
            .iter()
            .map(|parsed| (parsed.file_id, parsed))
            .collect();

        let mut items_by_id = BTreeMap::new();
        let mut by_full_path = BTreeMap::new();
        let mut by_scope_file_id: BTreeMap<FileId, Vec<ItemId>> =
            BTreeMap::new();
        let mut next_raw_id = 0u32;

        for (scope_file_id, scope) in &graph.scopes {
            let Some(parsed) = parsed_by_id.get(scope_file_id) else {
                continue;
            };

            for item in &parsed.ast.items {
                let (name, kind) = match &item.node {
                    Item::Scope(scope_decl) => {
                        (scope_decl.node.name.clone(), ItemKind::Scope)
                    }
                    Item::Function(function_decl) => {
                        (function_decl.node.name.clone(), ItemKind::Function)
                    }
                    Item::Struct(struct_decl) => {
                        (struct_decl.node.name.clone(), ItemKind::Struct)
                    }
                    Item::Enum(enum_decl) => {
                        (enum_decl.node.name.clone(), ItemKind::Enum)
                    }
                    Item::Protocol(protocol_decl) => {
                        (protocol_decl.node.name.clone(), ItemKind::Protocol)
                    }
                    _ => continue,
                };

                let id = ItemId::new(next_raw_id);
                next_raw_id = next_raw_id.saturating_add(1);

                let mut full_path = scope.scope_path.clone();
                full_path.push(name.clone());

                let global_item = GlobalItem {
                    id,
                    kind,
                    name,
                    defining_file_id: parsed.file_id,
                    containing_scope_file_id: *scope_file_id,
                    scope_path: scope.scope_path.clone(),
                    full_path: full_path.clone(),
                };

                items_by_id.insert(id, global_item);
                by_scope_file_id.entry(*scope_file_id).or_default().push(id);
                // First-definition-wins for path lookup keeps lookup behavior
                // deterministic until duplicate declaration diagnostics land.
                by_full_path.entry(full_path).or_insert(id);
            }
        }

        Self {
            items_by_id,
            by_full_path,
            by_scope_file_id,
        }
    }

    #[must_use]
    pub fn get(&self, item_id: ItemId) -> Option<&GlobalItem> {
        self.items_by_id.get(&item_id)
    }

    #[must_use]
    pub fn get_by_full_path(
        &self,
        full_path: &[String],
    ) -> Option<&GlobalItem> {
        self.by_full_path
            .get(full_path)
            .and_then(|item_id| self.items_by_id.get(item_id))
    }

    #[must_use]
    pub fn item_id_by_full_path(&self, full_path: &[String]) -> Option<ItemId> {
        self.by_full_path.get(full_path).copied()
    }

    #[must_use]
    pub fn ids_in_scope(&self, scope_file_id: FileId) -> &[ItemId] {
        self.by_scope_file_id
            .get(&scope_file_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn items_in_scope(&self, scope_file_id: FileId) -> Vec<&GlobalItem> {
        self.ids_in_scope(scope_file_id)
            .iter()
            .filter_map(|item_id| self.items_by_id.get(item_id))
            .collect()
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &GlobalItem> {
        self.items_by_id.values()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items_by_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items_by_id.is_empty()
    }
}

#[must_use]
pub fn build_global_item_table(
    graph: &ScopeGraph,
    parsed_files: &[ParsedFile],
) -> GlobalItemTable {
    GlobalItemTable::collect(graph, parsed_files)
}
