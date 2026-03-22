use super::analysis::{
    analyze_semantics_with_external_lookup, resolve_hir_semantic_input,
};
use super::types::Type;
use crate::frontend::DesugaredFile;
use crate::frontend::ast::{ExternMember, Item, ParamLabel, Span};
use crate::frontend::resolver::{
    NamedImportRoot, ScopeGraph,
    resolve_project_imports_with_named_roots_and_diagnostics,
};
use crate::frontend::source::{FileId, SourceDb};
use crate::midend::type_check::signatures::{
    TypedFunctionSignature, TypedParamLabel,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Lookup-only semantic context for call/type queries outside the current
/// target-local semantic tables.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExternalSemanticLookup {
    named_roots: BTreeMap<String, ExternalNamedRoot>,
    extern_libraries: BTreeMap<String, ExternalExternLibrary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ExternalNamedRoot {
    functions_by_path: BTreeMap<Vec<String>, TypedFunctionSignature>,
    definitions_by_path: BTreeMap<Vec<String>, ExternalDefinitionLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ExternalExternLibrary {
    functions_by_name: BTreeMap<String, TypedFunctionSignature>,
    definitions_by_name: BTreeMap<String, ExternalDefinitionLocation>,
}

/// Definition location for an item owned by an external semantic root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalDefinitionLocation {
    pub file_path: PathBuf,
    pub span: Span,
}

impl ExternalSemanticLookup {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_named_root_function(
        &mut self,
        root_name: String,
        path: Vec<String>,
        signature: TypedFunctionSignature,
    ) {
        self.insert_named_root_function_with_definition(
            root_name, path, signature, None,
        );
    }

    pub fn insert_named_root_function_with_definition(
        &mut self,
        root_name: String,
        path: Vec<String>,
        signature: TypedFunctionSignature,
        definition: Option<ExternalDefinitionLocation>,
    ) {
        let named_root = self.named_roots.entry(root_name).or_default();
        if let Some(definition) = definition {
            named_root
                .definitions_by_path
                .insert(path.clone(), definition);
        }
        named_root.functions_by_path.insert(path, signature);
    }

    pub fn insert_named_root_definition(
        &mut self,
        root_name: String,
        path: Vec<String>,
        location: ExternalDefinitionLocation,
    ) {
        self.named_roots
            .entry(root_name)
            .or_default()
            .definitions_by_path
            .insert(path, location);
    }

    #[must_use]
    pub fn function_for_named_root_path(
        &self,
        root_name: &str,
        path: &[String],
    ) -> Option<&TypedFunctionSignature> {
        self.named_roots.get(root_name)?.functions_by_path.get(path)
    }

    #[must_use]
    pub fn definition_for_named_root_path(
        &self,
        root_name: &str,
        path: &[String],
    ) -> Option<&ExternalDefinitionLocation> {
        self.named_roots
            .get(root_name)?
            .definitions_by_path
            .get(path)
    }

    pub fn insert_extern_function(
        &mut self,
        library_name: String,
        function_name: String,
        signature: TypedFunctionSignature,
    ) {
        self.insert_extern_function_with_definition(
            library_name,
            function_name,
            signature,
            None,
        );
    }

    pub fn insert_extern_function_with_definition(
        &mut self,
        library_name: String,
        function_name: String,
        signature: TypedFunctionSignature,
        definition: Option<ExternalDefinitionLocation>,
    ) {
        let extern_library =
            self.extern_libraries.entry(library_name).or_default();
        if let Some(definition) = definition {
            extern_library
                .definitions_by_name
                .insert(function_name.clone(), definition);
        }
        extern_library
            .functions_by_name
            .insert(function_name, signature);
    }

    pub fn insert_extern_definition(
        &mut self,
        library_name: String,
        function_name: String,
        location: ExternalDefinitionLocation,
    ) {
        self.extern_libraries
            .entry(library_name)
            .or_default()
            .definitions_by_name
            .insert(function_name, location);
    }

    #[must_use]
    pub fn extern_function_signature(
        &self,
        library_name: &str,
        function_name: &str,
    ) -> Option<&TypedFunctionSignature> {
        self.extern_libraries
            .get(library_name)?
            .functions_by_name
            .get(function_name)
    }

    #[must_use]
    pub fn extern_function_definition(
        &self,
        library_name: &str,
        function_name: &str,
    ) -> Option<&ExternalDefinitionLocation> {
        self.extern_libraries
            .get(library_name)?
            .definitions_by_name
            .get(function_name)
    }

    #[must_use]
    pub fn is_extern_library(&self, library_name: &str) -> bool {
        self.extern_libraries.contains_key(library_name)
    }

    #[must_use]
    pub fn extern_namespace_for_function(
        &self,
        function_name: &str,
    ) -> Option<&str> {
        self.extern_libraries
            .iter()
            .find_map(|(library, functions)| {
                functions
                    .functions_by_name
                    .contains_key(function_name)
                    .then_some(library.as_str())
            })
    }
}

/// Builds lookup-only semantic context for cross-target/project semantic
/// queries without merging foreign items into current-target tables.
#[must_use]
pub fn build_external_semantic_lookup(
    db: &SourceDb,
    named_roots: &BTreeMap<String, NamedImportRoot>,
    graph: &ScopeGraph,
    parsed_files: &[DesugaredFile],
) -> ExternalSemanticLookup {
    let mut lookup = ExternalSemanticLookup::new();

    for (root_name, root) in named_roots {
        let NamedImportRoot::LoadedLibrary {
            graph,
            parsed_files,
            path_by_file_id,
        } = root
        else {
            continue;
        };

        let empty_named_roots = BTreeMap::new();
        let (_, imports, _) =
            resolve_project_imports_with_named_roots_and_diagnostics(
                graph,
                parsed_files,
                &empty_named_roots,
                db,
            );
        let semantic = analyze_semantics_with_external_lookup(
            db,
            resolve_hir_semantic_input(graph, parsed_files, &imports),
            &ExternalSemanticLookup::new(),
        );
        let definitions = collect_item_definition_locations(
            graph,
            parsed_files,
            &semantic.global_items,
        );

        for item in semantic.global_items.iter() {
            if let Some(location) = definition_location_from_file_id(
                path_by_file_id,
                definitions.get(&item.id).copied(),
            ) {
                lookup.insert_named_root_definition(
                    root_name.clone(),
                    item.full_path.clone(),
                    location.clone(),
                );
                if let Some(signature) = semantic.typed_items.function(item.id)
                {
                    lookup.insert_named_root_function_with_definition(
                        root_name.clone(),
                        item.full_path.clone(),
                        signature.clone(),
                        Some(location),
                    );
                }
            } else if let Some(signature) =
                semantic.typed_items.function(item.id)
            {
                lookup.insert_named_root_function(
                    root_name.clone(),
                    item.full_path.clone(),
                    signature.clone(),
                );
            }
        }
    }

    let parsed_by_id: BTreeMap<FileId, &DesugaredFile> = parsed_files
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect();
    for scope_file_id in graph.scopes.keys() {
        let Some(parsed) = parsed_by_id.get(scope_file_id) else {
            continue;
        };
        let Some(source_file) = db.file(parsed.file_id) else {
            continue;
        };
        let file_path = source_file.path().to_path_buf();
        for item in &parsed.ast.items {
            let Item::ExternBlock(extern_block) = &item.node else {
                continue;
            };
            let library_name = extern_block.node.library_name.clone();
            for member in &extern_block.node.members {
                let ExternMember::Function(function) = &member.node;
                let location = ExternalDefinitionLocation {
                    file_path: file_path.clone(),
                    span: member.span,
                };
                lookup.insert_extern_definition(
                    library_name.clone(),
                    function.node.local_name.clone(),
                    location.clone(),
                );
                lookup.insert_extern_function_with_definition(
                    library_name.clone(),
                    function.node.local_name.clone(),
                    extern_function_signature(&function.node),
                    Some(location),
                );
            }
        }
    }

    lookup
}

fn definition_location_from_file_id(
    path_by_file_id: &BTreeMap<FileId, PathBuf>,
    location: Option<(FileId, Span)>,
) -> Option<ExternalDefinitionLocation> {
    let (file_id, span) = location?;
    let file_path = path_by_file_id.get(&file_id)?;
    Some(ExternalDefinitionLocation {
        file_path: file_path.clone(),
        span,
    })
}

fn collect_item_definition_locations(
    graph: &ScopeGraph,
    parsed_files: &[DesugaredFile],
    item_table: &crate::frontend::resolver::GlobalItemTable,
) -> BTreeMap<crate::frontend::resolver::ItemId, (FileId, Span)> {
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
                Item::ExternBlock(_) => None,
                Item::Use(_) => None,
                Item::Impl(_) => None,
                Item::Macro(_) => None,
            };
            let Some(name) = name else {
                continue;
            };
            let mut full_path = scope.scope_path.clone();
            full_path.push(name);
            if let Some(item_id) = item_table.item_id_by_full_path(&full_path) {
                definitions
                    .entry(item_id)
                    .or_insert((parsed.file_id, item.span));
            }
        }
    }

    definitions
}

fn extern_function_signature(
    decl: &crate::frontend::ast::ExternFunctionDecl,
) -> TypedFunctionSignature {
    TypedFunctionSignature {
        param_labels: decl
            .params
            .iter()
            .map(|param| match &param.node.label {
                ParamLabel::None => TypedParamLabel::None,
                ParamLabel::Explicit(label) => {
                    TypedParamLabel::Explicit(label.clone())
                }
                ParamLabel::FromName => TypedParamLabel::FromName,
            })
            .collect(),
        param_types: vec![Type::error(); decl.params.len()],
        return_type: decl.return_type.as_ref().map(|_| Type::error()),
    }
}
