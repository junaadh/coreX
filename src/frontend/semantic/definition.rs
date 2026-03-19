use super::analysis::SemanticAnalysis;
use super::external_lookup::{
    ExternalDefinitionLocation, ExternalSemanticLookup,
};
use super::types::Type;
use crate::frontend::DesugaredFile;
use crate::frontend::ast::{Item, Span};
use crate::frontend::resolver::{
    GlobalItemTable, ImportBindingKind, ItemId, LocalId, ResolvedBodyRef,
    ResolvedImportBinding, ResolvedImports, ScopeGraph,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

/// Local definition location inside current semantic analysis database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionLocation {
    pub file_id: FileId,
    pub span: Span,
}

/// Semantic completion entry returned by semantic query helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCompletionCandidate {
    pub label: String,
    pub kind: SemanticCompletionKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticCompletionKind {
    Local,
    ImportScope,
    ImportFunction,
    ImportStruct,
    ImportEnum,
    ImportProtocol,
    Scope,
    Function,
    Struct,
    Enum,
    Protocol,
}

/// Unified semantic definition target across local/current/external roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionTarget {
    LocalBinding {
        local_id: LocalId,
        location: DefinitionLocation,
    },
    CurrentTargetItem {
        item_id: ItemId,
        location: DefinitionLocation,
    },
    ExternalItem {
        root_name: String,
        path: Vec<String>,
        location: ExternalDefinitionLocation,
    },
}

/// Reusable semantic definition lookup over resolved semantic tables.
pub struct SemanticDefinitionLookup<'a> {
    semantic: &'a SemanticAnalysis,
    imports: &'a BTreeMap<FileId, ResolvedImports>,
    external_lookup: &'a ExternalSemanticLookup,
    item_definitions: &'a BTreeMap<ItemId, DefinitionLocation>,
}

impl<'a> SemanticDefinitionLookup<'a> {
    #[must_use]
    pub fn new(
        semantic: &'a SemanticAnalysis,
        imports: &'a BTreeMap<FileId, ResolvedImports>,
        external_lookup: &'a ExternalSemanticLookup,
        item_definitions: &'a BTreeMap<ItemId, DefinitionLocation>,
    ) -> Self {
        Self {
            semantic,
            imports,
            external_lookup,
            item_definitions,
        }
    }

    #[must_use]
    pub fn lookup_definition_target(
        &self,
        file_id: FileId,
        offset: usize,
        fallback_word: Option<&str>,
    ) -> Option<DefinitionTarget> {
        if let Some(target) =
            self.definition_from_body_reference(file_id, offset)
        {
            return Some(target);
        }
        if let Some(target) =
            self.definition_from_unresolved_reference(file_id, offset)
        {
            return Some(target);
        }
        if let Some(word) = fallback_word {
            if let Some(target) =
                self.definition_from_import_word(file_id, word)
            {
                return Some(target);
            }
            if let Some(target) = self.definition_from_scope_word(file_id, word)
            {
                return Some(target);
            }
        }
        None
    }

    fn definition_from_body_reference(
        &self,
        file_id: FileId,
        offset: usize,
    ) -> Option<DefinitionTarget> {
        for body in self.semantic.resolved_bodies.iter() {
            if body.containing_scope_file_id != file_id {
                continue;
            }
            for reference in &body.references {
                if !span_contains(reference.span, offset) {
                    continue;
                }
                match reference.resolved {
                    ResolvedBodyRef::Local(local_id) => {
                        let local = body
                            .locals
                            .iter()
                            .find(|item| item.id == local_id)?;
                        return Some(DefinitionTarget::LocalBinding {
                            local_id,
                            location: DefinitionLocation {
                                file_id: body.containing_scope_file_id,
                                span: local.declared_span,
                            },
                        });
                    }
                    ResolvedBodyRef::Item(item_id)
                    | ResolvedBodyRef::Import(item_id) => {
                        let location = *self.item_definitions.get(&item_id)?;
                        return Some(DefinitionTarget::CurrentTargetItem {
                            item_id,
                            location,
                        });
                    }
                    ResolvedBodyRef::Unresolved => {}
                }
            }
        }
        None
    }

    fn definition_from_unresolved_reference(
        &self,
        file_id: FileId,
        offset: usize,
    ) -> Option<DefinitionTarget> {
        for body in self.semantic.resolved_bodies.iter() {
            if body.containing_scope_file_id != file_id {
                continue;
            }
            for unresolved in &body.unresolved_references {
                if !span_contains(unresolved.span, offset) {
                    continue;
                }
                if let Some(target) =
                    self.external_definition_from_segments(&unresolved.segments)
                {
                    return Some(target);
                }
            }
        }
        None
    }

