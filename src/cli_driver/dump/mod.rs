mod builder;
mod formatter;
mod model;

use crate::cli_driver::DynError;
use crate::cli_driver::diagnostics::{
    emit_context_diagnostics, emit_diagnostics_bag, emit_file_diagnostics,
};
use crate::cli_driver::dump::builder::{
    build_desugared_dumps, build_expanded_dumps, build_hir_dumps,
    build_parsed_dumps, build_pipeline_dump, build_resolved_dumps,
    build_typed_dumps, canonical_pipeline_stage_order,
    load_canonical_dump_context, normalize_canonical_stage_list,
};
use crate::cli_driver::dump::formatter::{
    diagnostics_to_json, format_ast_text, format_expanded_text,
    format_imports_text, format_parsed_text, format_pipeline_text,
    format_resolved_text, format_scopes_text, format_semantic_text,
    format_tokens_text, format_typed_text,
};
use crate::cli_driver::dump::model::{
    FileAstDump, FileDesugaredDump, FileExpandedDump, FileHirDump,
    FileParsedDump, FilePipelineDump, FileResolvedDump, FileTokenDump,
    FileTypedDump, ResolvedImportDump, ResolvedScopeDump, ResolvedSemanticDump,
    TokenView,
};
use crate::cli_driver::project::{
    classify_single_root_target, load_project_context, parse_single_file,
    parsed_by_id, path_for_file_id, single_target_from_context,
    targets_from_context,
};
use clap::{Args, ValueEnum};
use core_x::frontend::lexer::Lexer;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Args)]
pub(crate) struct DumpArgs {
    /// Dump kind to emit from the frontend pipeline.
    #[arg(conflicts_with = "stages")]
    kind: Option<DumpKind>,

    /// Single source file path (mutually exclusive with `--project`).
    #[arg(conflicts_with = "project")]
    path: Option<std::path::PathBuf>,

    /// Project directory root (mutually exclusive with `<path>`).
    #[arg(long, conflicts_with = "path")]
    project: Option<std::path::PathBuf>,

    /// Comma-separated list of stages to dump (alternative to <kind>).
    /// Stages: parsed, expanded, desugared, hir, resolved, typed
    /// Use 'all' to dump all stages.
    #[arg(long, value_delimiter = ',')]
    stages: Option<Vec<DumpKind>>,

