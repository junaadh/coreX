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
    ImportBindingKind, ItemId, NamedImportRoot, ResolvedBodyRef,
    ResolvedImportBinding, ResolvedScopeKind, ScopeGraph, ScopeResolver,
    resolve_project_imports_with_named_roots_and_diagnostics,
};
use core_x::frontend::source::{FileId, SourceDb, SourceFile};
use core_x::frontend::{
    Diagnostic, DiagnosticsBag, GlobalItem, GlobalItemTable, ParsedFile,
    ProjectLoader, SemanticAnalysis, Type, TypedFunctionSignature,
    analyze_semantics_with_external_lookup,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub struct DocumentAnalysis {
    pub uri: String,
    pub db: SourceDb,
    pub parsed_files: Vec<ParsedFile>,
    pub primary_file_id: FileId,
    pub diagnostics: DiagnosticsBag,
    pub imports: BTreeMap<FileId, core_x::frontend::ResolvedImports>,
    pub semantic: Option<SemanticAnalysis>,
    path_by_file_id: BTreeMap<FileId, PathBuf>,
    item_definitions: BTreeMap<ItemId, (FileId, core_x::frontend::ast::Span)>,
}

pub fn analyze_document(
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
        if let Some(result) =
            hover_from_body_reference(analysis, semantic, offset)
        {
            return Some(result);
        }

        if let Some(global_item) = item_from_word(semantic, &word, analysis) {
            return Some(json!({
                "contents": {
                    "kind": "plaintext",
                    "value": hover_text_for_item(global_item, semantic),
                },
                "range": span_to_lsp_range(file, span),
            }));
        }

        if let Some(binding) = analysis
            .imports
            .get(&analysis.primary_file_id)
            .and_then(|imports| imports.get(&word))
            && let Some(item_id) = item_id_from_binding(binding, semantic)
            && let Some(global_item) = semantic.global_items.get(item_id)
        {
            return Some(json!({
                "contents": {
                    "kind": "plaintext",
                    "value": hover_text_for_item(global_item, semantic),
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
    let Some((word, _)) = word_span_at_position(file, position) else {
        return Vec::new();
    };
    let Some(semantic) = &analysis.semantic else {
        return Vec::new();
    };

    if let Some(location) =
        definition_local_location_from_reference(analysis, semantic, offset)
    {
        return vec![location];
    }

    if let Some(item_id) =
        definition_item_id_from_reference(semantic, analysis, offset)
    {
        if let Some(location) = location_for_item_id(analysis, item_id) {
            return vec![location];
        }
    }

    if let Some(binding) = analysis
        .imports
        .get(&analysis.primary_file_id)
        .and_then(|imports| imports.get(&word))
    {
        if let Some(item_id) = item_id_from_binding(binding, semantic)
            && let Some(location) = location_for_item_id(analysis, item_id)
        {
            return vec![location];
        }
        if matches!(binding.kind, ImportBindingKind::Scope)
            && let Some(path) =
                analysis.path_by_file_id.get(&binding.target_file_id)
            && let Some(target_file) = analysis.db.file(binding.target_file_id)
        {
            let zero = core_x::frontend::ast::Span::new(0, 0);
            let range = span_to_lsp_range(target_file, zero);
            return vec![json!({
                "uri": path_to_uri(path),
                "range": range,
            })];
        }
    }

    if let Some(global_item) = item_from_word(semantic, &word, analysis)
        && let Some(location) = location_for_item_id(analysis, global_item.id)
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
        "else", "while", "for", "return", "root", "super",
    ] {
        insert_completion_item(
            &mut items,
            keyword.to_string(),
            14,
            "keyword".to_string(),
        );
    }

    if let Some(semantic) = &analysis.semantic {
        for body in semantic.resolved_bodies.iter() {
            if body.containing_scope_file_id != analysis.primary_file_id {
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
                            format_type(ty, &semantic.global_items)
                        )
                    })
                    .unwrap_or_else(|| "local".to_string());
                insert_completion_item(
                    &mut items,
                    local.name.clone(),
                    6,
                    detail,
                );
            }
        }

        if let Some(imports) = analysis.imports.get(&analysis.primary_file_id) {
            for binding in imports.bindings.values() {
                let kind = match binding.kind {
                    ImportBindingKind::Scope => 9,
                    ImportBindingKind::Symbol(symbol_kind) => {
                        use core_x::frontend::resolver::SymbolKind;
                        match symbol_kind {
                            SymbolKind::Function => 3,
                            SymbolKind::Struct => 22,
                            SymbolKind::Enum => 13,
                            SymbolKind::Protocol => 8,
                            SymbolKind::Scope => 9,
                        }
                    }
                };
                let detail =
                    format!("import {}", binding.target_path.join("::"));
                insert_completion_item(
                    &mut items,
                    binding.local_name.clone(),
                    kind,
                    detail,
                );
            }
        }

        for item in semantic
            .global_items
            .items_in_scope(analysis.primary_file_id)
        {
            let kind = completion_kind_for_item(item.kind);
            let detail = item_kind_label(item.kind).to_string();
            insert_completion_item(&mut items, item.name.clone(), kind, detail);
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
    let parsed_files = vec![parsed];

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

        let external_lookup = build_external_semantic_lookup(
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
        item_definitions = collect_item_definition_spans(
            graph,
            &parsed_files,
            &semantic_result.global_items,
        );
        semantic = Some(semantic_result);
    }

    let mut path_by_file_id = BTreeMap::new();
    path_by_file_id.insert(file_id, path.clone());

    Ok(DocumentAnalysis {
        uri,
        db,
        parsed_files,
        primary_file_id: file_id,
        diagnostics,
        imports,
        semantic,
        path_by_file_id,
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
    let mut parsed_files = Vec::with_capacity(files.len());
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
        let parsed = parse_source_file_from_source_file_with_recovery(file)
            .map_err(|error| format!("failed to initialize parser: {error}"))?;
        parsed_files.push(parsed);
        path_by_file_id.insert(file_id, normalized.clone());
        file_id_by_path.insert(normalized, file_id);
    }

    let Some(primary_file_id) = file_id_by_path.get(&path).copied() else {
        return analyze_standalone(uri, path, open_text.to_string());
    };

    let mut diagnostics = DiagnosticsBag::new();
    for parsed in &parsed_files {
        diagnostics.extend(parsed.diagnostics.as_slice().iter().cloned());
    }

    let (root_kind, root_file_id) =
        select_target_for_file(&manifest, &path, &file_id_by_path)?;

    let scope_resolver = ScopeResolver::new(&db, &parsed_files);
    let (graph, scope_diagnostics) =
        resolve_target_scope_graph_with_diagnostics(
            &scope_resolver,
            &db,
            &parsed_files,
            root_file_id,
            root_kind,
        );
    diagnostics.extend(scope_diagnostics.as_slice().iter().cloned());

    let mut imports = BTreeMap::new();
    let mut semantic = None;
    let mut item_definitions = BTreeMap::new();
    if let Some(graph) = &graph {
        let mut named_roots = BTreeMap::new();
        if root_kind == ResolvedScopeKind::BinaryRoot {
            if let Some(library_target) = &manifest.library {
                if let Some(library_root_id) = file_id_by_path
                    .get(&normalize_path(&library_target.root_file))
                {
                    let (library_graph, library_diagnostics) = scope_resolver
                        .resolve_library_root_with_diagnostics(
                            *library_root_id,
                            &db,
                        );
                    diagnostics
                        .extend(library_diagnostics.as_slice().iter().cloned());
                    if let Some(library_graph) = library_graph {
                        named_roots.insert(
                            library_target.name.clone(),
                            NamedImportRoot::LoadedLibrary {
                                graph: library_graph,
                                parsed_files: parsed_files.clone(),
                            },
                        );
                    }
                }
            }
        }

        let (symbols, resolved_imports, import_diagnostics) =
            resolve_project_imports_with_named_roots_and_diagnostics(
                graph,
                &parsed_files,
                &named_roots,
                &db,
            );
        let _ = symbols;
        diagnostics.extend(import_diagnostics.as_slice().iter().cloned());
        imports = resolved_imports;

        let external_lookup = build_external_semantic_lookup(
            &db,
            &named_roots,
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
        item_definitions = collect_item_definition_spans(
            graph,
            &parsed_files,
            &semantic_result.global_items,
        );
        semantic = Some(semantic_result);
    }

    Ok(DocumentAnalysis {
        uri,
        db,
        parsed_files,
        primary_file_id,
        diagnostics,
        imports,
        semantic,
        path_by_file_id,
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

fn build_external_semantic_lookup(
    db: &SourceDb,
    named_roots: &BTreeMap<String, NamedImportRoot>,
    graph: &ScopeGraph,
    parsed_files: &[ParsedFile],
) -> core_x::frontend::ExternalSemanticLookup {
    let mut lookup = core_x::frontend::ExternalSemanticLookup::new();

    for (root_name, root) in named_roots {
        let NamedImportRoot::LoadedLibrary {
            graph,
            parsed_files,
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
            graph,
            parsed_files,
            &imports,
            &core_x::frontend::ExternalSemanticLookup::new(),
        );
        for item in semantic.global_items.iter() {
            if let Some(signature) = semantic.typed_items.function(item.id) {
                lookup.insert_named_root_function(
                    root_name.clone(),
                    item.full_path.clone(),
                    signature.clone(),
                );
            }
        }
    }

    let parsed_by_id: BTreeMap<FileId, &ParsedFile> = parsed_files
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect();

    for scope_file_id in graph.scopes.keys() {
        let Some(parsed) = parsed_by_id.get(scope_file_id) else {
            continue;
        };
        for item in &parsed.ast.items {
            let core_x::frontend::ast::Item::ExternBlock(extern_block) =
                &item.node
            else {
                continue;
            };
            let library_name = extern_block.node.library_name.clone();
            for member in &extern_block.node.members {
                match &member.node {
                    core_x::frontend::ast::ExternMember::Function(function) => {
                        lookup.insert_extern_function(
                            library_name.clone(),
                            function.node.local_name.clone(),
                            extern_function_signature(&function.node),
                        );
                    }
                }
            }
        }
    }

    lookup
}

fn extern_function_signature(
    decl: &core_x::frontend::ast::ExternFunctionDecl,
) -> TypedFunctionSignature {
    TypedFunctionSignature {
        param_types: vec![Type::error(); decl.params.len()],
        return_type: decl.return_type.as_ref().map(|_| Type::error()),
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

fn collect_item_definition_spans(
    graph: &ScopeGraph,
    parsed_files: &[ParsedFile],
    item_table: &GlobalItemTable,
) -> BTreeMap<ItemId, (FileId, core_x::frontend::ast::Span)> {
    let parsed_by_id: BTreeMap<FileId, &ParsedFile> = parsed_files
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect();
    let mut spans = BTreeMap::new();

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
                _ => None,
            };
            let Some(name) = name else {
                continue;
            };
            let mut full_path = scope.scope_path.clone();
            full_path.push(name);
            if let Some(item_id) = item_table.item_id_by_full_path(&full_path) {
                spans.entry(item_id).or_insert((*scope_file_id, item.span));
            }
        }
    }
    spans
}

fn hover_from_body_reference(
    analysis: &DocumentAnalysis,
    semantic: &SemanticAnalysis,
    offset: usize,
) -> Option<Value> {
    for body in semantic.resolved_bodies.iter() {
        if body.containing_scope_file_id != analysis.primary_file_id {
            continue;
        }
        for reference in &body.references {
            if !(reference.span.start <= offset && offset <= reference.span.end)
            {
                continue;
            }
            match reference.resolved {
                ResolvedBodyRef::Local(local_id) => {
                    let typed_body = semantic
                        .typed_bodies
                        .body(&body.owner, body.body_index)?;
                    let local_type = typed_body.local_types.get(&local_id)?;
                    let file = analysis.db.file(analysis.primary_file_id)?;
                    return Some(json!({
                        "contents": {
                            "kind": "plaintext",
                            "value": format!("local `{}`: {}", reference.segments.last().cloned().unwrap_or_default(), format_type(local_type, &semantic.global_items)),
                        },
                        "range": span_to_lsp_range(file, reference.span),
                    }));
                }
                ResolvedBodyRef::Item(item_id)
                | ResolvedBodyRef::Import(item_id) => {
                    let global_item = semantic.global_items.get(item_id)?;
                    let file = analysis.db.file(analysis.primary_file_id)?;
                    return Some(json!({
                        "contents": {
                            "kind": "plaintext",
                            "value": hover_text_for_item(global_item, semantic),
                        },
                        "range": span_to_lsp_range(file, reference.span),
                    }));
                }
                ResolvedBodyRef::Unresolved => {}
            }
        }
    }
    None
}

fn definition_local_location_from_reference(
    analysis: &DocumentAnalysis,
    semantic: &SemanticAnalysis,
    offset: usize,
) -> Option<Value> {
    for body in semantic.resolved_bodies.iter() {
        if body.containing_scope_file_id != analysis.primary_file_id {
            continue;
        }
        for reference in &body.references {
            if !(reference.span.start <= offset && offset <= reference.span.end)
            {
                continue;
            }
            let ResolvedBodyRef::Local(local_id) = reference.resolved else {
                continue;
            };
            let local =
                body.locals.iter().find(|local| local.id == local_id)?;
            let file = analysis.db.file(analysis.primary_file_id)?;
            let path =
                analysis.path_by_file_id.get(&analysis.primary_file_id)?;
            return Some(json!({
                "uri": path_to_uri(path),
                "range": span_to_lsp_range(file, local.declared_span),
            }));
        }
    }
    None
}

fn definition_item_id_from_reference(
    semantic: &SemanticAnalysis,
    analysis: &DocumentAnalysis,
    offset: usize,
) -> Option<ItemId> {
    for body in semantic.resolved_bodies.iter() {
        if body.containing_scope_file_id != analysis.primary_file_id {
            continue;
        }
        for reference in &body.references {
            if !(reference.span.start <= offset && offset <= reference.span.end)
            {
                continue;
            }
            match reference.resolved {
                ResolvedBodyRef::Item(item_id)
                | ResolvedBodyRef::Import(item_id) => {
                    return Some(item_id);
                }
                ResolvedBodyRef::Local(_) | ResolvedBodyRef::Unresolved => {}
            }
        }
    }
    None
}

fn item_id_from_binding(
    binding: &ResolvedImportBinding,
    semantic: &SemanticAnalysis,
) -> Option<ItemId> {
    if !matches!(binding.kind, ImportBindingKind::Symbol(_)) {
        return None;
    }
    semantic
        .global_items
        .item_id_by_full_path(&binding.target_path)
}

fn item_from_word<'a>(
    semantic: &'a SemanticAnalysis,
    word: &str,
    analysis: &DocumentAnalysis,
) -> Option<&'a GlobalItem> {
    semantic.global_items.iter().find(|item| {
        item.name == word && item.defining_file_id == analysis.primary_file_id
    })
}

fn location_for_item_id(
    analysis: &DocumentAnalysis,
    item_id: ItemId,
) -> Option<Value> {
    let (file_id, span) = analysis.item_definitions.get(&item_id).copied()?;
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

fn completion_kind_for_item(kind: core_x::frontend::resolver::ItemKind) -> i32 {
    match kind {
        core_x::frontend::resolver::ItemKind::Scope => 9,
        core_x::frontend::resolver::ItemKind::Function => 3,
        core_x::frontend::resolver::ItemKind::Struct => 22,
        core_x::frontend::resolver::ItemKind::Enum => 13,
        core_x::frontend::resolver::ItemKind::Protocol => 8,
    }
}

fn item_kind_label(kind: core_x::frontend::resolver::ItemKind) -> &'static str {
    match kind {
        core_x::frontend::resolver::ItemKind::Scope => "scope",
        core_x::frontend::resolver::ItemKind::Function => "function",
        core_x::frontend::resolver::ItemKind::Struct => "struct",
        core_x::frontend::resolver::ItemKind::Enum => "enum",
        core_x::frontend::resolver::ItemKind::Protocol => "protocol",
    }
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