    fn definition_from_import_word(
        &self,
        file_id: FileId,
        word: &str,
    ) -> Option<DefinitionTarget> {
        let binding = self.imports.get(&file_id)?.get(word)?;
        if let Some(item_id) = self
            .semantic
            .global_items
            .item_id_by_full_path(&binding.target_path)
            && let Some(location) = self.item_definitions.get(&item_id).copied()
        {
            return Some(DefinitionTarget::CurrentTargetItem {
                item_id,
                location,
            });
        }
        self.external_definition_from_binding(binding)
    }

    fn definition_from_scope_word(
        &self,
        file_id: FileId,
        word: &str,
    ) -> Option<DefinitionTarget> {
        self.semantic
            .global_items
            .items_in_scope(file_id)
            .into_iter()
            .find_map(|item| {
                (item.name == word).then_some(item.id).and_then(|item_id| {
                    self.item_definitions.get(&item_id).copied().map(
                        |location| DefinitionTarget::CurrentTargetItem {
                            item_id,
                            location,
                        },
                    )
                })
            })
    }

    fn external_definition_from_binding(
        &self,
        binding: &ResolvedImportBinding,
    ) -> Option<DefinitionTarget> {
        let root_name = binding.source_root.as_ref()?;
        let normalized_path =
            normalized_path_for_root(root_name, &binding.target_path);

        if let Some(location) = self
            .external_lookup
            .definition_for_named_root_path(root_name, &normalized_path)
            .cloned()
        {
            return Some(DefinitionTarget::ExternalItem {
                root_name: root_name.clone(),
                path: normalized_path,
                location,
            });
        }

        if matches!(binding.kind, ImportBindingKind::Symbol(_))
            && normalized_path.len() == 1
            && let Some(location) = self
                .external_lookup
                .extern_function_definition(root_name, &normalized_path[0])
                .cloned()
        {
            return Some(DefinitionTarget::ExternalItem {
                root_name: root_name.clone(),
                path: normalized_path,
                location,
            });
        }

        None
    }

    fn external_definition_from_segments(
        &self,
        segments: &[String],
    ) -> Option<DefinitionTarget> {
        if segments.len() < 2 {
            return None;
        }
        let root_name = segments.first()?.clone();
        let remainder = segments[1..].to_vec();

        if let Some(location) = self
            .external_lookup
            .definition_for_named_root_path(&root_name, &remainder)
            .cloned()
        {
            return Some(DefinitionTarget::ExternalItem {
                root_name,
                path: remainder,
                location,
            });
        }

        if remainder.len() == 1
            && let Some(location) = self
                .external_lookup
                .extern_function_definition(&root_name, &remainder[0])
                .cloned()
        {
            return Some(DefinitionTarget::ExternalItem {
                root_name,
                path: remainder,
                location,
            });
        }

        None
    }
}

#[must_use]
pub fn lookup_definition_target(
    semantic: &SemanticAnalysis,
    imports: &BTreeMap<FileId, ResolvedImports>,
    external_lookup: &ExternalSemanticLookup,
    item_definitions: &BTreeMap<ItemId, DefinitionLocation>,
    file_id: FileId,
    offset: usize,
    fallback_word: Option<&str>,
) -> Option<DefinitionTarget> {
    SemanticDefinitionLookup::new(
        semantic,
        imports,
        external_lookup,
        item_definitions,
    )
    .lookup_definition_target(file_id, offset, fallback_word)
}