    /// Output format for emitted dump payload.
    #[arg(long, value_enum, default_value_t = DumpFormat::Text)]
    format: DumpFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub(crate) enum DumpKind {
    Tokens,
    Ast,
    Parsed,
    Expanded,
    Desugared,
    Hir,
    Scopes,
    Imports,
    Resolved,
    Typed,
    Inferred,
    Semantic,
    Pipeline,
}

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub(crate) enum DumpFormat {
    Text,
    Json,
}

pub fn run_dump(args: DumpArgs) -> Result<(), DynError> {
    let input = parse_dump_input(args.path, args.project)?;

    // Determine which stages to dump
    let stages = if let Some(kind) = args.kind {
        // Single kind specified
        vec![kind]
    } else if let Some(stage_list) = args.stages {
        // Multiple stages specified
        stage_list
    } else {
        return Err("specify either <kind> or --stages".into());
    };

    let output = if are_canonical_pipeline_stages(&stages) {
        dump_canonical_stages(input, args.format, &stages)?
    } else if stages.len() == 1 {
        dump_single_stage(input, stages[0], args.format)?
    } else {
        dump_multiple_stages(input, args.format, &stages)?
    };

    println!("{output}");
    Ok(())
}

#[derive(Clone)]
pub(crate) enum DumpInput {
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

fn are_canonical_pipeline_stages(stages: &[DumpKind]) -> bool {
    stages.iter().all(|stage| {
        matches!(
            stage,
            DumpKind::Parsed
                | DumpKind::Expanded
                | DumpKind::Desugared
                | DumpKind::Hir
                | DumpKind::Resolved
                | DumpKind::Typed
                | DumpKind::Inferred
                | DumpKind::Pipeline
        )
    })
}

fn dump_canonical_stages(
    input: DumpInput,
    format: DumpFormat,
    requested_stages: &[DumpKind],
) -> Result<String, DynError> {
    let pipeline_requested = requested_stages
        .iter()
        .any(|stage| *stage == DumpKind::Pipeline);
    let mut stages = normalize_canonical_stage_list(requested_stages);
    if stages.is_empty() {
        stages = canonical_pipeline_stage_order();
    }

    let context = load_canonical_dump_context(&input)?;
    emit_canonical_stage_diagnostics(&context, &stages);

    if stages.len() == 1 && !pipeline_requested {
        let stage = stages[0];
        return match (stage, format) {
            (DumpKind::Parsed, DumpFormat::Text) => {
                Ok(format_parsed_text(&build_parsed_dumps(&context)?))
            }
            (DumpKind::Parsed, DumpFormat::Json) => {
                let files = build_parsed_dumps(&context)?;
                Ok(serde_json::to_string_pretty(&json!({
                    "kind": "parsed",
                    "mode": context.mode,
                    "files": files.iter().map(parsed_file_to_json).collect::<Vec<_>>(),
                }))?)
            }
            (DumpKind::Expanded, DumpFormat::Text) => {
                Ok(format_expanded_text(&build_expanded_dumps(&context)?))
            }
            (DumpKind::Expanded, DumpFormat::Json) => {
                let files = build_expanded_dumps(&context)?;
                Ok(serde_json::to_string_pretty(&json!({
                    "kind": "expanded",
                    "mode": context.mode,
                    "files": files.iter().map(expanded_file_to_json).collect::<Vec<_>>(),
                }))?)
            }
            (DumpKind::Desugared, DumpFormat::Text) => {
                Ok(crate::cli_driver::dump::formatter::format_desugared_text(
                    &build_desugared_dumps(&context)?,
                ))
            }
            (DumpKind::Desugared, DumpFormat::Json) => {
                let files = build_desugared_dumps(&context)?;
                Ok(serde_json::to_string_pretty(&json!({
                    "kind": "desugared",
                    "mode": context.mode,
                    "files": files.iter().map(desugared_file_to_json).collect::<Vec<_>>(),
                }))?)
            }
            (DumpKind::Hir, DumpFormat::Text) => {
                Ok(crate::cli_driver::dump::formatter::format_hir_text(
                    &build_hir_dumps(&context)?,
                ))
            }
            (DumpKind::Hir, DumpFormat::Json) => {
                let files = build_hir_dumps(&context)?;
                Ok(serde_json::to_string_pretty(&json!({
                    "kind": "hir",
                    "mode": context.mode,
                    "files": files.iter().map(hir_file_to_json).collect::<Vec<_>>(),
                }))?)
            }
            (DumpKind::Resolved, DumpFormat::Text) => {
                Ok(format_resolved_text(&build_resolved_dumps(&context)?))
            }
            (DumpKind::Resolved, DumpFormat::Json) => {
                let files = build_resolved_dumps(&context)?;
                Ok(serde_json::to_string_pretty(&json!({
                    "kind": "resolved",
                    "mode": context.mode,
                    "files": files.iter().map(resolved_file_to_json).collect::<Vec<_>>(),
                }))?)
            }
            (DumpKind::Typed, DumpFormat::Text) => {
                Ok(format_typed_text(&build_typed_dumps(&context)?))
            }
            (DumpKind::Typed, DumpFormat::Json) => {
                let files = build_typed_dumps(&context)?;
                Ok(serde_json::to_string_pretty(&json!({
                    "kind": "typed",
                    "mode": context.mode,
                    "files": files.iter().map(typed_file_to_json).collect::<Vec<_>>(),
                }))?)
            }
            _ => Err("unsupported canonical stage".into()),
        };
    }

    let pipeline = build_pipeline_dump(&context, &stages)?;
    let stage_labels = stages.iter().map(stage_label).collect::<Vec<_>>();

    match format {
        DumpFormat::Text => {
            Ok(format_pipeline_text(&pipeline.files, &stage_labels))
        }
        DumpFormat::Json => {
            let kind = if pipeline_requested {
                "pipeline"
            } else {
                "stages"
            };
            let files = pipeline
                .files
                .iter()
                .map(|file| pipeline_file_to_json(file, &stages))
                .collect::<Vec<_>>();
            Ok(serde_json::to_string_pretty(&json!({
                "kind": kind,
                "mode": context.mode,
                "stages": stage_labels,
                "files": files,
            }))?)
        }
    }
}

fn stage_label(stage: &DumpKind) -> &'static str {
    match stage {
        DumpKind::Parsed => "parsed",
        DumpKind::Expanded => "expanded",
        DumpKind::Desugared => "desugared",
        DumpKind::Hir => "hir",
        DumpKind::Resolved => "resolved",
        DumpKind::Typed | DumpKind::Inferred => "typed",
        DumpKind::Pipeline => "pipeline",
        DumpKind::Tokens => "tokens",
        DumpKind::Ast => "ast",
        DumpKind::Scopes => "scopes",
        DumpKind::Imports => "imports",
        DumpKind::Semantic => "semantic",
    }
}

fn emit_canonical_stage_diagnostics(
    context: &crate::cli_driver::dump::builder::CanonicalDumpContext,
    stages: &[DumpKind],
) {
    if stages
        .iter()
        .any(|stage| matches!(stage, DumpKind::Typed | DumpKind::Inferred))
    {
        emit_diagnostics_bag(&context.db, &context.analysis.diagnostics);
        return;
    }

    if stages.iter().any(|stage| {
        matches!(
            stage,
            DumpKind::Resolved | DumpKind::Hir | DumpKind::Desugared
        )
    }) {
        for desugared in &context.analysis.desugared {
            emit_diagnostics_bag(&context.db, &desugared.diagnostics);
        }
        return;
    }

    if stages.iter().any(|stage| *stage == DumpKind::Expanded) {
        for expanded in &context.analysis.expanded {
            emit_diagnostics_bag(&context.db, &expanded.diagnostics);
        }
        return;
    }

    if stages.iter().any(|stage| *stage == DumpKind::Parsed) {
        for parsed in &context.analysis.parsed {
            emit_diagnostics_bag(&context.db, &parsed.diagnostics);
        }
    }
}

fn parsed_file_to_json(file: &FileParsedDump) -> Value {
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
}

fn expanded_file_to_json(file: &FileExpandedDump) -> Value {
    json!({
        "file_id": file.file_id.raw(),
        "path": file.path,
        "item_count": file.item_count,
        "diagnostics_count": file.diagnostics_count,
        "provenance_summary": file.provenance_summary,
        "provenance": file.provenance_summary_json,
        "expanded": {
            "file_id": file.file_id.raw(),
            "ast": file.ast_json,
            "diagnostics": file.diagnostics_json,
        },
    })
}

fn desugared_file_to_json(file: &FileDesugaredDump) -> Value {
    json!({
        "file_id": file.file_id.raw(),
        "path": file.path,
        "item_count": file.item_count,
        "diagnostics_count": file.diagnostics_count,
        "normalized_forms_summary": file.normalized_forms_summary,
        "normalized_forms": file.normalized_forms_json,
        "desugared": {
            "file_id": file.file_id.raw(),
            "ast": file.ast_json,
            "diagnostics": file.diagnostics_json,
        },
    })
}

fn hir_file_to_json(file: &FileHirDump) -> Value {
    json!({
        "file_id": file.file_id.raw(),
        "path": file.path,
        "root_items_count": file.root_items_count,
        "bodies_count": file.bodies_count,
        "exprs_count": file.exprs_count,
        "stmts_count": file.stmts_count,
        "types_count": file.types_count,
        "patterns_count": file.patterns_count,
        "diagnostics_count": file.diagnostics_count,
        "hir": {
            "file_structure": file.file_structure_json,
            "items": file.items_json,
            "bodies": file.bodies_json,
            "expr_table": file.expr_table_json,
            "stmt_table": file.stmt_table_json,
            "type_table": file.type_table_json,
            "pattern_table": file.pattern_table_json,
            "origin_summary": file.origin_summary_json,
            "diagnostics": file.diagnostics_json,
        },
    })
}

fn resolved_file_to_json(file: &FileResolvedDump) -> Value {
    json!({
        "file_id": file.file_id.raw(),
        "path": file.path,
        "global_items_count": file.global_items_count,
        "local_bindings_count": file.local_bindings_count,
        "path_resolutions_count": file.path_resolutions_count,
        "import_bindings_count": file.import_bindings_count,
        "associated_member_resolutions_count": file.associated_member_resolutions_count,
        "resolved_bodies_count": file.resolved_bodies_count,
        "diagnostics_count": file.diagnostics_count,
        "resolved": {
            "item_table": file.item_table_json,
            "local_bindings": file.local_bindings_json,
            "path_resolutions": file.path_resolutions_json,
            "import_bindings": file.import_bindings_json,
            "named_root_resolutions": file.named_root_resolutions_json,
            "scope_symbols": file.scope_symbols_json,
            "associated_member_resolutions": file.associated_member_resolutions_json,
            "diagnostics": file.diagnostics_json,
        },
    })
}

fn typed_file_to_json(file: &FileTypedDump) -> Value {
    json!({
        "file_id": file.file_id.raw(),
        "path": file.path,
        "typed_items_count": file.typed_items_count,
        "typed_impls_count": file.typed_impls_count,
        "expr_types_count": file.expr_types_count,
        "local_types_count": file.local_types_count,
        "selected_call_targets_count": file.selected_call_targets_count,
        "diagnostics_count": file.diagnostics_count,
        "typed": {
            "typed_signatures": file.typed_signatures_json,
            "inferred_expr_types": file.inferred_expr_types_json,
            "inferred_local_types": file.inferred_local_types_json,
            "selected_call_targets": file.call_targets_json,
            "diagnostics": file.diagnostics_json,
        },
    })
}

fn pipeline_file_to_json(
    file: &FilePipelineDump,
    stages: &[DumpKind],
) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("file_id".to_string(), json!(file.file_id.raw()));
    object.insert("path".to_string(), json!(file.path));

