use crate::cli_driver::project::{
    build_dependency_named_roots, collect_project_cx_files,
};
use crate::lsp::convert::{
    LspPosition, LspRange, offset_to_position, path_to_uri, position_to_offset,
    span_to_lsp_range, word_span_at_position,
};
use crate::lsp::state::{DocumentPipelineState, ServerState};
use core_x::frontend::ast::{EnumMember, Item, ProtocolMember, StructMember};
use core_x::frontend::hir::{
    HirArrayElement, HirBodyId, HirExprId, HirExprKind, HirStmtKind,
    HirStructExprField,
};
use core_x::frontend::resolver::{ItemId, LocalId, ResolvedScopeKind};
use core_x::frontend::source::{FileId, SourceDb, SourceFile};
use core_x::frontend::{
    DefinitionLocation, DefinitionTarget, DesugaredFile, Diagnostic,
    DiagnosticsBag, ExternalSemanticLookup, FrontendContext, GlobalItem,
    GlobalItemTable, ImportRootKind, MacroDefinitionIndex, MacroScopeTable,
    ParseSessionError, ProjectLoader, SemanticAnalysis, SemanticCompletionKind,
    analyze_project, build_target_roots, completion_candidates_for_file,
    load_local_dependency_project_graph, local_binding_type,
    lookup_definition_target,
};
use core_x::midend::{
    BodyInferenceTable, CompletionContext, CompletionData, CompletionInput,
    CompletionKind, completion_candidates,
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
    pub parsed_files: Vec<DesugaredFile>,
    pub primary_file_id: FileId,
    pub diagnostics: DiagnosticsBag,
    pub imports: BTreeMap<FileId, core_x::frontend::ResolvedImports>,
    pub semantic: Option<SemanticAnalysis>,
    pub inference: Option<BodyInferenceTable>,
    external_lookup: ExternalSemanticLookup,
    path_by_file_id: BTreeMap<FileId, PathBuf>,
    file_id_by_path: BTreeMap<PathBuf, FileId>,
    item_definitions: BTreeMap<ItemId, DefinitionLocation>,
    method_definitions: BTreeMap<(ItemId, String), DefinitionLocation>,
    macro_definition_index: Option<MacroDefinitionIndex>,
    macro_scope_table: Option<MacroScopeTable>,
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
    state: &mut ServerState,
    uri: &str,
) -> Result<DocumentAnalysis, String> {
    let Some(document) = state.document(uri).cloned() else {
        return Err(format!("document is not open: {uri}"));
    };
    let open_text_by_path = state.open_text_by_path();
    if state.pipeline_state(uri).is_none() {
        let pipeline = if let Some(project_root) =
            find_project_root(&document.path)
        {
            build_project_pipeline_state(
                &document.path,
                &document.text,
                &open_text_by_path,
                &project_root,
            )
            .or_else(|_| {
                build_standalone_pipeline_state(&document.path, &document.text)
            })?
        } else {
            build_standalone_pipeline_state(&document.path, &document.text)?
        };
        state.upsert_pipeline_state(uri.to_string(), pipeline);
    }

    let Some(pipeline) = state.pipeline_state_mut(uri) else {
        return Err(format!("missing analysis pipeline state for {uri}"));
    };
    sync_pipeline_with_open_documents(pipeline, &open_text_by_path)?;
    let frontend_analysis =
        analyze_project(&mut pipeline.frontend, &pipeline.entry_files)
            .map_err(|error| {
                format_parse_session_error(
                    &pipeline.frontend,
                    error,
                    "failed to run canonical frontend analysis for",
                )
            })?;
    build_document_analysis_from_frontend(
        uri.to_string(),
        pipeline,
        frontend_analysis,
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
    let word_span = word_span_at_position(file, position);
    let word = word_span.as_ref().map(|(word, _)| word.clone());
    let highlight_span = word_span.map_or_else(
        || {
            let end = offset.saturating_add(1).min(file.len());
            core_x::frontend::ast::Span::new(offset, end)
        },
        |(_, span)| span,
    );

    if let Some(semantic) = &analysis.semantic {
        if let Some(target) = lookup_definition_target(
            semantic,
            &analysis.imports,
            &analysis.external_lookup,
            &analysis.item_definitions,
            analysis.primary_file_id,
            offset,
            word.as_deref(),
        ) {
            let hover_text = match target {
                DefinitionTarget::LocalBinding { local_id, .. } => {
                    let local_type = inferred_local_type_for_resolved_local(
                        analysis, semantic, local_id,
                    )
                    .or_else(|| local_binding_type(semantic, local_id))?;
                    let local_name = word.as_deref().unwrap_or("binding");
                    format!(
                        "local `{local_name}`: {}",
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
                "range": span_to_lsp_range(file, highlight_span),
            }));
        }

        if let Some(expr_hover_text) =
            inferred_expression_hover_text(analysis, semantic, offset)
        {
            return Some(json!({
                "contents": {
                    "kind": "plaintext",
                    "value": expr_hover_text,
                },
                "range": span_to_lsp_range(file, highlight_span),
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
    if let Some(semantic) = &analysis.semantic {
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

        if let Some(location) = method_location_for_position(
            analysis,
            semantic,
            offset,
            fallback_word.as_deref(),
        ) {
            return vec![location];
        }
    }

    if let Some(location) =
        macro_location_for_position(analysis, fallback_word.as_deref())
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

    let Some(offset) = position_to_offset(file, position) else {
        return Vec::new();
    };

    // Try HIR-driven completion first
    if let Some(semantic) = &analysis.semantic {
        // Prepare HIR files and analysis data for completion input
        let hir_files: BTreeMap<
            core_x::frontend::source::FileId,
            core_x::frontend::hir::HirFile,
        > = semantic
            .hir
            .hir_files
            .iter()
            .map(|hir_file| (hir_file.file_id, hir_file.clone()))
            .collect();

        // Get expression type table
        let expression_types = semantic.expr_types.clone();

        // Get signature table
        let signatures = semantic.signatures.clone();

        // Build completion input
        let completion_input = CompletionInput::new(
            &analysis.db,
            &hir_files,
            semantic,
            &signatures,
            &expression_types,
            &analysis.imports,
            &analysis.external_lookup,
        );

        // Compute completion candidates from midend
        let hir_completion_items = if let Some(completion_data) =
            completion_candidates(&completion_input, analysis.primary_file_id, offset)
        {
            convert_completion_data_to_lsp(completion_data, &prefix)
        } else {
            Vec::new()
        };

        // Build items map from HIR completion
        let mut items: BTreeMap<String, Value> = BTreeMap::new();
        for item in hir_completion_items {
            if let Some(label) = item.get("label").and_then(Value::as_str) {
                items.insert(label.to_string(), item);
            }
        }

        // Add keywords
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

        // Always add semantic completion candidates (they complement HIR completion)
        // The midend completion may not include all local bindings yet
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

        return items
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
            .collect();
    }

    Vec::new()
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
    let Some(inference) = &analysis.inference else {
        return Vec::new();
    };

    let mut seen = BTreeSet::new();
    let mut hints = Vec::new();
    for body in semantic.resolved_bodies.iter() {
        if body.containing_scope_file_id != analysis.primary_file_id {
            continue;
        }

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
            let Some(ty) = inference.local_type_for_resolved_local(
                &body.owner,
                body.body_index,
                local.id,
            ) else {
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

fn inferred_local_type_for_resolved_local<'a>(
    analysis: &'a DocumentAnalysis,
    semantic: &SemanticAnalysis,
    local_id: LocalId,
) -> Option<&'a core_x::frontend::Type> {
    let inference = analysis.inference.as_ref()?;
    for body in semantic.resolved_bodies.iter() {
        if !body.locals.iter().any(|local| local.id == local_id) {
            continue;
        }
        if let Some(ty) = inference.local_type_for_resolved_local(
            &body.owner,
            body.body_index,
            local_id,
        ) {
            return Some(ty);
        }
    }
    None
}

fn inferred_expression_hover_text(
    analysis: &DocumentAnalysis,
    semantic: &SemanticAnalysis,
    offset: usize,
) -> Option<String> {
    let inference = analysis.inference.as_ref()?;
    let mut best = None::<(usize, core_x::frontend::Type)>;

    for body in semantic.resolved_bodies.iter() {
        if body.containing_scope_file_id != analysis.primary_file_id {
            continue;
        }
        let Some(body_ref) =
            semantic.hir.body_ref(&body.owner, body.body_index)
        else {
            continue;
        };
        if body_ref.file_id != analysis.primary_file_id {
            continue;
        }
        let Some(module) = semantic.hir.hir_modules.get(&body_ref.file_id)
        else {
            continue;
        };

        for (expr_id, expr) in &module.exprs {
            let span = expr.origin.span;
            if !(span.start <= offset && offset <= span.end) {
                continue;
            }
            let Some(ty) = inference.expr_type_for_hir_expr(
                &body.owner,
                body.body_index,
                *expr_id,
            ) else {
                continue;
            };
            if ty.is_error() {
                continue;
            }
            let span_len = span.end.saturating_sub(span.start);
            if best
                .as_ref()
                .is_none_or(|(existing_len, _)| span_len <= *existing_len)
            {
                best = Some((span_len, ty.clone()));
            }
        }
    }

    let (_, ty) = best?;
    Some(format!(
        "expr: {}",
        format_type(&ty, &semantic.global_items)
    ))
}

fn build_standalone_pipeline_state(
    path: &Path,
    text: &str,
) -> Result<DocumentPipelineState, String> {
    let mut frontend = FrontendContext::new();
    let normalized_path = normalize_path(path);
    let primary_file_id = frontend.add_file(normalized_path, text.to_string());
    Ok(DocumentPipelineState {
        frontend,
        primary_file_id,
        entry_files: vec![primary_file_id],
    })
}

fn build_project_pipeline_state(
    path: &Path,
    open_text: &str,
    open_text_by_path: &BTreeMap<PathBuf, String>,
    project_root: &Path,
) -> Result<DocumentPipelineState, String> {
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
        return Err(format!(
            "document {} is not part of project {}",
            path.display(),
            project_root.display()
        ));
    }

    let mut frontend = FrontendContext::new();
    for file_path in &files {
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
        frontend.add_file(normalized, source);
    }

    let file_id_by_path = frontend.file_id_by_path().clone();
    let Some(primary_file_id) = file_id_by_path.get(&path).copied() else {
        return Err(format!(
            "missing file id for open project document {}",
            path.display()
        ));
    };

    for binary in &manifest.binaries {
        let root_path = normalize_path(&binary.root_file);
        if let Some(file_id) = file_id_by_path.get(&root_path).copied() {
            frontend.set_root_kind(file_id, ResolvedScopeKind::BinaryRoot);
        }
    }
    if let Some(library) = &manifest.library {
        let root_path = normalize_path(&library.root_file);
        if let Some(file_id) = file_id_by_path.get(&root_path).copied() {
            frontend.set_root_kind(file_id, ResolvedScopeKind::Root);
        }
    }

    let current_library_import_root =
        target_roots.by_name.iter().find_map(|(name, root)| {
            (root.kind == ImportRootKind::CurrentLibrary).then(|| name.clone())
        });
    let dependency_named_roots =
        build_dependency_named_roots(&project_graph, &target_roots).map_err(
            |error| format!("failed to build dependency import roots: {error}"),
        )?;
    frontend.set_dependency_named_roots(dependency_named_roots);
    let library_root_file_id = manifest.library.as_ref().and_then(|library| {
        file_id_by_path
            .get(&normalize_path(&library.root_file))
            .copied()
    });
    frontend.configure_current_library_root(
        current_library_import_root,
        library_root_file_id,
    );

    let (root_kind, root_file_id) =
        select_target_for_file(&manifest, &path, &file_id_by_path)?;
    frontend.set_root_kind(root_file_id, root_kind);

    Ok(DocumentPipelineState {
        frontend,
        primary_file_id,
        entry_files: vec![root_file_id],
    })
}

fn sync_pipeline_with_open_documents(
    pipeline: &mut DocumentPipelineState,
    open_text_by_path: &BTreeMap<PathBuf, String>,
) -> Result<(), String> {
    for (path, text) in open_text_by_path {
        let normalized = normalize_path(path);
        let Some(file_id) = pipeline.frontend.file_id_for_path(&normalized)
        else {
            continue;
        };
        pipeline
            .frontend
            .replace_file_source(file_id, text.clone())
            .map_err(|error| {
                format_parse_session_error(
                    &pipeline.frontend,
                    error,
                    "failed to update open document source for",
                )
            })?;
    }
    Ok(())
}

fn build_document_analysis_from_frontend(
    uri: String,
    pipeline: &DocumentPipelineState,
    frontend_analysis: core_x::frontend::FrontendAnalysis,
) -> Result<DocumentAnalysis, String> {
    let primary_entry = pipeline
        .entry_files
        .first()
        .copied()
        .unwrap_or(pipeline.primary_file_id);
    let resolution = frontend_analysis.resolution_tables.get(&primary_entry);
    let imports = resolution
        .map(|tables| tables.imports.clone())
        .unwrap_or_default();
    let semantic = frontend_analysis
        .semantic_tables
        .get(&primary_entry)
        .cloned();
    let inference = frontend_analysis
        .inference_tables
        .get(&primary_entry)
        .cloned();
    let external_lookup = resolution
        .map(|tables| tables.external_lookup.clone())
        .unwrap_or_else(ExternalSemanticLookup::new);
    let item_definitions = resolution
        .map(|tables| tables.item_definitions.clone())
        .unwrap_or_default();
    let method_definitions = semantic
        .as_ref()
        .map(|semantic| {
            collect_method_definitions(semantic, &frontend_analysis.desugared)
        })
        .unwrap_or_default();
    let macro_definition_index =
        pipeline.frontend.macro_definition_index().cloned();
    let macro_scope_table = pipeline.frontend.macro_scope_table().cloned();

    Ok(DocumentAnalysis {
        uri,
        db: pipeline.frontend.db().clone(),
        parsed_files: frontend_analysis.desugared,
        primary_file_id: pipeline.primary_file_id,
        diagnostics: frontend_analysis.diagnostics,
        imports,
        semantic,
        inference,
        external_lookup,
        path_by_file_id: pipeline.frontend.path_by_file_id().clone(),
        file_id_by_path: pipeline.frontend.file_id_by_path().clone(),
        item_definitions,
        method_definitions,
        macro_definition_index,
        macro_scope_table,
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

fn format_parse_session_error(
    context: &FrontendContext,
    error: ParseSessionError,
    message_prefix: &str,
) -> String {
    match error {
        ParseSessionError::MissingFile { file_id } => {
            format!(
                "{} missing source file id {}",
                message_prefix,
                file_id.raw()
            )
        }
        ParseSessionError::Parse(file_error) => {
            let path =
                context.path_for_file_id(file_error.file_id).map_or_else(
                    || format!("<unknown:{}>", file_error.file_id.raw()),
                    |path| path.display().to_string(),
                );
            format!("{} {}: {}", message_prefix, path, file_error.error)
        }
    }
}

fn collect_method_definitions(
    semantic: &SemanticAnalysis,
    parsed_files: &[DesugaredFile],
) -> BTreeMap<(ItemId, String), DefinitionLocation> {
    let mut method_definitions = BTreeMap::new();

    for parsed in parsed_files {
        let in_scope_items = semantic
            .global_items
            .items_in_scope(parsed.file_id)
            .into_iter()
            .map(|item| (item.name.clone(), item.kind, item.id))
            .collect::<Vec<_>>();
        for item in &parsed.ast.items {
            let (item_name, item_kind) = match &item.node {
                Item::Struct(struct_decl) => (
                    struct_decl.node.name.clone(),
                    core_x::frontend::ItemKind::Struct,
                ),
                Item::Enum(enum_decl) => (
                    enum_decl.node.name.clone(),
                    core_x::frontend::ItemKind::Enum,
                ),
                Item::Protocol(protocol_decl) => (
                    protocol_decl.node.name.clone(),
                    core_x::frontend::ItemKind::Protocol,
                ),
                _ => continue,
            };
            let Some(item_id) =
                in_scope_items.iter().find_map(|(name, kind, id)| {
                    (name == &item_name && *kind == item_kind).then_some(*id)
                })
            else {
                continue;
            };
            match &item.node {
                Item::Struct(struct_decl) => {
                    for member in &struct_decl.node.members {
                        match &member.node {
                            StructMember::Function(function_decl) => {
                                method_definitions.insert(
                                    (item_id, function_decl.node.name.clone()),
                                    DefinitionLocation {
                                        file_id: parsed.file_id,
                                        span: member.span,
                                    },
                                );
                            }
                            StructMember::Init(_) => {
                                method_definitions.insert(
                                    (item_id, "init".to_string()),
                                    DefinitionLocation {
                                        file_id: parsed.file_id,
                                        span: member.span,
                                    },
                                );
                            }
                            StructMember::Field(_) => {}
                        }
                    }
                }
                Item::Enum(enum_decl) => {
                    for member in &enum_decl.node.members {
                        match &member.node {
                            EnumMember::Function(function_decl) => {
                                method_definitions.insert(
                                    (item_id, function_decl.node.name.clone()),
                                    DefinitionLocation {
                                        file_id: parsed.file_id,
                                        span: member.span,
                                    },
                                );
                            }
                            EnumMember::Init(_) => {
                                method_definitions.insert(
                                    (item_id, "init".to_string()),
                                    DefinitionLocation {
                                        file_id: parsed.file_id,
                                        span: member.span,
                                    },
                                );
                            }
                            EnumMember::Case(_) => {}
                        }
                    }
                }
                Item::Protocol(protocol_decl) => {
                    for member in &protocol_decl.node.members {
                        match &member.node {
                            ProtocolMember::Function(function_member) => {
                                method_definitions.insert(
                                    (
                                        item_id,
                                        function_member.node.name.clone(),
                                    ),
                                    DefinitionLocation {
                                        file_id: parsed.file_id,
                                        span: member.span,
                                    },
                                );
                            }
                            ProtocolMember::Initializer(_) => {
                                method_definitions.insert(
                                    (item_id, "init".to_string()),
                                    DefinitionLocation {
                                        file_id: parsed.file_id,
                                        span: member.span,
                                    },
                                );
                            }
                            ProtocolMember::AssociatedType(_)
                            | ProtocolMember::Property(_) => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    method_definitions
}

fn method_location_for_position(
    analysis: &DocumentAnalysis,
    semantic: &SemanticAnalysis,
    offset: usize,
    fallback_word: Option<&str>,
) -> Option<Value> {
    for body in semantic.resolved_bodies.iter() {
        if body.containing_scope_file_id != analysis.primary_file_id {
            continue;
        }
        let body_ref = semantic.hir.body_ref(&body.owner, body.body_index)?;
        if body_ref.file_id != analysis.primary_file_id {
            continue;
        }
        let module = semantic.hir.hir_modules.get(&body_ref.file_id)?;
        let Some(method_call) = find_method_call_at_offset(
            module,
            body_ref.body_id,
            offset,
            fallback_word,
        ) else {
            continue;
        };
        let Some(receiver_ty) = semantic.expr_types.expr_type_for_hir_expr(
            &body.owner,
            body.body_index,
            method_call.receiver_expr_id,
        ) else {
            continue;
        };
        let Some(receiver_item_id) = named_item_id_from_type(receiver_ty)
        else {
            continue;
        };
        let key = (receiver_item_id, method_call.method_name.clone());
        let Some(location) = analysis.method_definitions.get(&key) else {
            continue;
        };
        if let Some(value) = location_for_file_and_span(
            analysis,
            location.file_id,
            location.span,
        ) {
            return Some(value);
        }
    }
    if let Some(word) = fallback_word {
        let matches = analysis
            .method_definitions
            .iter()
            .filter_map(|((_, name), location)| {
                (name == word).then_some(*location)
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            let location = matches[0];
            return location_for_file_and_span(
                analysis,
                location.file_id,
                location.span,
            );
        }
    }
    None
}

#[derive(Debug, Clone)]
struct MethodCallMatch {
    receiver_expr_id: HirExprId,
    method_name: String,
    span_len: usize,
}

fn find_method_call_at_offset(
    module: &core_x::frontend::HirModule,
    body_id: HirBodyId,
    offset: usize,
    fallback_word: Option<&str>,
) -> Option<MethodCallMatch> {
    let body = module.bodies.get(&body_id)?;
    let mut best = None::<MethodCallMatch>;
    for stmt_id in &body.stmts {
        let Some(stmt) = module.stmts.get(stmt_id) else {
            continue;
        };
        match &stmt.kind {
            HirStmtKind::Let(let_stmt) => {
                if let Some(value) = let_stmt.value {
                    search_method_call_in_expr(
                        module,
                        value,
                        offset,
                        fallback_word,
                        &mut best,
                    );
                }
            }
            HirStmtKind::Expr { expr } | HirStmtKind::Semi { expr } => {
                search_method_call_in_expr(
                    module,
                    *expr,
                    offset,
                    fallback_word,
                    &mut best,
                );
            }
            HirStmtKind::Item { .. } => {}
        }
    }
    if let Some(tail_expr) = body.tail_expr {
        search_method_call_in_expr(
            module,
            tail_expr,
            offset,
            fallback_word,
            &mut best,
        );
    }
    best
}

fn search_method_call_in_expr(
    module: &core_x::frontend::HirModule,
    expr_id: HirExprId,
    offset: usize,
    fallback_word: Option<&str>,
    best: &mut Option<MethodCallMatch>,
) {
    let Some(expr) = module.exprs.get(&expr_id) else {
        return;
    };
    let span = expr.origin.span;
    if span.start <= offset && offset <= span.end {
        if let HirExprKind::MethodCall {
            receiver,
            method_name,
            ..
        } = &expr.kind
        {
            if fallback_word.map_or(true, |word| word == method_name.as_str()) {
                let candidate = MethodCallMatch {
                    receiver_expr_id: *receiver,
                    method_name: method_name.clone(),
                    span_len: span.end.saturating_sub(span.start),
                };
                if best.as_ref().map_or(true, |existing| {
                    candidate.span_len <= existing.span_len
                }) {
                    *best = Some(candidate);
                }
            }
        }
    }

    match &expr.kind {
        HirExprKind::Array { elements } => {
            for element in elements {
                let child = match element {
                    HirArrayElement::Expr(id) | HirArrayElement::Spread(id) => {
                        *id
                    }
                };
                search_method_call_in_expr(
                    module,
                    child,
                    offset,
                    fallback_word,
                    best,
                );
            }
        }
        HirExprKind::Call { callee, args } => {
            search_method_call_in_expr(
                module,
                *callee,
                offset,
                fallback_word,
                best,
            );
            for arg in args {
                search_method_call_in_expr(
                    module,
                    arg.value,
                    offset,
                    fallback_word,
                    best,
                );
            }
        }
        HirExprKind::Block { body } => {
            if let Some(child) =
                find_method_call_at_offset(module, *body, offset, fallback_word)
            {
                if best.as_ref().map_or(true, |existing| {
                    child.span_len <= existing.span_len
                }) {
                    *best = Some(child);
                }
            }
        }
        HirExprKind::If {
            condition,
            then_body,
            else_expr,
        } => {
            search_method_call_in_expr(
                module,
                *condition,
                offset,
                fallback_word,
                best,
            );
            if let Some(child) = find_method_call_at_offset(
                module,
                *then_body,
                offset,
                fallback_word,
            ) {
                if best.as_ref().map_or(true, |existing| {
                    child.span_len <= existing.span_len
                }) {
                    *best = Some(child);
                }
            }
            if let Some(else_expr) = else_expr {
                search_method_call_in_expr(
                    module,
                    *else_expr,
                    offset,
                    fallback_word,
                    best,
                );
            }
        }
        HirExprKind::While { condition, body } => {
            search_method_call_in_expr(
                module,
                *condition,
                offset,
                fallback_word,
                best,
            );
            if let Some(child) =
                find_method_call_at_offset(module, *body, offset, fallback_word)
            {
                if best.as_ref().map_or(true, |existing| {
                    child.span_len <= existing.span_len
                }) {
                    *best = Some(child);
                }
            }
        }
        HirExprKind::For { iterator, body, .. } => {
            search_method_call_in_expr(
                module,
                *iterator,
                offset,
                fallback_word,
                best,
            );
            if let Some(child) =
                find_method_call_at_offset(module, *body, offset, fallback_word)
            {
                if best.as_ref().map_or(true, |existing| {
                    child.span_len <= existing.span_len
                }) {
                    *best = Some(child);
                }
            }
        }
        HirExprKind::Return { value } => {
            if let Some(value) = value {
                search_method_call_in_expr(
                    module,
                    *value,
                    offset,
                    fallback_word,
                    best,
                );
            }
        }
        HirExprKind::Assign { target, value, .. } => {
            search_method_call_in_expr(
                module,
                *target,
                offset,
                fallback_word,
                best,
            );
            search_method_call_in_expr(
                module,
                *value,
                offset,
                fallback_word,
                best,
            );
        }
        HirExprKind::Unary { expr, .. }
        | HirExprKind::ForceUnwrap { expr }
        | HirExprKind::Cast { expr, .. }
        | HirExprKind::Spread { expr }
        | HirExprKind::Try { expr } => {
            search_method_call_in_expr(
                module,
                *expr,
                offset,
                fallback_word,
                best,
            );
        }
        HirExprKind::Binary { lhs, rhs, .. } => {
            search_method_call_in_expr(
                module,
                *lhs,
                offset,
                fallback_word,
                best,
            );
            search_method_call_in_expr(
                module,
                *rhs,
                offset,
                fallback_word,
                best,
            );
        }
        HirExprKind::Field { base, .. }
        | HirExprKind::OptionalField { base, .. }
        | HirExprKind::NamespaceField { base, .. } => {
            search_method_call_in_expr(
                module,
                *base,
                offset,
                fallback_word,
                best,
            );
        }
        HirExprKind::MethodCall { receiver, args, .. } => {
            search_method_call_in_expr(
                module,
                *receiver,
                offset,
                fallback_word,
                best,
            );
            for arg in args {
                search_method_call_in_expr(
                    module,
                    arg.value,
                    offset,
                    fallback_word,
                    best,
                );
            }
        }
        HirExprKind::Index { base, index }
        | HirExprKind::OptionalIndex { base, index } => {
            search_method_call_in_expr(
                module,
                *base,
                offset,
                fallback_word,
                best,
            );
            search_method_call_in_expr(
                module,
                *index,
                offset,
                fallback_word,
                best,
            );
        }
        HirExprKind::Tuple { elements } => {
            for element in elements {
                search_method_call_in_expr(
                    module,
                    *element,
                    offset,
                    fallback_word,
                    best,
                );
            }
        }
        HirExprKind::Struct { fields, .. } => {
            for field in fields {
                let value = match field {
                    HirStructExprField::Named { value, .. }
                    | HirStructExprField::Spread { value } => value,
                };
                search_method_call_in_expr(
                    module,
                    *value,
                    offset,
                    fallback_word,
                    best,
                );
            }
        }
        HirExprKind::Match { subject, arms } => {
            search_method_call_in_expr(
                module,
                *subject,
                offset,
                fallback_word,
                best,
            );
            for arm in arms {
                search_method_call_in_expr(
                    module,
                    arm.expr,
                    offset,
                    fallback_word,
                    best,
                );
            }
        }
        HirExprKind::Closure { body, .. } => {
            if let Some(child) =
                find_method_call_at_offset(module, *body, offset, fallback_word)
            {
                if best.as_ref().map_or(true, |existing| {
                    child.span_len <= existing.span_len
                }) {
                    *best = Some(child);
                }
            }
        }
        HirExprKind::Range { start, end, .. } => {
            if let Some(start) = start {
                search_method_call_in_expr(
                    module,
                    *start,
                    offset,
                    fallback_word,
                    best,
                );
            }
            if let Some(end) = end {
                search_method_call_in_expr(
                    module,
                    *end,
                    offset,
                    fallback_word,
                    best,
                );
            }
        }
        HirExprKind::Literal(_)
        | HirExprKind::Path(_)
        | HirExprKind::Break
        | HirExprKind::Continue => {}
    }
}

fn named_item_id_from_type(ty: &core_x::frontend::Type) -> Option<ItemId> {
    match ty {
        core_x::frontend::Type::Named { item_id, .. } => Some(*item_id),
        core_x::frontend::Type::Pointer { pointee, .. } => {
            named_item_id_from_type(pointee)
        }
        core_x::frontend::Type::Builtin(_) | core_x::frontend::Type::Error => {
            None
        }
    }
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

fn macro_location_for_position(
    analysis: &DocumentAnalysis,
    fallback_word: Option<&str>,
) -> Option<Value> {
    let macro_name = fallback_word?;
    let macro_scope_table = analysis.macro_scope_table.as_ref()?;
    let macro_definition_index = analysis.macro_definition_index.as_ref()?;
    let resolved_macro_name = macro_scope_table
        .binding_for_file(analysis.primary_file_id, macro_name)
        .map_or(macro_name, |binding| binding.macro_name.as_str());
    let (file_id, span) =
        macro_definition_index.declaration_location(resolved_macro_name)?;
    location_for_file_and_span(analysis, file_id, span)
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

/// Convert HIR-driven completion data to LSP completion items.
///
/// This function converts the semantic completion results from the new
/// midend::completion system into LSP completion items.
fn convert_completion_data_to_lsp(
    completion_data: CompletionData,
    prefix: &str,
) -> Vec<Value> {
    let context_info = format_completion_context(&completion_data.context);

    completion_data
        .candidates
        .into_iter()
        .filter_map(|candidate| {
            // Filter by prefix if provided
            if !prefix.is_empty() && !candidate.label.starts_with(prefix) {
                return None;
            }

            let lsp_kind = completion_kind_to_lsp(candidate.kind);

            Some(json!({
                "label": candidate.label,
                "kind": lsp_kind,
                "detail": candidate.detail.unwrap_or_default(),
                "documentation": candidate.documentation.unwrap_or_default(),
                "context": context_info,
                "deprecated": candidate.metadata.deprecated,
            }))
        })
        .collect()
}

/// Format completion context for debugging/display purposes.
fn format_completion_context(context: &CompletionContext) -> String {
    match context {
        CompletionContext::Global => "global".to_string(),
        CompletionContext::PathAccess { scope_item } => {
            format!("path::{:?}", scope_item)
        }
        CompletionContext::AssociatedAccess { base_type } => {
            format!("associated::{:?}", base_type)
        }
        CompletionContext::MemberAccess { receiver_type } => {
            format!("member::{:?}", receiver_type)
        }
        CompletionContext::EnumCaseAccess { enum_type } => {
            format!("enum::{:?}", enum_type)
        }
    }
}

/// Convert internal CompletionKind to LSP completion item kind.
///
/// LSP completion item kinds are defined by the Language Server Protocol.
/// See: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#completionItemKind
fn completion_kind_to_lsp(kind: CompletionKind) -> i32 {
    match kind {
        CompletionKind::Local => 6,           // Variable
        CompletionKind::Function => 3,        // Function
        CompletionKind::Struct => 22,         // Struct
        CompletionKind::Enum => 13,           // Enum
        CompletionKind::EnumVariant => 12, // EnumMember (LSP 3.15+) or close to it
        CompletionKind::Protocol => 8,     // Interface
        CompletionKind::Field => 5,        // Field
        CompletionKind::Scope => 9,        // Module
        CompletionKind::TypeParameter => 14, // TypeParameter
        CompletionKind::AssociatedType => 14, // TypeParameter (close enough)
        CompletionKind::Property => 10,    // Property
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
