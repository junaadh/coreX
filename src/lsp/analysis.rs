use crate::cli_driver::project::{
    collect_project_cx_files, resolve_target_scope_graph_with_diagnostics,
};
use crate::lsp::convert::{
    LspPosition, LspRange, offset_to_position, path_to_uri, position_to_offset,
    span_to_lsp_range, word_span_at_position,
};
use crate::lsp::state::ServerState;
use core_x::frontend::ast::Item;
use core_x::frontend::parser::parse_source_file_from_source_file_with_recovery;
use core_x::frontend::resolver::{
    ItemId, NamedImportRoot, ResolvedScopeKind, ScopeResolver,
    resolve_project_imports_with_named_roots_and_diagnostics,
    resolve_project_scopes,
};
use core_x::frontend::source::{FileId, SourceDb, SourceFile};
use core_x::frontend::{
    DefinitionLocation, DefinitionTarget, Diagnostic, DiagnosticsBag,
    ExpansionOptions, ExpandedFile, ExternalSemanticLookup, GlobalItem, GlobalItemTable,
    ImportRootKind, ProjectLoader, SemanticAnalysis,
    SemanticCompletionKind, analyze_semantics_with_external_lookup,
    build_external_semantic_lookup, build_target_roots,
    collect_item_definition_locations, completion_candidates_for_file,
    expand_parsed_files, load_local_dependency_project_graph,
    local_binding_type, lookup_definition_target,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub struct DocumentAnalysis {
    pub uri: String,
    pub db: SourceDb,
    pub parsed_files: Vec<ExpandedFile>,
    pub primary_file_id: FileId,
    pub diagnostics: DiagnosticsBag,
    pub imports: BTreeMap<FileId, core_x::frontend::ResolvedImports>,
    pub semantic: Option<SemanticAnalysis>,
    external_lookup: ExternalSemanticLookup,
    path_by_file_id: BTreeMap<FileId, PathBuf>,
    file_id_by_path: BTreeMap<PathBuf, FileId>,
    item_definitions: BTreeMap<ItemId, DefinitionLocation>,
}

pub fn analyze_document_cached(
    state: &mut ServerState,
    uri: &str,
) -> Result<Arc<DocumentAnalysis>, String> {
    let version = state.document(uri).and_then(|document| document.version);
    if let Some(cached) = state.cached_analysis(uri, version) {
        return Ok(cached);
    }

    let analysis = Arc::new(analyze_document_uncached(state, uri)?);
    state.store_cached_analysis(uri, version, Arc::clone(&analysis));
    Ok(analysis)
}

fn analyze_document_uncached(
    state: &ServerState,
    uri: &str,
) -> Result<DocumentAnalysis, String> {
    let Some(document) = state.document(uri) else {
        return Err(format!("document is not open: {uri}"));
    };
    let open_text_by_path = state.open_text_by_path();

    if let Some(project_root) = find_project_root(&document.path)
        && let Ok(project_analysis) = analyze_in_project(
            uri.to_string(),
            document.path.clone(),
            &document.text,
            &open_text_by_path,
            &project_root,
        )
    {
        return Ok(project_analysis);
    }

    analyze_standalone(
        uri.to_string(),
        document.path.clone(),
        document.text.clone(),
    )
}

pub fn diagnostics_for_document(analysis: &DocumentAnalysis) -> Vec<Value> {
    let Some(primary_file) = analysis.db.file(analysis.primary_file_id) else {
        return Vec::new();
    };

    analysis
        .diagnostics
        .as_slice()
        .iter()
        .filter_map(|diagnostic| {
            diagnostic_to_lsp(
                diagnostic,
                analysis.primary_file_id,
                primary_file,
            )
        })
        .collect()
}

pub fn document_symbols_for_document(
    analysis: &DocumentAnalysis,
) -> Vec<Value> {
    let parsed = analysis
        .parsed_files
        .iter()
        .find(|parsed| parsed.file_id == analysis.primary_file_id);
    let Some(parsed) = parsed else {
        return Vec::new();
    };
    let Some(file) = analysis.db.file(analysis.primary_file_id) else {
        return Vec::new();
    };

    parsed
        .ast
        .items
        .iter()
        .filter_map(|item| match &item.node {
            Item::Function(function_decl) => Some(json!({
                "name": function_decl.node.name,
                "kind": 12,
                "range": span_to_lsp_range(file, item.span),
                "selectionRange": span_to_lsp_range(file, item.span),
                "detail": "function",
            })),
            Item::Struct(struct_decl) => Some(json!({
                "name": struct_decl.node.name,
                "kind": 23,
                "range": span_to_lsp_range(file, item.span),
                "selectionRange": span_to_lsp_range(file, item.span),
                "detail": "struct",
            })),
            Item::Enum(enum_decl) => Some(json!({
                "name": enum_decl.node.name,
                "kind": 10,
                "range": span_to_lsp_range(file, item.span),
                "selectionRange": span_to_lsp_range(file, item.span),
                "detail": "enum",
            })),
            Item::Protocol(protocol_decl) => Some(json!({
                "name": protocol_decl.node.name,
                "kind": 11,
                "range": span_to_lsp_range(file, item.span),
                "selectionRange": span_to_lsp_range(file, item.span),
                "detail": "protocol",
            })),
            Item::Scope(scope_decl) => Some(json!({
                "name": scope_decl.node.name,
                "kind": 3,
                "range": span_to_lsp_range(file, item.span),
                "selectionRange": span_to_lsp_range(file, item.span),
                "detail": "scope",
            })),
            _ => None,
        })
        .collect()
}

pub fn hover_for_position(
    analysis: &DocumentAnalysis,
    position: LspPosition,
) -> Option<Value> {
    let file = analysis.db.file(analysis.primary_file_id)?;
    let offset = position_to_offset(file, position)?;
    let (word, span) = word_span_at_position(file, position)?;

    if let Some(semantic) = &analysis.semantic {
        if let Some(target) = lookup_definition_target(
            semantic,
            &analysis.imports,
            &analysis.external_lookup,
            &analysis.item_definitions,
            analysis.primary_file_id,
            offset,
            Some(&word),
        ) {
            let hover_text = match target {
                DefinitionTarget::LocalBinding { local_id, .. } => {
                    let local_type = local_binding_type(semantic, local_id)?;
                    format!(
                        "local `{word}`: {}",
                        format_type(local_type, &semantic.global_items)
                    )
                }
                DefinitionTarget::CurrentTargetItem { item_id, .. } => {
                    let global_item = semantic.global_items.get(item_id)?;
                    hover_text_for_item(global_item, semantic)
                }
                DefinitionTarget::ExternalItem {
                    root_name, path, ..
                } => {
                    if let Some(signature) = analysis
                        .external_lookup
                        .function_for_named_root_path(&root_name, &path)
                    {
                        hover_text_for_external_function(
                            &root_name, &path, signature, semantic,
                        )
                    } else if path.len() == 1 {
                        if let Some(signature) = analysis
                            .external_lookup
                            .extern_function_signature(&root_name, &path[0])
                        {
                            hover_text_for_external_function(
                                &root_name, &path, signature, semantic,
                            )
                        } else {
                            format!(
                                "external {}",
                                [root_name, path.join("::")].join("::")
                            )
                        }
                    } else {
                        format!(
                            "external {}",
                            [root_name, path.join("::")].join("::")
                        )
                    }
                }
            };

            return Some(json!({
                "contents": {
                    "kind": "plaintext",
                    "value": hover_text,
                },
                "range": span_to_lsp_range(file, span),
            }));
        }
    }

    None
}

pub fn definition_for_position(
    analysis: &DocumentAnalysis,
    position: LspPosition,
) -> Vec<Value> {
    let Some(file) = analysis.db.file(analysis.primary_file_id) else {
        return Vec::new();
    };
    let Some(offset) = position_to_offset(file, position) else {
        return Vec::new();
    };
    let fallback_word =
        word_span_at_position(file, position).map(|(word, _)| word);
    let Some(semantic) = &analysis.semantic else {
        return Vec::new();
    };

    if let Some(target) = lookup_definition_target(
        semantic,
        &analysis.imports,
        &analysis.external_lookup,
        &analysis.item_definitions,
        analysis.primary_file_id,
        offset,
        fallback_word.as_deref(),
    ) && let Some(location) =
        location_for_definition_target(analysis, &target)
    {
        return vec![location];
    }

    Vec::new()
}

pub fn completion_for_position(
    analysis: &DocumentAnalysis,
    position: LspPosition,
) -> Vec<Value> {
    let Some(file) = analysis.db.file(analysis.primary_file_id) else {
        return Vec::new();
    };
    let prefix = word_span_at_position(file, position)
        .map(|(word, _)| word)
        .unwrap_or_default();
    let mut items = BTreeMap::new();

    for keyword in [
        "fn", "struct", "enum", "protocol", "scope", "use", "let", "var", "if",
        "else", "while", "for", "return", "async", "unsafe", "await", "root",
        "super",
    ] {
        insert_completion_item(
            &mut items,
            keyword.to_string(),
            14,
            "keyword".to_string(),
        );
    }

    if let Some(semantic) = &analysis.semantic {
        for candidate in completion_candidates_for_file(
            semantic,
            &analysis.imports,
            analysis.primary_file_id,
        ) {
            insert_completion_item(
                &mut items,
                candidate.label,
                completion_kind_for_semantic_candidate(candidate.kind),
                candidate.detail,
            );
        }
    }

    items
        .into_values()
        .filter(|entry| {
            if prefix.is_empty() {
                return true;
            }
            entry
                .get("label")
                .and_then(Value::as_str)
                .is_some_and(|label| label.starts_with(&prefix))
        })
        .collect()
}

pub fn inlay_hints_for_range(
    analysis: &DocumentAnalysis,
    range: LspRange,
) -> Vec<Value> {
    let Some(file) = analysis.db.file(analysis.primary_file_id) else {
        return Vec::new();
    };
    let Some(start_offset) = position_to_offset(file, range.start) else {
        return Vec::new();
    };
    let Some(end_offset) = position_to_offset(file, range.end) else {
        return Vec::new();
    };
    let Some(semantic) = &analysis.semantic else {
        return Vec::new();
    };

    let mut seen = BTreeSet::new();
    let mut hints = Vec::new();
    for body in semantic.resolved_bodies.iter() {
        if body.containing_scope_file_id != analysis.primary_file_id {
            continue;
        }
        let Some(typed_body) =
            semantic.typed_bodies.body(&body.owner, body.body_index)
        else {
            continue;
        };

        for local in &body.locals {
            if local.declared_type.is_some() {
                continue;
            }
            if !matches!(
                local.kind,
                core_x::frontend::resolver::LocalKind::LocalBinding
                    | core_x::frontend::resolver::LocalKind::PatternBinding
            ) {
                continue;
            }
            let Some(ty) = typed_body.local_types.get(&local.id) else {
                continue;
            };
            if ty.is_error() {
                continue;
            }
            let anchor = local.declared_span.end.min(file.len());
            if anchor < start_offset || anchor > end_offset {
                continue;
            }
            if !seen.insert((local.id.raw(), anchor)) {
                continue;
            }
            hints.push(json!({
                "position": offset_to_position(file, anchor),
                "label": format!(": {}", format_type(ty, &semantic.global_items)),
                "kind": 1,
            }));
        }
    }

    hints.sort_by(|lhs, rhs| {
        let lhs_line = lhs
            .get("position")
            .and_then(|pos| pos.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let rhs_line = rhs
            .get("position")
            .and_then(|pos| pos.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let lhs_col = lhs
            .get("position")
            .and_then(|pos| pos.get("character"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let rhs_col = rhs
            .get("position")
            .and_then(|pos| pos.get("character"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        lhs_line.cmp(&rhs_line).then(lhs_col.cmp(&rhs_col))
    });
    hints
}

fn analyze_standalone(
    uri: String,
    path: PathBuf,
    text: String,
) -> Result<DocumentAnalysis, String> {
    let mut db = SourceDb::new();
    let file_id = db.add_file(path.clone(), text);
    let Some(file) = db.file(file_id) else {
        return Err(format!("missing source file id {}", file_id.raw()));
    };
    let parsed = parse_source_file_from_source_file_with_recovery(file)
        .map_err(|error| format!("failed to initialize parser: {error}"))?;
    let parsed_files =
        expand_parsed_files(&db, &[parsed], ExpansionOptions::default());

    let mut diagnostics = DiagnosticsBag::new();
    for parsed in &parsed_files {
        diagnostics.extend(parsed.diagnostics.as_slice().iter().cloned());
    }

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let (graph, scope_diagnostics) =
        resolver.resolve_library_root_with_diagnostics(file_id, &db);
    diagnostics.extend(scope_diagnostics.as_slice().iter().cloned());

    let mut imports = BTreeMap::new();
    let mut semantic = None;
    let mut external_lookup = ExternalSemanticLookup::new();
    let mut item_definitions = BTreeMap::new();
    if let Some(graph) = &graph {
        let (symbols, resolved_imports, import_diagnostics) =
            resolve_project_imports_with_named_roots_and_diagnostics(
                graph,
                &parsed_files,
                &BTreeMap::new(),
                &db,
            );
        let _ = symbols;
        diagnostics.extend(import_diagnostics.as_slice().iter().cloned());
        imports = resolved_imports;

        external_lookup = build_external_semantic_lookup(
            &db,
            &BTreeMap::new(),
            graph,
            &parsed_files,
        );
        let semantic_result = analyze_semantics_with_external_lookup(
            &db,
            graph,
            &parsed_files,
            &imports,
            &external_lookup,
        );
        diagnostics
            .extend(semantic_result.diagnostics.as_slice().iter().cloned());
        item_definitions = collect_item_definition_locations(
            graph,
            &parsed_files,
            &semantic_result.global_items,
        );
        semantic = Some(semantic_result);
    }

    let mut path_by_file_id = BTreeMap::new();
    let mut file_id_by_path = BTreeMap::new();
    path_by_file_id.insert(file_id, path.clone());
    file_id_by_path.insert(path, file_id);

    Ok(DocumentAnalysis {
        uri,
        db,
        parsed_files,
        primary_file_id: file_id,
        diagnostics,
        imports,
        semantic,
        external_lookup,
        path_by_file_id,
        file_id_by_path,
        item_definitions,
    })
}

fn analyze_in_project(
    uri: String,
    path: PathBuf,
    open_text: &str,
    open_text_by_path: &BTreeMap<PathBuf, String>,
    project_root: &Path,
) -> Result<DocumentAnalysis, String> {
    let loaded_project = ProjectLoader::load_project(project_root)
        .map_err(|error| format!("failed to load project: {error}"))?;
    let project_graph =
        load_local_dependency_project_graph(loaded_project.clone()).map_err(
            |error| format!("failed to load local dependency graph: {error}"),
        )?;
    let target_roots = build_target_roots(&project_graph)
        .map_err(|error| format!("failed to build target roots: {error}"))?;
    let manifest = loaded_project.manifest.clone();
    let files = collect_project_cx_files(&manifest)
        .map_err(|error| format!("failed to collect project files: {error}"))?;

    let path = normalize_path(&path);
    if !files
        .iter()
        .any(|candidate| normalize_path(candidate) == path)
    {
        return analyze_standalone(uri, path, open_text.to_string());
    }

    let mut db = SourceDb::new();
    let mut parsed = Vec::with_capacity(files.len());
    let mut path_by_file_id = BTreeMap::new();
    let mut file_id_by_path = BTreeMap::new();

    for file_path in files {
        let normalized = normalize_path(&file_path);
        let source = if normalized == path {
            open_text.to_string()
        } else if let Some(text) = open_text_by_path.get(&normalized) {
            text.clone()
        } else {
            fs::read_to_string(&normalized).map_err(|error| {
                format!("failed reading {}: {error}", normalized.display())
            })?
        };
        let file_id = db.add_file(normalized.clone(), source);
        let Some(file) = db.file(file_id) else {
            return Err(format!("missing source file id {}", file_id.raw()));
        };
        let parsed_file = parse_source_file_from_source_file_with_recovery(file)
            .map_err(|error| format!("failed to initialize parser: {error}"))?;
        parsed.push(parsed_file);
        path_by_file_id.insert(file_id, normalized.clone());
        file_id_by_path.insert(normalized, file_id);
    }

    let expanded_files =
        expand_parsed_files(&db, &parsed, ExpansionOptions::default());

    let Some(primary_file_id) = file_id_by_path.get(&path).copied() else {
        return analyze_standalone(uri, path, open_text.to_string());
    };

    let mut diagnostics = DiagnosticsBag::new();
    for expanded in &expanded_files {
        diagnostics.extend(expanded.diagnostics.as_slice().iter().cloned());
    }

    let (root_kind, root_file_id) =
        select_target_for_file(&manifest, &path, &file_id_by_path)?;

    let scope_resolver = ScopeResolver::new(&db, &expanded_files);
    let (graph, scope_diagnostics) =
        resolve_target_scope_graph_with_diagnostics(
            &scope_resolver,
            &db,
            &expanded_files,
            root_file_id,
            root_kind,
        );
    diagnostics.extend(scope_diagnostics.as_slice().iter().cloned());

    let mut imports = BTreeMap::new();
    let mut semantic = None;
    let mut external_lookup = ExternalSemanticLookup::new();
    let mut item_definitions = BTreeMap::new();
    if let Some(graph) = &graph {
        let named_roots = build_named_roots_for_project_analysis(
            root_kind,
            &scope_resolver,
            &db,
            &expanded_files,
            &file_id_by_path,
            &project_graph,
            &target_roots,
            &mut diagnostics,
        )?;

        let (symbols, resolved_imports, import_diagnostics) =
            resolve_project_imports_with_named_roots_and_diagnostics(
                graph,
                &expanded_files,
                &named_roots,
                &db,
            );
        let _ = symbols;
        diagnostics.extend(import_diagnostics.as_slice().iter().cloned());
        imports = resolved_imports;

        external_lookup = build_external_semantic_lookup(
            &db,
            &named_roots,
            graph,
            &expanded_files,
        );
        let semantic_result = analyze_semantics_with_external_lookup(
            &db,
            graph,
            &expanded_files,
            &imports,
            &external_lookup,
        );
        diagnostics
            .extend(semantic_result.diagnostics.as_slice().iter().cloned());
        item_definitions = collect_item_definition_locations(
            graph,
            &expanded_files,
            &semantic_result.global_items,
        );
        semantic = Some(semantic_result);
    }

    Ok(DocumentAnalysis {
        uri,
        db,
        parsed_files: expanded_files,
        primary_file_id,
        diagnostics,
        imports,
        semantic,
        external_lookup,
        path_by_file_id,
        file_id_by_path,
        item_definitions,
    })
}

fn select_target_for_file(
    manifest: &core_x::frontend::ProjectManifest,
    path: &Path,
    file_id_by_path: &BTreeMap<PathBuf, FileId>,
) -> Result<(ResolvedScopeKind, FileId), String> {
    for binary in &manifest.binaries {
        let root = normalize_path(&binary.root_file);
        if root == path {
            let Some(file_id) = file_id_by_path.get(&root).copied() else {
                return Err(format!(
                    "missing parsed file for {}",
                    root.display()
                ));
            };
            return Ok((ResolvedScopeKind::BinaryRoot, file_id));
        }
    }

    if let Some(library) = &manifest.library {
        let root = normalize_path(&library.root_file);
        let Some(file_id) = file_id_by_path.get(&root).copied() else {
            return Err(format!("missing parsed file for {}", root.display()));
        };
        return Ok((ResolvedScopeKind::Root, file_id));
    }

    if let Some(binary) = manifest.binaries.first() {
        let root = normalize_path(&binary.root_file);
        let Some(file_id) = file_id_by_path.get(&root).copied() else {
            return Err(format!("missing parsed file for {}", root.display()));
        };
        return Ok((ResolvedScopeKind::BinaryRoot, file_id));
    }

    Err("project manifest has no compilation targets".to_string())
}

fn find_project_root(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent()?.to_path_buf();
    loop {
        if current.join("corex.toml").is_file()
            && ProjectLoader::load_project(&current).is_ok()
        {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn normalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn build_named_roots_for_project_analysis(
    root_kind: ResolvedScopeKind,
    scope_resolver: &ScopeResolver<'_>,
    db: &SourceDb,
    parsed_files: &[ExpandedFile],
    file_id_by_path: &BTreeMap<PathBuf, FileId>,
    project_graph: &core_x::frontend::ProjectGraph,
    target_roots: &core_x::frontend::TargetRoots,
    diagnostics: &mut DiagnosticsBag,
) -> Result<BTreeMap<String, NamedImportRoot>, String> {
    let mut named_roots = BTreeMap::new();

    if root_kind == ResolvedScopeKind::BinaryRoot
        && let Some(library_target) =
            &project_graph.root_project.manifest.library
        && let Some(library_root_id) =
            file_id_by_path.get(&normalize_path(&library_target.root_file))
    {
        let (library_graph, library_diagnostics) = scope_resolver
            .resolve_library_root_with_diagnostics(*library_root_id, db);
        diagnostics.extend(library_diagnostics.as_slice().iter().cloned());
        if let Some(library_graph) = library_graph {
            named_roots.insert(
                library_target.name.clone(),
                NamedImportRoot::LoadedLibrary {
                    graph: library_graph,
                    parsed_files: parsed_files.to_vec(),
                    path_by_file_id: file_id_by_path
                        .iter()
                        .map(|(path, file_id)| (*file_id, path.clone()))
                        .collect(),
                },
            );
        }
    }

    for (name, root) in &target_roots.by_name {
        match root.kind {
            ImportRootKind::CurrentLibrary => {}
            ImportRootKind::UnloadedGitDependency => {
                named_roots
                    .insert(name.clone(), NamedImportRoot::UnloadedDependency);
            }
            ImportRootKind::LocalDependencyLibrary => {
                let dependency = project_graph
                    .local_dependencies
                    .iter()
                    .find(|dependency| dependency.dependency_name == *name)
                    .ok_or_else(|| {
                        format!(
                            "missing loaded local dependency project for root `{name}`"
                        )
                    })?;
                let library_target =
                    dependency.project.manifest.library.as_ref().ok_or_else(
                        || {
                            format!(
                                "dependency `{}` has no library target",
                                dependency.dependency_name
                            )
                        },
                    )?;
                let (dep_db, dep_parsed_files, dep_file_id_by_path) =
                    parse_loaded_project_files(&dependency.project)?;
                let library_root_file_id = dep_file_id_by_path
                    .get(&library_target.root_file)
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "missing dependency library root file {}",
                            library_target.root_file.display()
                        )
                    })?;
                let graph = resolve_project_scopes(
                    &dep_db,
                    &dep_parsed_files,
                    library_root_file_id,
                    ResolvedScopeKind::Root,
                )
                .map_err(|error| {
                    format!(
                        "failed to resolve dependency root `{}`: {error}",
                        dependency.dependency_name
                    )
                })?;
                named_roots.insert(
                    name.clone(),
                    NamedImportRoot::LoadedLibrary {
                        graph,
                        parsed_files: dep_parsed_files,
                        path_by_file_id: dep_file_id_by_path
                            .iter()
                            .map(|(path, file_id)| (*file_id, path.clone()))
                            .collect(),
                    },
                );
            }
        }
    }

    Ok(named_roots)
}

fn parse_loaded_project_files(
    project: &core_x::frontend::LoadedProject,
) -> Result<(SourceDb, Vec<ExpandedFile>, BTreeMap<PathBuf, FileId>), String> {
    let project_files =
        collect_project_cx_files(&project.manifest).map_err(|error| {
            format!("failed collecting dependency files: {error}")
        })?;
    let mut db = SourceDb::new();
    let mut parsed = Vec::with_capacity(project_files.len());
    let mut file_id_by_path = BTreeMap::new();

    for absolute_path in project_files {
        let source = fs::read_to_string(&absolute_path).map_err(|error| {
            format!(
                "failed reading dependency file {}: {error}",
                absolute_path.display()
            )
        })?;
        let file_id = db.add_file(absolute_path.clone(), source);
        let Some(file) = db.file(file_id) else {
            return Err(format!(
                "missing dependency source file id {}",
                file_id.raw()
            ));
        };
        let parsed_file = parse_source_file_from_source_file_with_recovery(file)
            .map_err(|error| {
                format!(
                    "failed to initialize parser for dependency file {}: {error}",
                    absolute_path.display()
                )
            })?;
        parsed.push(parsed_file);
        file_id_by_path.insert(absolute_path, file_id);
    }

    let expanded_files =
        expand_parsed_files(&db, &parsed, ExpansionOptions::default());

    Ok((db, expanded_files, file_id_by_path))
}

fn diagnostic_to_lsp(
    diagnostic: &Diagnostic,
    primary_file_id: FileId,
    primary_file: &SourceFile,
) -> Option<Value> {
    let label = diagnostic
        .labels
        .iter()
        .find(|label| label.span.file_id == primary_file_id);
    let range = label.map_or(
        LspRange {
            start: LspPosition {
                line: 0,
                character: 0,
            },
            end: LspPosition {
                line: 0,
                character: 0,
            },
        },
        |label| span_to_lsp_range(primary_file, label.span.span),
    );

    let severity = match diagnostic.severity {
        core_x::frontend::DiagnosticSeverity::Error => 1,
        core_x::frontend::DiagnosticSeverity::Warning => 2,
        core_x::frontend::DiagnosticSeverity::Note => 3,
        core_x::frontend::DiagnosticSeverity::Help => 4,
    };
    Some(json!({
        "range": range,
        "severity": severity,
        "source": "corex",
        "message": diagnostic.message,
    }))
}

fn location_for_definition_target(
    analysis: &DocumentAnalysis,
    target: &DefinitionTarget,
) -> Option<Value> {
    match target {
        DefinitionTarget::LocalBinding { location, .. }
        | DefinitionTarget::CurrentTargetItem { location, .. } => {
            location_for_file_and_span(
                analysis,
                location.file_id,
                location.span,
            )
        }
        DefinitionTarget::ExternalItem { location, .. } => {
            let normalized_path = normalize_path(&location.file_path);
            if let Some(file_id) =
                analysis.file_id_by_path.get(&normalized_path)
                && let Some(file) = analysis.db.file(*file_id)
            {
                return Some(json!({
                    "uri": path_to_uri(&normalized_path),
                    "range": span_to_lsp_range(file, location.span),
                }));
            }

            Some(json!({
                "uri": path_to_uri(&normalized_path),
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 0},
                },
            }))
        }
    }
}

fn location_for_file_and_span(
    analysis: &DocumentAnalysis,
    file_id: FileId,
    span: core_x::frontend::ast::Span,
) -> Option<Value> {
    let file_path = analysis.path_by_file_id.get(&file_id)?;
    let file = analysis.db.file(file_id)?;
    Some(json!({
        "uri": path_to_uri(file_path),
        "range": span_to_lsp_range(file, span),
    }))
}

fn hover_text_for_item(
    item: &GlobalItem,
    semantic: &SemanticAnalysis,
) -> String {
    let kind = match item.kind {
        core_x::frontend::resolver::ItemKind::Scope => "scope",
        core_x::frontend::resolver::ItemKind::Function => "function",
        core_x::frontend::resolver::ItemKind::Struct => "struct",
        core_x::frontend::resolver::ItemKind::Enum => "enum",
        core_x::frontend::resolver::ItemKind::Protocol => "protocol",
    };

    if let Some(function) = semantic.typed_items.function(item.id) {
        let params = function
            .param_types
            .iter()
            .map(|ty| format_type(ty, &semantic.global_items))
            .collect::<Vec<_>>()
            .join(", ");
        let return_type = function
            .return_type
            .as_ref()
            .map(|ty| format_type(ty, &semantic.global_items))
            .unwrap_or_else(|| "void".to_string());
        return format!("{kind} {}({params}) -> {return_type}", item.name);
    }

    format!("{kind} {}", item.full_path.join("::"))
}

fn insert_completion_item(
    items: &mut BTreeMap<String, Value>,
    label: String,
    kind: i32,
    detail: String,
) {
    items.entry(label.clone()).or_insert_with(|| {
        json!({
            "label": label,
            "kind": kind,
            "detail": detail,
        })
    });
}

fn completion_kind_for_semantic_candidate(kind: SemanticCompletionKind) -> i32 {
    match kind {
        SemanticCompletionKind::Local => 6,
        SemanticCompletionKind::ImportScope | SemanticCompletionKind::Scope => {
            9
        }
        SemanticCompletionKind::ImportFunction
        | SemanticCompletionKind::Function => 3,
        SemanticCompletionKind::ImportStruct
        | SemanticCompletionKind::Struct => 22,
        SemanticCompletionKind::ImportEnum | SemanticCompletionKind::Enum => 13,
        SemanticCompletionKind::ImportProtocol
        | SemanticCompletionKind::Protocol => 8,
    }
}

fn hover_text_for_external_function(
    root_name: &str,
    path: &[String],
    signature: &core_x::frontend::TypedFunctionSignature,
    semantic: &SemanticAnalysis,
) -> String {
    let params = signature
        .param_types
        .iter()
        .map(|ty| format_type(ty, &semantic.global_items))
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = signature
        .return_type
        .as_ref()
        .map(|ty| format_type(ty, &semantic.global_items))
        .unwrap_or_else(|| "void".to_string());
    format!(
        "external function {}({params}) -> {return_type}",
        [root_name, &path.join("::")].join("::")
    )
}

fn format_type(
    ty: &core_x::frontend::Type,
    item_table: &GlobalItemTable,
) -> String {
    match ty {
        core_x::frontend::Type::Builtin(builtin) => builtin.to_string(),
        core_x::frontend::Type::Named { item_id, .. } => item_table
            .get(*item_id)
            .map(|item| item.full_path.join("::"))
            .unwrap_or_else(|| format!("item#{}", item_id.raw())),
        core_x::frontend::Type::Pointer {
            pointee,
            mutability,
        } => match mutability {
            core_x::frontend::Mutability::Const => {
                format!("*{}", format_type(pointee, item_table))
            }
            core_x::frontend::Mutability::Mut => {
                format!("*mut {}", format_type(pointee, item_table))
            }
        },
        core_x::frontend::Type::Error => "<error>".to_string(),
    }
}
