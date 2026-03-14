mod formatter;
mod model;

use crate::cli_driver::DynError;
use crate::cli_driver::diagnostics::{
    emit_context_diagnostics, emit_diagnostics_bag, emit_file_diagnostics,
};
use crate::cli_driver::dump::formatter::{
    diagnostics_to_json, format_ast_text, format_imports_text,
    format_parsed_text, format_scopes_text, format_semantic_text,
    format_tokens_text,
};
use crate::cli_driver::dump::model::{
    FileAstDump, FileParsedDump, FileTokenDump, ResolvedImportDump,
    ResolvedScopeDump, ResolvedSemanticDump, TokenView,
};
use crate::cli_driver::project::{
    classify_single_root_target, load_project_context, parse_single_file,
    parsed_by_id, path_for_file_id,
    resolve_target_scope_graph_with_diagnostics, single_target_from_context,
    targets_from_context,
};
use clap::{Args, ValueEnum};
use core_x::frontend::NamedImportRoot;
use core_x::frontend::lexer::Lexer;
use core_x::frontend::resolver::{
    ResolvedScopeKind, resolve_project_imports_with_named_roots_and_diagnostics,
};
use core_x::frontend::{
    analyze_semantics_with_external_lookup, build_external_semantic_lookup,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub(crate) struct DumpArgs {
    /// Dump kind to emit from the frontend pipeline.
    kind: DumpKind,
    /// Single source file path (mutually exclusive with `--project`).
    path: Option<std::path::PathBuf>,
    /// Project directory root (mutually exclusive with `<path>`).
    #[arg(long)]
    project: Option<std::path::PathBuf>,
    /// Output format for emitted dump payload.
    #[arg(long, value_enum, default_value_t = DumpFormat::Text)]
    format: DumpFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum DumpKind {
    Tokens,
    Ast,
    Scopes,
    Imports,
    Semantic,
    Parsed,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum DumpFormat {
    Text,
    Json,
}

pub fn run_dump(args: DumpArgs) -> Result<(), DynError> {
    let input = parse_dump_input(args.path, args.project)?;
    let output = match args.kind {
        DumpKind::Tokens => dump_tokens(input, args.format)?,
        DumpKind::Ast => dump_ast(input, args.format)?,
        DumpKind::Parsed => dump_parsed(input, args.format)?,
        DumpKind::Scopes => dump_scopes(input, args.format)?,
        DumpKind::Imports => dump_imports(input, args.format)?,
        DumpKind::Semantic => dump_semantic(input, args.format)?,
    };

    println!("{output}");
    Ok(())
}

enum DumpInput {
    File(PathBuf),
    Project(PathBuf),
}

fn parse_dump_input(
    path: Option<PathBuf>,
    project: Option<PathBuf>,
) -> Result<DumpInput, DynError> {
    match (path, project) {
        (Some(_), Some(_)) => {
            Err("provide either <path> or --project <dir>, not both".into())
        }
        (None, None) => Err("provide either <path> or --project <dir>".into()),
        (Some(path), None) => Ok(DumpInput::File(path)),
        (None, Some(project)) => Ok(DumpInput::Project(project)),
    }
}

fn dump_tokens(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    let (files, mode) = match input {
        DumpInput::File(path) => {
            (vec![dump_tokens_for_file_path(&path)?], "file")
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let mut files = Vec::new();
            for file_id in &context.ordered_file_ids {
                let source_file =
                    context.db.file(*file_id).ok_or_else(|| {
                        format!("missing source file id {}", file_id.raw())
                    })?;
                files.push(FileTokenDump {
                    file_id: *file_id,
                    path: path_for_file_id(&context, *file_id),
                    tokens: lex_token_views(source_file.source())?,
                });
            }
            (files, "project")
        }
    };

    match format {
        DumpFormat::Text => Ok(format_tokens_text(&files)),
        DumpFormat::Json => Ok(serde_json::to_string_pretty(&json!({
            "kind": "tokens",
            "mode": mode,
            "files": files.iter().map(|file| {
                json!({
                    "file_id": file.file_id.raw(),
                    "path": file.path,
                    "tokens": file.tokens.iter().map(|token| {
                        json!({
                            "kind": token.kind,
                            "start": token.start,
                            "end": token.end,
                            "text": token.text,
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
        }))?),
    }
}

fn dump_ast(input: DumpInput, format: DumpFormat) -> Result<String, DynError> {
    let (files, mode) = match input {
        DumpInput::File(path) => {
            let (db, parsed, file_id) = parse_single_file(&path)?;
            emit_file_diagnostics(&db, &parsed);
            let source_file = db.file(file_id).ok_or_else(|| {
                format!("missing source file id {}", file_id.raw())
            })?;
            (
                vec![FileAstDump {
                    file_id,
                    path: source_file.path().display().to_string(),
                    item_count: parsed.ast.items.len(),
                    ast_debug: format!("{:#?}", parsed.ast),
                    diagnostics_count: parsed.diagnostics.len(),
                    ast_json: serde_json::to_value(&parsed.ast).map_err(
                        |error| format!("failed to encode AST JSON: {error}"),
                    )?,
                }],
                "file",
            )
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let parsed_by_id = parsed_by_id(&context.parsed_files);
            let mut files = Vec::new();

            for file_id in &context.ordered_file_ids {
                let parsed = parsed_by_id.get(file_id).ok_or_else(|| {
                    format!("missing parsed file id {}", file_id.raw())
                })?;
                files.push(FileAstDump {
                    file_id: *file_id,
                    path: path_for_file_id(&context, *file_id),
                    item_count: parsed.ast.items.len(),
                    ast_debug: format!("{:#?}", parsed.ast),
                    diagnostics_count: parsed.diagnostics.len(),
                    ast_json: serde_json::to_value(&parsed.ast).map_err(
                        |error| format!("failed to encode AST JSON: {error}"),
                    )?,
                });
            }

            (files, "project")
        }
    };

    match format {
        DumpFormat::Text => Ok(format_ast_text(&files)),
        DumpFormat::Json => Ok(serde_json::to_string_pretty(&json!({
            "kind": "ast",
            "mode": mode,
            "files": files.iter().map(|file| {
                json!({
                    "file_id": file.file_id.raw(),
                    "path": file.path,
                    "item_count": file.item_count,
                    "diagnostics_count": file.diagnostics_count,
                    "ast": file.ast_json,
                })
            }).collect::<Vec<_>>(),
        }))?),
    }
}

fn dump_parsed(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    let (files, mode) = match input {
        DumpInput::File(path) => {
            let (db, parsed, file_id) = parse_single_file(&path)?;
            emit_file_diagnostics(&db, &parsed);
            let source_file = db.file(file_id).ok_or_else(|| {
                format!("missing source file id {}", file_id.raw())
            })?;
            (
                vec![FileParsedDump {
                    file_id,
                    path: source_file.path().display().to_string(),
                    item_count: parsed.ast.items.len(),
                    diagnostics_count: parsed.diagnostics.len(),
                    parsed_debug: format!("{parsed:#?}"),
                    ast_json: serde_json::to_value(&parsed.ast).map_err(
                        |error| {
                            format!("failed to encode parsed AST JSON: {error}")
                        },
                    )?,
                    diagnostics_json: diagnostics_to_json(&parsed.diagnostics),
                }],
                "file",
            )
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let parsed_by_id = parsed_by_id(&context.parsed_files);
            let mut files = Vec::new();

            for file_id in &context.ordered_file_ids {
                let parsed = parsed_by_id.get(file_id).ok_or_else(|| {
                    format!("missing parsed file id {}", file_id.raw())
                })?;
                files.push(FileParsedDump {
                    file_id: *file_id,
                    path: path_for_file_id(&context, *file_id),
                    item_count: parsed.ast.items.len(),
                    diagnostics_count: parsed.diagnostics.len(),
                    parsed_debug: format!("{parsed:#?}"),
                    ast_json: serde_json::to_value(&parsed.ast).map_err(
                        |error| {
                            format!("failed to encode parsed AST JSON: {error}")
                        },
                    )?,
                    diagnostics_json: diagnostics_to_json(&parsed.diagnostics),
                });
            }

            (files, "project")
        }
    };

    match format {
        DumpFormat::Text => Ok(format_parsed_text(&files)),
        DumpFormat::Json => Ok(serde_json::to_string_pretty(&json!({
            "kind": "parsed",
            "mode": mode,
            "files": files.iter().map(|file| {
                json!({
                    "file_id": file.file_id.raw(),
                    "path": file.path,
                    "item_count": file.item_count,
                    "diagnostics_count": file.diagnostics_count,
                    "parsed": {
                        "file_id": file.file_id.raw(),
                        "ast": file.ast_json,
                        "diagnostics": file.diagnostics_json,
                    },
                })
            }).collect::<Vec<_>>(),
        }))?),
    }
}

fn dump_scopes(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    let (context, targets, mode) = match input {
        DumpInput::File(path) => {
            let (project_dir, root_kind) = classify_single_root_target(&path)?;
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let target = single_target_from_context(&context, root_kind)?;
            (context, vec![target], "file")
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let targets = targets_from_context(&context)?;
            (context, targets, "project")
        }
    };

    let mut resolved = Vec::new();
    let scope_resolver = core_x::frontend::ScopeResolver::new(
        &context.db,
        &context.parsed_files,
    );
    for target in targets {
        let (graph, scope_diagnostics) =
            resolve_target_scope_graph_with_diagnostics(
                &scope_resolver,
                &context.db,
                &context.parsed_files,
                target.root_file_id,
                target.kind,
            );
        emit_diagnostics_bag(&context.db, &scope_diagnostics);
        let graph = graph.ok_or_else(|| {
            format!(
                "failed to build {} scope graph for {}",
                target.label,
                path_for_file_id(&context, target.root_file_id)
            )
        })?;
        resolved.push(ResolvedScopeDump { target, graph });
    }

    match format {
        DumpFormat::Text => Ok(format_scopes_text(&context, &resolved)),
        DumpFormat::Json => {
            let targets_json = resolved
                .iter()
                .map(|item| {
                    let root_path =
                        path_for_file_id(&context, item.target.root_file_id);

                    json!({
                        "target_kind": item.target.label,
                        "root_file_id": item.target.root_file_id.raw(),
                        "root_path": root_path,
                        "scope_graph_debug": format!("{:#?}", item.graph),
                    })
                })
                .collect::<Vec<_>>();

            Ok(serde_json::to_string_pretty(&json!({
                "kind": "scopes",
                "mode": mode,
                "targets": targets_json,
            }))?)
        }
    }
}

fn dump_imports(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    let (context, targets, mode) = match input {
        DumpInput::File(path) => {
            let (project_dir, root_kind) = classify_single_root_target(&path)?;
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let target = single_target_from_context(&context, root_kind)?;
            (context, vec![target], "file")
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let targets = targets_from_context(&context)?;
            (context, targets, "project")
        }
    };

    let mut resolved = Vec::new();
    let scope_resolver = core_x::frontend::ScopeResolver::new(
        &context.db,
        &context.parsed_files,
    );
    for target in targets {
        let (graph, scope_diagnostics) =
            resolve_target_scope_graph_with_diagnostics(
                &scope_resolver,
                &context.db,
                &context.parsed_files,
                target.root_file_id,
                target.kind,
            );
        emit_diagnostics_bag(&context.db, &scope_diagnostics);
        let graph = graph.ok_or_else(|| {
            format!(
                "failed to build {} scope graph for {}",
                target.label,
                path_for_file_id(&context, target.root_file_id)
            )
        })?;

        let mut named_roots = context.dependency_named_roots.clone();
        maybe_add_current_library_root_for_binary(
            &context,
            &scope_resolver,
            &target,
            &mut named_roots,
        )?;

        let (symbols, imports, import_diagnostics) =
            resolve_project_imports_with_named_roots_and_diagnostics(
                &graph,
                &context.parsed_files,
                &named_roots,
                &context.db,
            );
        emit_diagnostics_bag(&context.db, &import_diagnostics);

        resolved.push(ResolvedImportDump {
            target,
            graph,
            symbols,
            imports,
        });
    }

    match format {
        DumpFormat::Text => Ok(format_imports_text(&context, &resolved)),
        DumpFormat::Json => {
            let targets_json = resolved
                .iter()
                .map(|item| {
                    let root_path =
                        path_for_file_id(&context, item.target.root_file_id);

                    json!({
                        "target_kind": item.target.label,
                        "root_file_id": item.target.root_file_id.raw(),
                        "root_path": root_path,
                        "scope_graph_debug": format!("{:#?}", item.graph),
                        "scope_symbols_debug": format!("{:#?}", item.symbols),
                        "resolved_imports_debug": format!("{:#?}", item.imports),
                    })
                })
                .collect::<Vec<_>>();

            Ok(serde_json::to_string_pretty(&json!({
                "kind": "imports",
                "mode": mode,
                "targets": targets_json,
            }))?)
        }
    }
}

fn dump_semantic(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    let (context, targets, mode) = match input {
        DumpInput::File(path) => {
            let (project_dir, root_kind) = classify_single_root_target(&path)?;
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let target = single_target_from_context(&context, root_kind)?;
            (context, vec![target], "file")
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let targets = targets_from_context(&context)?;
            (context, targets, "project")
        }
    };

    let mut resolved = Vec::new();
    let scope_resolver = core_x::frontend::ScopeResolver::new(
        &context.db,
        &context.parsed_files,
    );
    for target in targets {
        let (graph, scope_diagnostics) =
            resolve_target_scope_graph_with_diagnostics(
                &scope_resolver,
                &context.db,
                &context.parsed_files,
                target.root_file_id,
                target.kind,
            );
        emit_diagnostics_bag(&context.db, &scope_diagnostics);
        let graph = graph.ok_or_else(|| {
            format!(
                "failed to build {} scope graph for {}",
                target.label,
                path_for_file_id(&context, target.root_file_id)
            )
        })?;

        let mut named_roots = context.dependency_named_roots.clone();
        maybe_add_current_library_root_for_binary(
            &context,
            &scope_resolver,
            &target,
            &mut named_roots,
        )?;

        let (symbols, imports, import_diagnostics) =
            resolve_project_imports_with_named_roots_and_diagnostics(
                &graph,
                &context.parsed_files,
                &named_roots,
                &context.db,
            );
        emit_diagnostics_bag(&context.db, &import_diagnostics);

        let external_lookup = build_external_semantic_lookup(
            &context.db,
            &named_roots,
            &graph,
            &context.parsed_files,
        );
        let semantic = analyze_semantics_with_external_lookup(
            &context.db,
            &graph,
            &context.parsed_files,
            &imports,
            &external_lookup,
        );
        emit_diagnostics_bag(&context.db, &semantic.diagnostics);

        resolved.push(ResolvedSemanticDump {
            target,
            graph,
            symbols,
            imports,
            semantic,
        });
    }

    match format {
        DumpFormat::Text => Ok(format_semantic_text(&context, &resolved)),
        DumpFormat::Json => {
            let targets_json = resolved
                .iter()
                .map(|item| {
                    let root_path =
                        path_for_file_id(&context, item.target.root_file_id);

                    json!({
                        "target_kind": item.target.label,
                        "root_file_id": item.target.root_file_id.raw(),
                        "root_path": root_path,
                        "scope_graph_debug": format!("{:#?}", item.graph),
                        "scope_symbols_debug": format!("{:#?}", item.symbols),
                        "resolved_imports_debug": format!("{:#?}", item.imports),
                        "semantic_summary": {
                            "global_items": item.semantic.global_items.len(),
                            "typed_items": item.semantic.typed_items.len(),
                            "typed_bodies": item.semantic.typed_bodies.len(),
                            "semantic_diagnostics": item.semantic.diagnostics.len(),
                        },
                    })
                })
                .collect::<Vec<_>>();

            Ok(serde_json::to_string_pretty(&json!({
                "kind": "semantic",
                "mode": mode,
                "targets": targets_json,
            }))?)
        }
    }
}

fn maybe_add_current_library_root_for_binary(
    context: &crate::cli_driver::project::ProjectContext,
    scope_resolver: &core_x::frontend::ScopeResolver<'_>,
    target: &crate::cli_driver::project::TargetSelection,
    named_roots: &mut BTreeMap<String, NamedImportRoot>,
) -> Result<(), DynError> {
    if target.kind != ResolvedScopeKind::BinaryRoot {
        return Ok(());
    }

    let (Some(root_name), Some(library_target)) = (
        context.current_library_import_root.as_ref(),
        context.library_target.as_ref(),
    ) else {
        return Ok(());
    };

    let (library_graph, library_diagnostics) =
        resolve_target_scope_graph_with_diagnostics(
            scope_resolver,
            &context.db,
            &context.parsed_files,
            library_target.root_file_id,
            ResolvedScopeKind::Root,
        );
    emit_diagnostics_bag(&context.db, &library_diagnostics);
    let library_graph = library_graph.ok_or_else(|| {
        format!(
            "failed to build library scope graph for {}",
            path_for_file_id(context, library_target.root_file_id)
        )
    })?;
    named_roots.insert(
        root_name.clone(),
        NamedImportRoot::LoadedLibrary {
            graph: library_graph,
            parsed_files: context.parsed_files.clone(),
            path_by_file_id: context.path_by_file_id.clone(),
        },
    );
    Ok(())
}

fn dump_tokens_for_file_path(path: &Path) -> Result<FileTokenDump, DynError> {
    let (db, parsed, file_id) = parse_single_file(path)?;
    emit_file_diagnostics(&db, &parsed);
    let source_file = db
        .file(file_id)
        .ok_or_else(|| format!("missing source file id {}", file_id.raw()))?;
    let tokens = lex_token_views(source_file.source())?;
    Ok(FileTokenDump {
        file_id,
        path: path.display().to_string(),
        tokens,
    })
}

fn lex_token_views(source: &str) -> Result<Vec<TokenView>, DynError> {
    let tokens = Lexer::new(source)
        .lex_all()
        .map_err(|error| format!("failed to lex source: {error}"))?;

    let mut rows = Vec::with_capacity(tokens.len());
    for token in tokens {
        let text = source
            .get(token.span.start..token.span.end)
            .unwrap_or("")
            .to_string();
        rows.push(TokenView {
            kind: format!("{:?}", token.kind),
            start: token.span.start,
            end: token.span.end,
            text,
        });
    }
    Ok(rows)
}