    for stage in stages {
        match stage {
            DumpKind::Parsed => {
                if let Some(parsed) = &file.parsed {
                    object.insert(
                        "parsed".to_string(),
                        parsed_file_to_json(parsed),
                    );
                }
            }
            DumpKind::Expanded => {
                if let Some(expanded) = &file.expanded {
                    object.insert(
                        "expanded".to_string(),
                        expanded_file_to_json(expanded),
                    );
                }
            }
            DumpKind::Desugared => {
                if let Some(desugared) = &file.desugared {
                    object.insert(
                        "desugared".to_string(),
                        desugared_file_to_json(desugared),
                    );
                }
            }
            DumpKind::Hir => {
                if let Some(hir) = &file.hir {
                    object.insert("hir".to_string(), hir_file_to_json(hir));
                }
            }
            DumpKind::Resolved => {
                if let Some(resolved) = &file.resolved {
                    object.insert(
                        "resolved".to_string(),
                        resolved_file_to_json(resolved),
                    );
                }
            }
            DumpKind::Typed | DumpKind::Inferred => {
                if let Some(typed) = &file.typed {
                    object
                        .insert("typed".to_string(), typed_file_to_json(typed));
                }
            }
            DumpKind::Pipeline => {}
            _ => {}
        }
    }

    Value::Object(object)
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
    for target in targets {
        let graph = context
            .analysis
            .resolution_tables
            .get(&target.root_file_id)
            .map(|resolution| resolution.graph.clone())
            .ok_or_else(|| {
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
    for target in targets {
        let resolution = context
            .analysis
            .resolution_tables
            .get(&target.root_file_id)
            .ok_or_else(|| {
                format!(
                    "failed to build {} import tables for {}",
                    target.label,
                    path_for_file_id(&context, target.root_file_id)
                )
            })?;

        resolved.push(ResolvedImportDump {
            target,
            graph: resolution.graph.clone(),
            symbols: resolution.symbols.clone(),
            imports: resolution.imports.clone(),
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

    emit_diagnostics_bag(&context.db, &context.analysis.diagnostics);
    let mut resolved = Vec::new();
    for target in targets {
        let resolution = context
            .analysis
            .resolution_tables
            .get(&target.root_file_id)
            .ok_or_else(|| {
                format!(
                    "failed to build {} semantic tables for {}",
                    target.label,
                    path_for_file_id(&context, target.root_file_id)
                )
            })?;
        let semantic = context
            .analysis
            .semantic_tables
            .get(&target.root_file_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "missing semantic tables for {} ({})",
                    target.label,
                    path_for_file_id(&context, target.root_file_id)
                )
            })?;
        let inference = context
            .analysis
            .inference_tables
            .get(&target.root_file_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "missing inference tables for {} ({})",
                    target.label,
                    path_for_file_id(&context, target.root_file_id)
                )
            })?;

        resolved.push(ResolvedSemanticDump {
            target,
            graph: resolution.graph.clone(),
            symbols: resolution.symbols.clone(),
            imports: resolution.imports.clone(),
            semantic,
            inference,
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
                        "inference_summary": {
                            "expr_types": item.inference.expr_type_count(),
                            "local_types": item.inference.inferred_hir_local_count(),
                            "root_types": item.inference.root_type_count(),
                            "selected_call_targets": item.inference.call_target_count(),
                            "inference_diagnostics": item.inference.issues.len(),
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

// Implementations for new dump kinds

fn dump_expanded(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    dump_canonical_stages(input, format, &[DumpKind::Expanded])
}

fn dump_desugared(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    dump_canonical_stages(input, format, &[DumpKind::Desugared])
}

fn dump_hir(input: DumpInput, format: DumpFormat) -> Result<String, DynError> {
    dump_canonical_stages(input, format, &[DumpKind::Hir])
}

fn dump_resolved(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    dump_canonical_stages(input, format, &[DumpKind::Resolved])
}

fn dump_typed(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    dump_canonical_stages(input, format, &[DumpKind::Typed])
}

/// Dump a single stage.
fn dump_single_stage(
    input: DumpInput,
    kind: DumpKind,
    format: DumpFormat,
) -> Result<String, DynError> {
    match kind {
        DumpKind::Tokens => dump_tokens(input, format),
        DumpKind::Ast => dump_ast(input, format),
        DumpKind::Parsed => {
            dump_canonical_stages(input, format, &[DumpKind::Parsed])
        }
        DumpKind::Expanded => dump_expanded(input, format),
        DumpKind::Desugared => dump_desugared(input, format),
        DumpKind::Hir => dump_hir(input, format),
        DumpKind::Scopes => dump_scopes(input, format),
        DumpKind::Imports => dump_imports(input, format),
        DumpKind::Resolved => dump_resolved(input, format),
        DumpKind::Typed | DumpKind::Inferred => dump_typed(input, format),
        DumpKind::Semantic => dump_semantic(input, format),
        DumpKind::Pipeline => {
            dump_canonical_stages(input, format, &[DumpKind::Pipeline])
        }
    }
}

/// Dump multiple stages and combine output.
fn dump_multiple_stages(
    input: DumpInput,
    format: DumpFormat,
    stages: &[DumpKind],
) -> Result<String, DynError> {
    if format == DumpFormat::Json {
        dump_multiple_stages_json(input, stages)
    } else {
        dump_multiple_stages_text(input, stages)
    }
}

/// Dump multiple stages in JSON format.
fn dump_multiple_stages_json(
    input: DumpInput,
    stages: &[DumpKind],
) -> Result<String, DynError> {
    let mut stage_outputs = Vec::new();

    for stage in stages {
        let output = match stage {
            DumpKind::Tokens => dump_tokens(input.clone(), DumpFormat::Json)?,
            DumpKind::Ast => dump_ast(input.clone(), DumpFormat::Json)?,
            DumpKind::Parsed => dump_parsed(input.clone(), DumpFormat::Json)?,
            DumpKind::Expanded => {
                dump_expanded(input.clone(), DumpFormat::Json)?
            }
            DumpKind::Desugared => {
                dump_desugared(input.clone(), DumpFormat::Json)?
            }
            DumpKind::Hir => dump_hir(input.clone(), DumpFormat::Json)?,
            DumpKind::Scopes => dump_scopes(input.clone(), DumpFormat::Json)?,
            DumpKind::Imports => dump_imports(input.clone(), DumpFormat::Json)?,
            DumpKind::Resolved => {
                dump_resolved(input.clone(), DumpFormat::Json)?
            }
            DumpKind::Typed | DumpKind::Inferred => {
                dump_typed(input.clone(), DumpFormat::Json)?
            }
            DumpKind::Semantic => {
                dump_semantic(input.clone(), DumpFormat::Json)?
            }
            DumpKind::Pipeline => {
                return Err(
                    "pipeline should be expanded before this point".into()
                );
            }
        };

        // Parse the JSON output and extract the stage data
        let value: serde_json::Value = serde_json::from_str(&output)?;
        stage_outputs.push(value);
    }

    // Combine all stages into a single JSON output
    let combined = json!({
        "kind": "multiple_stages",
        "stages": stage_outputs,
    });

    Ok(serde_json::to_string_pretty(&combined)?)
}

/// Dump multiple stages in text format.
fn dump_multiple_stages_text(
    input: DumpInput,
    stages: &[DumpKind],
) -> Result<String, DynError> {
    let mut outputs = Vec::new();

    for (index, stage) in stages.iter().enumerate() {
        let output = match stage {
            DumpKind::Tokens => dump_tokens(input.clone(), DumpFormat::Text)?,
            DumpKind::Ast => dump_ast(input.clone(), DumpFormat::Text)?,
            DumpKind::Parsed => dump_parsed(input.clone(), DumpFormat::Text)?,
            DumpKind::Expanded => {
                dump_expanded(input.clone(), DumpFormat::Text)?
            }
            DumpKind::Desugared => {
                dump_desugared(input.clone(), DumpFormat::Text)?
            }
            DumpKind::Hir => dump_hir(input.clone(), DumpFormat::Text)?,
            DumpKind::Scopes => dump_scopes(input.clone(), DumpFormat::Text)?,
            DumpKind::Imports => dump_imports(input.clone(), DumpFormat::Text)?,
            DumpKind::Resolved => {
                dump_resolved(input.clone(), DumpFormat::Text)?
            }
            DumpKind::Typed | DumpKind::Inferred => {
                dump_typed(input.clone(), DumpFormat::Text)?
            }
            DumpKind::Semantic => {
                dump_semantic(input.clone(), DumpFormat::Text)?
            }
            DumpKind::Pipeline => {
                return Err(
                    "pipeline should be expanded before this point".into()
                );
            }
        };

        if index > 0 {
            outputs.push("\n\n====================\n\n".to_string());
        }
        outputs.push(output);
    }

    Ok(outputs.concat())
}