#[must_use]
pub fn completion_candidates_for_file(
    semantic: &SemanticAnalysis,
    imports: &BTreeMap<FileId, ResolvedImports>,
    file_id: FileId,
) -> Vec<SemanticCompletionCandidate> {
    let mut dedup = BTreeMap::new();

    for body in semantic.resolved_bodies.iter() {
        if body.containing_scope_file_id != file_id {
            continue;
        }
        let typed_body =
            semantic.typed_bodies.body(&body.owner, body.body_index);
        for local in &body.locals {
            let detail = typed_body
                .and_then(|typed| typed.local_types.get(&local.id))
                .map(|ty| {
                    format!(
                        "local: {}",
                        format_type_for_completion(ty, &semantic.global_items)
                    )
                })
                .unwrap_or_else(|| "local".to_string());
            dedup.entry(local.name.clone()).or_insert(
                SemanticCompletionCandidate {
                    label: local.name.clone(),
                    kind: SemanticCompletionKind::Local,
                    detail,
                },
            );
        }
    }

    if let Some(file_imports) = imports.get(&file_id) {
        for binding in file_imports.bindings.values() {
            let kind = match binding.kind {
                ImportBindingKind::Scope => SemanticCompletionKind::ImportScope,
                ImportBindingKind::Symbol(symbol_kind) => {
                    use crate::frontend::resolver::SymbolKind;
                    match symbol_kind {
                        SymbolKind::Function => {
                            SemanticCompletionKind::ImportFunction
                        }
                        SymbolKind::Struct => {
                            SemanticCompletionKind::ImportStruct
                        }
                        SymbolKind::Enum => SemanticCompletionKind::ImportEnum,
                        SymbolKind::Protocol => {
                            SemanticCompletionKind::ImportProtocol
                        }
                        SymbolKind::Scope => {
                            SemanticCompletionKind::ImportScope
                        }
                    }
                }
            };
            dedup.entry(binding.local_name.clone()).or_insert(
                SemanticCompletionCandidate {
                    label: binding.local_name.clone(),
                    kind,
                    detail: format!(
                        "import {}",
                        binding.target_path.join("::")
                    ),
                },
            );
        }
    }

    for item in semantic.global_items.items_in_scope(file_id) {
        let kind = match item.kind {
            crate::frontend::resolver::ItemKind::Scope => {
                SemanticCompletionKind::Scope
            }
            crate::frontend::resolver::ItemKind::Function => {
                SemanticCompletionKind::Function
            }
            crate::frontend::resolver::ItemKind::Struct => {
                SemanticCompletionKind::Struct
            }
            crate::frontend::resolver::ItemKind::Enum => {
                SemanticCompletionKind::Enum
            }
            crate::frontend::resolver::ItemKind::Protocol => {
                SemanticCompletionKind::Protocol
            }
        };
        dedup
            .entry(item.name.clone())
            .or_insert(SemanticCompletionCandidate {
                label: item.name.clone(),
                kind,
                detail: item.full_path.join("::"),
            });
    }

    dedup.into_values().collect()
}

#[must_use]
pub fn local_binding_type(
    semantic: &SemanticAnalysis,
    local_id: LocalId,
) -> Option<&Type> {
    semantic.resolved_bodies.iter().find_map(|body| {
        semantic
            .typed_bodies
            .body(&body.owner, body.body_index)
            .and_then(|typed| typed.local_types.get(&local_id))
    })
}

/// Collects local item declaration spans keyed by `ItemId`.
#[must_use]
pub fn collect_item_definition_locations(
    graph: &ScopeGraph,
    parsed_files: &[DesugaredFile],
    item_table: &GlobalItemTable,
) -> BTreeMap<ItemId, DefinitionLocation> {
    let parsed_by_id: BTreeMap<FileId, &DesugaredFile> = parsed_files
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect();
    let mut definitions = BTreeMap::new();

    for (scope_file_id, scope) in &graph.scopes {
        let Some(parsed) = parsed_by_id.get(scope_file_id) else {
            continue;
        };
        for item in &parsed.ast.items {
            let name = match &item.node {
                Item::Function(function_decl) => {
                    Some(function_decl.node.name.clone())
                }
                Item::Struct(struct_decl) => {
                    Some(struct_decl.node.name.clone())
                }
                Item::Enum(enum_decl) => Some(enum_decl.node.name.clone()),
                Item::Protocol(protocol_decl) => {
                    Some(protocol_decl.node.name.clone())
                }
                Item::Scope(scope_decl) => Some(scope_decl.node.name.clone()),
                Item::Use(_)
                | Item::Impl(_)
                | Item::ExternBlock(_)
                | Item::Macro(_) => None,
            };
            let Some(name) = name else {
                continue;
            };
            let mut full_path = scope.scope_path.clone();
            full_path.push(name);
            if let Some(item_id) = item_table.item_id_by_full_path(&full_path) {
                definitions.entry(item_id).or_insert(DefinitionLocation {
                    file_id: parsed.file_id,
                    span: item.span,
                });
            }
        }
    }

    definitions
}

fn span_contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

fn normalized_path_for_root(root_name: &str, path: &[String]) -> Vec<String> {
    if path.first().is_some_and(|segment| segment == root_name) {
        path[1..].to_vec()
    } else {
        path.to_vec()
    }
}

fn format_type_for_completion(
    ty: &Type,
    item_table: &GlobalItemTable,
) -> String {
    match ty {
        Type::Builtin(builtin) => builtin.to_string(),
        Type::Named { item_id, .. } => item_table
            .get(*item_id)
            .map(|item| item.full_path.join("::"))
            .unwrap_or_else(|| format!("item#{}", item_id.raw())),
        Type::Pointer {
            pointee,
            mutability,
        } => match mutability {
            crate::frontend::Mutability::Const => {
                format!("*{}", format_type_for_completion(pointee, item_table))
            }
            crate::frontend::Mutability::Mut => {
                format!(
                    "*mut {}",
                    format_type_for_completion(pointee, item_table)
                )
            }
        },
        Type::Error => "<error>".to_string(),
    }
}
