use super::{DumpInput, DumpKind};
use crate::cli_driver::DynError;
use crate::cli_driver::dump::formatter::diagnostics_to_json;
use crate::cli_driver::dump::model::{
    FileDesugaredDump, FileExpandedDump, FileHirDump, FileParsedDump,
    FilePipelineDump, FileResolvedDump, FileTypedDump, PipelineDump,
};
use crate::cli_driver::project::load_project_context;
use core_x::frontend::expansion::{Provenance, ProvenanceMap};
use core_x::frontend::hir::{HirArrayElement, HirStructExprField};
use core_x::frontend::resolver::{AssociatedMemberKind, DeclarationOwner};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    BodyInferIssue, BodyInferIssueKind, FrontendAnalysis, FrontendContext,
    HirExprKind, HirItemKind, HirPatKind, HirPathResolution, HirStmtKind,
    HirTypeKind, InferredCallTarget, ItemKind, ParseSessionError, Type,
    TypedItemData, TypedParamLabel, analyze_project,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct CanonicalDumpContext {
    pub mode: &'static str,
    pub db: SourceDb,
    pub analysis: FrontendAnalysis,
    pub ordered_file_ids: Vec<FileId>,
    pub path_by_file_id: BTreeMap<FileId, PathBuf>,
}

pub(crate) fn load_canonical_dump_context(
    input: &DumpInput,
) -> Result<CanonicalDumpContext, DynError> {
    match input {
        DumpInput::File(path) => load_single_file_context(path),
        DumpInput::Project(project_dir) => {
            let project = load_project_context(project_dir)?;
            Ok(CanonicalDumpContext {
                mode: "project",
                db: project.db,
                analysis: project.analysis,
                ordered_file_ids: project.ordered_file_ids,
                path_by_file_id: project.path_by_file_id,
            })
        }
    }
}

fn load_single_file_context(
    path: &Path,
) -> Result<CanonicalDumpContext, DynError> {
    let source = fs::read_to_string(path)?;
    let mut frontend = FrontendContext::new();
    let file_id = frontend.add_file(path.to_path_buf(), source);
    let analysis =
        analyze_project(&mut frontend, &[file_id]).map_err(|error| {
            format_single_file_parse_session_error(path, &frontend, error)
        })?;

    let ordered_file_ids = frontend.ordered_file_ids().to_vec();
    let path_by_file_id = frontend.path_by_file_id().clone();
    let db = frontend.into_db();

    Ok(CanonicalDumpContext {
        mode: "file",
        db,
        analysis,
        ordered_file_ids,
        path_by_file_id,
    })
}

fn format_single_file_parse_session_error(
    path: &Path,
    frontend: &FrontendContext,
    error: ParseSessionError,
) -> DynError {
    match error {
        ParseSessionError::MissingFile { file_id } => {
            format!(
                "failed to run frontend canonical analysis for {}: missing source file id {}",
                path.display(),
                file_id.raw()
            )
            .into()
        }
        ParseSessionError::Parse(file_error) => {
            let display_path = frontend
                .path_for_file_id(file_error.file_id)
                .map_or_else(
                    || format!("<unknown:{}>", file_error.file_id.raw()),
                    |path| path.display().to_string(),
                );
            format!(
                "failed to run frontend canonical analysis for {}: {}",
                display_path, file_error.error
            )
            .into()
        }
    }
}

pub(crate) fn path_for_file_id(
    context: &CanonicalDumpContext,
    file_id: FileId,
) -> String {
    context.path_by_file_id.get(&file_id).map_or_else(
        || format!("<unknown:{}>", file_id.raw()),
        |path| path.display().to_string(),
    )
}

pub(crate) fn build_parsed_dumps(
    context: &CanonicalDumpContext,
) -> Result<Vec<FileParsedDump>, DynError> {
    let parsed_by_id = context
        .analysis
        .parsed
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect::<BTreeMap<_, _>>();

    let mut files = Vec::new();
    for file_id in &context.ordered_file_ids {
        let Some(parsed) = parsed_by_id.get(file_id) else {
            continue;
        };

        files.push(FileParsedDump {
            file_id: *file_id,
            path: path_for_file_id(context, *file_id),
            item_count: parsed.ast.items.len(),
            diagnostics_count: parsed.diagnostics.len(),
            parsed_debug: format!("{parsed:#?}"),
            ast_json: serde_json::to_value(&parsed.ast).map_err(|error| {
                format!("failed to encode parsed AST JSON: {error}")
            })?,
            diagnostics_json: diagnostics_to_json(&parsed.diagnostics),
        });
    }

    Ok(files)
}

pub(crate) fn build_expanded_dumps(
    context: &CanonicalDumpContext,
) -> Result<Vec<FileExpandedDump>, DynError> {
    let expanded_by_id = context
        .analysis
        .expanded
        .iter()
        .map(|expanded| (expanded.file_id, expanded))
        .collect::<BTreeMap<_, _>>();

    let mut files = Vec::new();
    for file_id in &context.ordered_file_ids {
        let Some(expanded) = expanded_by_id.get(file_id) else {
            continue;
        };
        let (provenance_summary, provenance_summary_json) =
            summarize_provenance_map(&expanded.provenance_map);

        files.push(FileExpandedDump {
            file_id: *file_id,
            path: path_for_file_id(context, *file_id),
            item_count: expanded.ast.items.len(),
            diagnostics_count: expanded.diagnostics.len(),
            expanded_debug: format!("{:#?}", expanded.ast),
            ast_json: serde_json::to_value(&expanded.ast).map_err(|error| {
                format!("failed to encode expanded AST JSON: {error}")
            })?,
            diagnostics_json: diagnostics_to_json(&expanded.diagnostics),
            provenance_summary,
            provenance_summary_json,
        });
    }

    Ok(files)
}

pub(crate) fn build_desugared_dumps(
    context: &CanonicalDumpContext,
) -> Result<Vec<FileDesugaredDump>, DynError> {
    let desugared_by_id = context
        .analysis
        .desugared
        .iter()
        .map(|desugared| (desugared.file_id, desugared))
        .collect::<BTreeMap<_, _>>();

    let mut files = Vec::new();
    for file_id in &context.ordered_file_ids {
        let Some(desugared) = desugared_by_id.get(file_id) else {
            continue;
        };

        let desugared_debug = format!("{:#?}", desugared.ast);
        let grouped_wrappers = desugared_debug.matches("Grouped(").count();
        let normalized_forms_summary = Some(format!(
            "grouped_wrapper_nodes={} (post-desugar)",
            grouped_wrappers
        ));
        let normalized_forms_json = json!({
            "grouped_wrapper_nodes": grouped_wrappers,
        });

        files.push(FileDesugaredDump {
            file_id: *file_id,
            path: path_for_file_id(context, *file_id),
            item_count: desugared.ast.items.len(),
            diagnostics_count: desugared.diagnostics.len(),
            desugared_debug,
            ast_json: serde_json::to_value(&desugared.ast).map_err(
                |error| format!("failed to encode desugared AST JSON: {error}"),
            )?,
            diagnostics_json: diagnostics_to_json(&desugared.diagnostics),
            normalized_forms_summary,
            normalized_forms_json,
        });
    }

    Ok(files)
}

pub(crate) fn build_hir_dumps(
    context: &CanonicalDumpContext,
) -> Result<Vec<FileHirDump>, DynError> {
    let semantic_root_by_file = semantic_root_by_file(context);

    let mut files = Vec::new();
    for file_id in &context.ordered_file_ids {
        let Some(root_file_id) = semantic_root_by_file.get(file_id).copied()
        else {
            continue;
        };
        let Some(semantic) =
            context.analysis.semantic_tables.get(&root_file_id)
        else {
            continue;
        };
        let Some(hir_file) = semantic
            .hir
            .hir_files
            .iter()
            .find(|hir_file| hir_file.file_id == *file_id)
        else {
            continue;
        };
        let Some(module) = semantic.hir.hir_modules.get(file_id) else {
            continue;
        };

        let root_items_json = hir_file
            .root_items
            .iter()
            .map(|item_id| {
                let item = module.items.get(item_id);
                json!({
                    "item_id": item_id.raw(),
                    "kind": item
                        .map(|item| hir_item_kind_name(&item.kind))
                        .unwrap_or("<missing>"),
                    "name": item.and_then(|item| hir_item_name(&item.kind)),
                    "origin": item
                        .map(|item| hir_origin_to_json(&item.origin))
                        .unwrap_or_else(|| json!(null)),
                })
            })
            .collect::<Vec<_>>();

        let items_json = module
            .items
            .iter()
            .map(|(item_id, item)| {
                json!({
                    "item_id": item_id.raw(),
                    "kind": hir_item_kind_name(&item.kind),
                    "name": hir_item_name(&item.kind),
                    "origin": hir_origin_to_json(&item.origin),
                    "detail": serialize_hir_item_kind(&item.kind),
                })
            })
            .collect::<Vec<_>>();

        let bodies_json = module
            .bodies
            .iter()
            .map(|(body_id, body)| {
                json!({
                    "body_id": body_id.raw(),
                    "origin": hir_origin_to_json(&body.origin),
                    "stmt_ids": body.stmts.iter().map(|stmt| stmt.raw()).collect::<Vec<_>>(),
                    "tail_expr": body.tail_expr.map(|tail| tail.raw()),
                })
            })
            .collect::<Vec<_>>();

        let expr_table_json = module
            .exprs
            .iter()
            .map(|(expr_id, expr)| {
                json!({
                    "expr_id": expr_id.raw(),
                    "origin": hir_origin_to_json(&expr.origin),
                    "kind": serialize_hir_expr_kind(&expr.kind),
                })
            })
            .collect::<Vec<_>>();

        let stmt_table_json = module
            .stmts
            .iter()
            .map(|(stmt_id, stmt)| {
                json!({
                    "stmt_id": stmt_id.raw(),
                    "origin": hir_origin_to_json(&stmt.origin),
                    "kind": serialize_hir_stmt_kind(&stmt.kind),
                })
            })
            .collect::<Vec<_>>();

        let type_table_json = module
            .types
            .iter()
            .map(|(type_id, ty)| {
                json!({
                    "type_id": type_id.raw(),
                    "origin": hir_origin_to_json(&ty.origin),
                    "kind": serialize_hir_type_kind(&ty.kind),
                })
            })
            .collect::<Vec<_>>();

        let pattern_table_json = module
            .patterns
            .iter()
            .map(|(pat_id, pat)| {
                json!({
                    "pattern_id": pat_id.raw(),
                    "origin": hir_origin_to_json(&pat.origin),
                    "kind": serialize_hir_pattern_kind(&pat.kind),
                })
            })
            .collect::<Vec<_>>();

        let origin_summary_json = summarize_hir_origins(module);

        files.push(FileHirDump {
            file_id: *file_id,
            path: path_for_file_id(context, *file_id),
            root_items_count: hir_file.root_items.len(),
            bodies_count: module.bodies.len(),
            exprs_count: module.exprs.len(),
            stmts_count: module.stmts.len(),
            types_count: module.types.len(),
            patterns_count: module.patterns.len(),
            hir_debug: format!(
                "HirFile {:#?}\nHirModule {:#?}",
                hir_file, module
            ),
            diagnostics_count: 0,
            diagnostics_json: Vec::new(),
            file_structure_json: json!({
                "file_id": file_id.raw(),
                "root_items": root_items_json,
            }),
            items_json,
            bodies_json,
            expr_table_json,
            stmt_table_json,
            type_table_json,
            pattern_table_json,
            origin_summary_json,
        });
    }

    Ok(files)
}

pub(crate) fn build_resolved_dumps(
    context: &CanonicalDumpContext,
) -> Result<Vec<FileResolvedDump>, DynError> {
    let semantic_root_by_file = semantic_root_by_file(context);

    let mut files = Vec::new();
    for file_id in &context.ordered_file_ids {
        let Some(root_file_id) = semantic_root_by_file.get(file_id).copied()
        else {
            continue;
        };
        let Some(semantic) =
            context.analysis.semantic_tables.get(&root_file_id)
        else {
            continue;
        };
        let Some(resolution) =
            context.analysis.resolution_tables.get(&root_file_id)
        else {
            continue;
        };

        let global_items = semantic
            .global_items
            .items_in_scope(*file_id)
            .into_iter()
            .map(|item| {
                json!({
                    "item_id": item.id.raw(),
                    "kind": item_kind_name(item.kind),
                    "name": item.name,
                    "defining_file_id": item.defining_file_id.raw(),
                    "containing_scope_file_id": item.containing_scope_file_id.raw(),
                    "scope_path": item.scope_path,
                    "full_path": item.full_path,
                })
            })
            .collect::<Vec<_>>();

        let hir_items = semantic
            .hir
            .hir_item_table
            .item_refs_in_file(*file_id)
            .iter()
            .filter_map(|item_ref| {
                semantic.hir.hir_item_table.get(*item_ref).map(|item| {
                    json!({
                        "file_id": item_ref.file_id.raw(),
                        "item_id": item_ref.item_id.raw(),
                        "name": item.name,
                        "kind": format!("{:?}", item.kind),
                    })
                })
            })
            .collect::<Vec<_>>();

        let item_table_json = json!({
            "global_items": global_items,
            "hir_items": hir_items,
        });

        let local_bindings_json = semantic
            .hir
            .hir_local_bindings
            .binding_ids_in_file(*file_id)
            .iter()
            .filter_map(|binding_id| {
                semantic.hir.hir_local_bindings.binding(*binding_id).map(
                    |binding| {
                        json!({
                            "local_id": binding.id.raw(),
                            "file_id": binding.file_id.raw(),
                            "body_id": binding.body_id.raw(),
                            "name": binding.name,
                            "kind": format!("{:?}", binding.kind),
                            "mutability": format!("{:?}", binding.mutability),
                            "declared_pat_id": binding.declared_pat.map(|pat| pat.raw()),
                        })
                    },
                )
            })
            .collect::<Vec<_>>();

        let module = semantic.hir.hir_modules.get(file_id);
        let path_resolutions_json = semantic
            .hir
            .hir_path_table
            .iter_expr()
            .filter(|(expr_ref, _)| expr_ref.file_id == *file_id)
            .map(|(expr_ref, resolution)| {
                let segments = module
                    .and_then(|module| {
                        namespace_segments_for_expr(module, expr_ref.expr_id)
                    })
                    .unwrap_or_default();
                json!({
                    "file_id": expr_ref.file_id.raw(),
                    "expr_id": expr_ref.expr_id.raw(),
                    "path_segments": segments,
                    "resolution": serialize_hir_path_resolution(resolution),
                })
            })
            .collect::<Vec<_>>();

        let associated_member_resolutions_json = path_resolutions_json
            .iter()
            .filter(|entry| {
                entry
                    .get("resolution")
                    .and_then(|resolution| resolution.get("kind"))
                    .and_then(Value::as_str)
                    == Some("associated_member")
            })
            .cloned()
            .collect::<Vec<_>>();

        let import_bindings_json = resolution
            .imports
            .get(file_id)
            .map(|imports| {
                imports
                    .bindings
                    .values()
                    .map(|binding| {
                        json!({
                            "local_name": binding.local_name,
                            "kind": serialize_import_binding_kind(&binding.kind),
                            "target_file_id": binding.target_file_id.raw(),
                            "target_path": binding.target_path,
                            "source_root": binding.source_root,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let named_root_resolutions_json = {
            let mut by_root = BTreeMap::<String, usize>::new();
            for binding in &import_bindings_json {
                let Some(root) = binding
                    .get("source_root")
                    .and_then(Value::as_str)
                    .filter(|root| !root.is_empty())
                else {
                    continue;
                };
                *by_root.entry(root.to_string()).or_insert(0) += 1;
            }
            by_root
                .into_iter()
                .map(|(root, binding_count)| {
                    json!({
                        "root": root,
                        "binding_count": binding_count,
                    })
                })
                .collect::<Vec<_>>()
        };

        let scope_symbols_json = resolution
            .symbols
            .get(file_id)
            .map(|symbols| {
                symbols
                    .symbols
                    .values()
                    .map(|symbol| {
                        json!({
                            "name": symbol.name,
                            "kind": item_kind_name(symbol.kind),
                            "defining_file_id": symbol.defining_file_id.raw(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let diagnostics_json =
            resolver_diagnostics_for_file(*file_id, semantic);
        let resolved_bodies_count = semantic
            .resolved_bodies
            .iter()
            .filter(|body| body.containing_scope_file_id == *file_id)
            .count();

        files.push(FileResolvedDump {
            file_id: *file_id,
            path: path_for_file_id(context, *file_id),
            global_items_count: semantic
                .global_items
                .items_in_scope(*file_id)
                .len(),
            local_bindings_count: local_bindings_json.len(),
            path_resolutions_count: path_resolutions_json.len(),
            import_bindings_count: import_bindings_json.len(),
            associated_member_resolutions_count:
                associated_member_resolutions_json.len(),
            resolved_bodies_count,
            diagnostics_count: diagnostics_json.len(),
            diagnostics_json,
            item_table_json,
            local_bindings_json,
            path_resolutions_json,
            import_bindings_json,
            named_root_resolutions_json,
            associated_member_resolutions_json,
            scope_symbols_json,
        });
    }

    Ok(files)
}

pub(crate) fn build_typed_dumps(
    context: &CanonicalDumpContext,
) -> Result<Vec<FileTypedDump>, DynError> {
    let semantic_root_by_file = semantic_root_by_file(context);

    let mut files = Vec::new();
    for file_id in &context.ordered_file_ids {
        let Some(root_file_id) = semantic_root_by_file.get(file_id).copied()
        else {
            continue;
        };
        let Some(semantic) =
            context.analysis.semantic_tables.get(&root_file_id)
        else {
            continue;
        };
        let Some(inference) =
            context.analysis.inference_tables.get(&root_file_id)
        else {
            continue;
        };

        let typed_items = semantic.typed_items.ids_in_scope(*file_id);
        let typed_impl_signatures =
            semantic.signatures.impls_in_scope(*file_id);

        let mut functions_json = Vec::new();
        let mut structs_json = Vec::new();
        let mut enums_json = Vec::new();
        let mut protocols_json = Vec::new();

        for item_id in typed_items {
            let Some(global_item) = semantic.global_items.get(*item_id) else {
                continue;
            };
            let Some(typed_item) = semantic.typed_items.get(*item_id) else {
                continue;
            };

            let base = json!({
                "item_id": item_id.raw(),
                "item_name": global_item.name,
                "item_kind": item_kind_name(global_item.kind),
                "full_path": global_item.full_path,
            });

            match typed_item {
                TypedItemData::Function(signature) => {
                    let mut entry = base;
                    entry["signature"] =
                        typed_function_signature_to_json(signature);
                    functions_json.push(entry);
                }
                TypedItemData::Struct(signature) => {
                    let mut entry = base;
                    entry["signature"] =
                        typed_struct_signature_to_json(signature);
                    structs_json.push(entry);
                }
                TypedItemData::Enum(signature) => {
                    let mut entry = base;
                    entry["signature"] =
                        typed_enum_signature_to_json(signature);
                    enums_json.push(entry);
                }
                TypedItemData::Protocol(signature) => {
                    let mut entry = base;
                    entry["signature"] =
                        typed_protocol_signature_to_json(signature);
                    protocols_json.push(entry);
                }
            }
        }

        let impls_json = typed_impl_signatures
            .iter()
            .map(typed_impl_signature_to_json)
            .collect::<Vec<_>>();

        let typed_signatures_json = json!({
            "functions": functions_json,
            "structs": structs_json,
            "enums": enums_json,
            "protocols": protocols_json,
            "impls": impls_json,
        });

        let mut inferred_expr_types_json = Vec::new();
        let mut inferred_local_types_json = Vec::new();
        let mut call_targets_json = Vec::new();

        for body in semantic
            .resolved_bodies
            .iter()
            .filter(|body| body.containing_scope_file_id == *file_id)
        {
            let Some(body_ref) =
                semantic.hir.body_ref(&body.owner, body.body_index)
            else {
                continue;
            };
            let Some(module) = semantic.hir.hir_modules.get(&body_ref.file_id)
            else {
                continue;
            };

            for (expr_id, expr) in &module.exprs {
                let Some(ty) = inference.expr_type_for_hir_expr(
                    &body.owner,
                    body.body_index,
                    *expr_id,
                ) else {
                    continue;
                };
                inferred_expr_types_json.push(json!({
                    "owner": declaration_owner_to_json(&body.owner),
                    "body_index": body.body_index,
                    "body_id": body_ref.body_id.raw(),
                    "expr_id": expr_id.raw(),
                    "span": span_to_json(expr.origin.span),
                    "type": type_to_json(ty),
                }));
            }

            for local in &body.locals {
                let Some(ty) = inference.local_type_for_resolved_local(
                    &body.owner,
                    body.body_index,
                    local.id,
                ) else {
                    continue;
                };

                inferred_local_types_json.push(json!({
                    "owner": declaration_owner_to_json(&body.owner),
                    "body_index": body.body_index,
                    "local_id_kind": "resolved",
                    "local_id": local.id.raw(),
                    "name": local.name,
                    "kind": format!("{:?}", local.kind),
                    "mutability": format!("{:?}", local.mutability),
                    "type": type_to_json(ty),
                }));
            }

            if let Some(hir_local_types) =
                inference.local_types_for_hir_body(&body.owner, body.body_index)
            {
                for (hir_local_id, ty) in hir_local_types {
                    let (name, kind, mutability) = semantic
                        .hir
                        .hir_local_bindings
                        .binding(*hir_local_id)
                        .map_or_else(
                            || {
                                (
                                    "<unknown-local>".to_string(),
                                    "<unknown-kind>".to_string(),
                                    "<unknown-mutability>".to_string(),
                                )
                            },
                            |binding| {
                                (
                                    binding.name.clone(),
                                    format!("{:?}", binding.kind),
                                    format!("{:?}", binding.mutability),
                                )
                            },
                        );

                    inferred_local_types_json.push(json!({
                        "owner": declaration_owner_to_json(&body.owner),
                        "body_index": body.body_index,
                        "local_id_kind": "hir",
                        "local_id": hir_local_id.raw(),
                        "name": name,
                        "kind": kind,
                        "mutability": mutability,
                        "type": type_to_json(ty),
                    }));
                }
            }

            for (expr_id, target) in
                inference.call_targets_for_body(&body.owner, body.body_index)
            {
                call_targets_json.push(json!({
                    "owner": declaration_owner_to_json(&body.owner),
                    "body_index": body.body_index,
                    "expr_id": expr_id.raw(),
                    "target": inferred_call_target_to_json(&target),
                }));
            }
        }

        let mut diagnostics_json = diagnostics_to_json(&semantic.diagnostics)
            .into_iter()
            .map(|diagnostic| {
                json!({
                    "source": "type_check",
                    "diagnostic": diagnostic,
                })
            })
            .collect::<Vec<_>>();

        diagnostics_json.extend(inference.issues.iter().map(|issue| {
            json!({
                "source": "type_infer",
                "issue": body_infer_issue_to_json(issue),
            })
        }));

        files.push(FileTypedDump {
            file_id: *file_id,
            path: path_for_file_id(context, *file_id),
            typed_items_count: typed_items.len(),
            typed_impls_count: typed_impl_signatures.len(),
            expr_types_count: inferred_expr_types_json.len(),
            local_types_count: inferred_local_types_json.len(),
            selected_call_targets_count: call_targets_json.len(),
            diagnostics_count: diagnostics_json.len(),
            diagnostics_json,
            typed_signatures_json,
            inferred_expr_types_json,
            inferred_local_types_json,
            call_targets_json,
        });
    }

    Ok(files)
}

pub(crate) fn build_pipeline_dump(
    context: &CanonicalDumpContext,
    stages: &[DumpKind],
) -> Result<PipelineDump, DynError> {
    let include_parsed = stage_requested(stages, DumpKind::Parsed);
    let include_expanded = stage_requested(stages, DumpKind::Expanded);
    let include_desugared = stage_requested(stages, DumpKind::Desugared);
    let include_hir = stage_requested(stages, DumpKind::Hir);
    let include_resolved = stage_requested(stages, DumpKind::Resolved);
    let include_typed = stage_requested(stages, DumpKind::Typed);

    let parsed_by_file = if include_parsed {
        Some(
            build_parsed_dumps(context)?
                .into_iter()
                .map(|file| (file.file_id, file))
                .collect::<BTreeMap<_, _>>(),
        )
    } else {
        None
    };

    let expanded_by_file = if include_expanded {
        Some(
            build_expanded_dumps(context)?
                .into_iter()
                .map(|file| (file.file_id, file))
                .collect::<BTreeMap<_, _>>(),
        )
    } else {
        None
    };

    let desugared_by_file = if include_desugared {
        Some(
            build_desugared_dumps(context)?
                .into_iter()
                .map(|file| (file.file_id, file))
                .collect::<BTreeMap<_, _>>(),
        )
    } else {
        None
    };

    let hir_by_file = if include_hir {
        Some(
            build_hir_dumps(context)?
                .into_iter()
                .map(|file| (file.file_id, file))
                .collect::<BTreeMap<_, _>>(),
        )
    } else {
        None
    };

    let resolved_by_file = if include_resolved {
        Some(
            build_resolved_dumps(context)?
                .into_iter()
                .map(|file| (file.file_id, file))
                .collect::<BTreeMap<_, _>>(),
        )
    } else {
        None
    };

    let typed_by_file = if include_typed {
        Some(
            build_typed_dumps(context)?
                .into_iter()
                .map(|file| (file.file_id, file))
                .collect::<BTreeMap<_, _>>(),
        )
    } else {
        None
    };

    let files = context
        .ordered_file_ids
        .iter()
        .map(|file_id| FilePipelineDump {
            file_id: *file_id,
            path: path_for_file_id(context, *file_id),
            parsed: parsed_by_file
                .as_ref()
                .and_then(|by_file| by_file.get(file_id).cloned()),
            expanded: expanded_by_file
                .as_ref()
                .and_then(|by_file| by_file.get(file_id).cloned()),
            desugared: desugared_by_file
                .as_ref()
                .and_then(|by_file| by_file.get(file_id).cloned()),
            hir: hir_by_file
                .as_ref()
                .and_then(|by_file| by_file.get(file_id).cloned()),
            resolved: resolved_by_file
                .as_ref()
                .and_then(|by_file| by_file.get(file_id).cloned()),
            typed: typed_by_file
                .as_ref()
                .and_then(|by_file| by_file.get(file_id).cloned()),
        })
        .collect();

    Ok(PipelineDump { files })
}

fn stage_requested(stages: &[DumpKind], kind: DumpKind) -> bool {
    stages.iter().any(|stage| {
        *stage == kind
            || (*stage == DumpKind::Inferred && kind == DumpKind::Typed)
    })
}

fn semantic_root_by_file(
    context: &CanonicalDumpContext,
) -> BTreeMap<FileId, FileId> {
    let mut by_file = BTreeMap::new();
    for (root_file_id, semantic) in &context.analysis.semantic_tables {
        for hir_file in &semantic.hir.hir_files {
            by_file.entry(hir_file.file_id).or_insert(*root_file_id);
        }
    }
    by_file
}

fn summarize_provenance_map(
    provenance_map: &ProvenanceMap,
) -> (Option<String>, Value) {
    let mut direct_source = 0usize;
    let mut expanded_from = 0usize;
    let mut synthetic = 0usize;

    for (_, provenance) in provenance_map.iter() {
        match provenance {
            Provenance::DirectSource { .. } => direct_source += 1,
            Provenance::ExpandedFrom { .. } => expanded_from += 1,
            Provenance::SyntheticFor { .. } => synthetic += 1,
        }
    }

    let total = direct_source + expanded_from + synthetic;
    let summary = format!(
        "entries={total}, direct_source={direct_source}, expanded_from={expanded_from}, synthetic={synthetic}"
    );

    (
        Some(summary),
        json!({
            "entries": total,
            "direct_source": direct_source,
            "expanded_from": expanded_from,
            "synthetic": synthetic,
        }),
    )
}

fn hir_origin_to_json(origin: &core_x::frontend::HirOrigin) -> Value {
    json!({
        "file_id": origin.file_id.raw(),
        "span": span_to_json(origin.span),
        "provenance": provenance_to_json(&origin.provenance),
    })
}

fn provenance_to_json(provenance: &Provenance) -> Value {
    match provenance {
        Provenance::DirectSource { file_id, span } => json!({
            "kind": "direct_source",
            "file_id": file_id.raw(),
            "span": span_to_json(*span),
        }),
        Provenance::ExpandedFrom {
            macro_name,
            call_site_file,
            call_site_span,
            definition_span,
        } => json!({
            "kind": "expanded_from",
            "macro_name": macro_name,
            "call_site_file_id": call_site_file.raw(),
            "call_site_span": span_to_json(*call_site_span),
            "definition_span": definition_span.map(|span| span_to_json(span)),
        }),
        Provenance::SyntheticFor {
            purpose,
            related_span,
        } => json!({
            "kind": "synthetic",
            "purpose": purpose.to_string(),
            "related_span": related_span.as_ref().map(|(file_id, span)| {
                json!({
                    "file_id": file_id.raw(),
                    "span": span_to_json(*span),
                })
            }),
        }),
    }
}

fn summarize_hir_origins(module: &core_x::frontend::HirModule) -> Value {
    let mut direct_source = 0usize;
    let mut expanded_from = 0usize;
    let mut synthetic = 0usize;

    let mut visit_origin =
        |origin: &core_x::frontend::HirOrigin| match &origin.provenance {
            Provenance::DirectSource { .. } => direct_source += 1,
            Provenance::ExpandedFrom { .. } => expanded_from += 1,
            Provenance::SyntheticFor { .. } => synthetic += 1,
        };

    for item in module.items.values() {
        visit_origin(&item.origin);
    }
    for expr in module.exprs.values() {
        visit_origin(&expr.origin);
    }
    for stmt in module.stmts.values() {
        visit_origin(&stmt.origin);
    }
    for ty in module.types.values() {
        visit_origin(&ty.origin);
    }
    for pat in module.patterns.values() {
        visit_origin(&pat.origin);
    }
    for body in module.bodies.values() {
        visit_origin(&body.origin);
    }

    let total = direct_source + expanded_from + synthetic;
    json!({
        "total_nodes": total,
        "direct_source": direct_source,
        "expanded_from": expanded_from,
        "synthetic": synthetic,
    })
}

fn hir_item_name(kind: &HirItemKind) -> Option<String> {
    match kind {
        HirItemKind::Function(function) => Some(function.name.clone()),
        HirItemKind::Struct(struct_decl) => Some(struct_decl.name.clone()),
        HirItemKind::Enum(enum_decl) => Some(enum_decl.name.clone()),
        HirItemKind::Protocol(protocol) => Some(protocol.name.clone()),
        HirItemKind::Impl(_) => Some("impl".to_string()),
        HirItemKind::Extern(extern_block) => {
            Some(format!("extern {}", extern_block.library_name))
        }
        HirItemKind::Use(_) => Some("use".to_string()),
    }
}

fn hir_item_kind_name(kind: &HirItemKind) -> &'static str {
    match kind {
        HirItemKind::Function(_) => "function",
        HirItemKind::Struct(_) => "struct",
        HirItemKind::Enum(_) => "enum",
        HirItemKind::Protocol(_) => "protocol",
        HirItemKind::Impl(_) => "impl",
        HirItemKind::Extern(_) => "extern",
        HirItemKind::Use(_) => "use",
    }
}

fn serialize_hir_item_kind(kind: &HirItemKind) -> Value {
    match kind {
        HirItemKind::Function(function) => json!({
            "kind": "function",
            "name": function.name,
            "init_origin": function
                .init_origin
                .map(|origin| format!("{:?}", origin)),
            "param_count": function.signature.params.len(),
            "return_type_id": function.signature.return_type.map(|ty| ty.raw()),
            "body_id": function.body.raw(),
        }),
        HirItemKind::Struct(struct_decl) => json!({
            "kind": "struct",
            "name": struct_decl.name,
            "generic_params": struct_decl.generic_params,
            "field_count": struct_decl.fields.len(),
            "method_count": struct_decl.functions.len(),
        }),
        HirItemKind::Enum(enum_decl) => json!({
            "kind": "enum",
            "name": enum_decl.name,
            "generic_params": enum_decl.generic_params,
            "case_count": enum_decl.variants.len(),
            "method_count": enum_decl.functions.len(),
        }),
        HirItemKind::Protocol(protocol) => json!({
            "kind": "protocol",
            "name": protocol.name,
            "generic_params": protocol.generic_params,
            "inheritance_count": protocol.inherited_types.len(),
            "property_count": protocol.properties.len(),
            "function_count": protocol.functions.len(),
            "associated_type_count": protocol.associated_types.len(),
        }),
        HirItemKind::Impl(impl_decl) => json!({
            "kind": "impl",
            "target_type_id": impl_decl.target.raw(),
            "conformance_type_id": impl_decl.conformance.map(|conformance| conformance.raw()),
            "function_count": impl_decl.functions.len(),
        }),
        HirItemKind::Extern(extern_block) => json!({
            "kind": "extern",
            "library_name": extern_block.library_name,
            "function_count": extern_block.functions.len(),
        }),
        HirItemKind::Use(use_item) => json!({
            "kind": "use",
            "tree": format!("{:?}", use_item.tree),
        }),
    }
}

fn serialize_hir_expr_kind(kind: &HirExprKind) -> Value {
    match kind {
        HirExprKind::Literal(literal) => json!({
            "kind": "literal",
            "literal": format!("{:?}", literal),
        }),
        HirExprKind::Path(path) => json!({
            "kind": "path",
            "segments": path.segments,
        }),
        HirExprKind::Array { elements } => json!({
            "kind": "array",
            "elements": elements
                .iter()
                .map(|element| match element {
                    HirArrayElement::Expr(expr_id) => {
                        json!({"kind": "expr", "expr_id": expr_id.raw()})
                    }
                    HirArrayElement::Spread(expr_id) => {
                        json!({"kind": "spread", "expr_id": expr_id.raw()})
                    }
                })
                .collect::<Vec<_>>(),
        }),
        HirExprKind::Call { callee, args } => json!({
            "kind": "call",
            "callee": callee.raw(),
            "args": args
                .iter()
                .map(|arg| {
                    json!({
                        "label": arg.label,
                        "value": arg.value.raw(),
                    })
                })
                .collect::<Vec<_>>(),
        }),
        HirExprKind::Block { body } => json!({
            "kind": "block",
            "body_id": body.raw(),
        }),
        HirExprKind::If {
            condition,
            then_body,
            else_expr,
        } => json!({
            "kind": "if",
            "condition": condition.raw(),
            "then_body": then_body.raw(),
            "else_expr": else_expr.map(|expr| expr.raw()),
        }),
        HirExprKind::While { condition, body } => json!({
            "kind": "while",
            "condition": condition.raw(),
            "body_id": body.raw(),
        }),
        HirExprKind::For {
            pat,
            iterator,
            body,
        } => json!({
            "kind": "for",
            "pat_id": pat.raw(),
            "iterator": iterator.raw(),
            "body_id": body.raw(),
        }),
        HirExprKind::Return { value } => json!({
            "kind": "return",
            "value": value.map(|expr| expr.raw()),
        }),
        HirExprKind::Assign { op, target, value } => json!({
            "kind": "assign",
            "op": format!("{:?}", op),
            "target": target.raw(),
            "value": value.raw(),
        }),
        HirExprKind::Unary { op, expr } => json!({
            "kind": "unary",
            "op": format!("{:?}", op),
            "expr": expr.raw(),
        }),
        HirExprKind::Binary { op, lhs, rhs } => json!({
            "kind": "binary",
            "op": format!("{:?}", op),
            "lhs": lhs.raw(),
            "rhs": rhs.raw(),
        }),
        HirExprKind::Field { base, name } => json!({
            "kind": "field",
            "base": base.raw(),
            "name": name,
        }),
        HirExprKind::OptionalField { base, name } => json!({
            "kind": "optional_field",
            "base": base.raw(),
            "name": name,
        }),
        HirExprKind::NamespaceField {
            base,
            name,
            turbofish,
        } => json!({
            "kind": "namespace_field",
            "base": base.raw(),
            "name": name,
            "turbofish": turbofish.iter().map(|ty| ty.raw()).collect::<Vec<_>>(),
        }),
        HirExprKind::MethodCall {
            receiver,
            method_name,
            args,
        } => json!({
            "kind": "method_call",
            "receiver": receiver.raw(),
            "method_name": method_name,
            "args": args
                .iter()
                .map(|arg| {
                    json!({
                        "label": arg.label,
                        "value": arg.value.raw(),
                    })
                })
                .collect::<Vec<_>>(),
        }),
        HirExprKind::Index { base, index } => json!({
            "kind": "index",
            "base": base.raw(),
            "index": index.raw(),
        }),
        HirExprKind::OptionalIndex { base, index } => json!({
            "kind": "optional_index",
            "base": base.raw(),
            "index": index.raw(),
        }),
        HirExprKind::Tuple { elements } => json!({
            "kind": "tuple",
            "elements": elements.iter().map(|expr| expr.raw()).collect::<Vec<_>>(),
        }),
        HirExprKind::Struct { ty, fields } => json!({
            "kind": "struct_literal",
            "type_id": ty.raw(),
            "fields": fields
                .iter()
                .map(|field| match field {
                    HirStructExprField::Named { name, value } => {
                        json!({"kind": "named", "name": name, "value": value.raw()})
                    }
                    HirStructExprField::Spread { value } => {
                        json!({"kind": "spread", "value": value.raw()})
                    }
                })
                .collect::<Vec<_>>(),
        }),
        HirExprKind::Match { subject, arms } => json!({
            "kind": "match",
            "subject": subject.raw(),
            "arms": arms
                .iter()
                .map(|arm| {
                    json!({
                        "pat_id": arm.pat.raw(),
                        "expr_id": arm.expr.raw(),
                    })
                })
                .collect::<Vec<_>>(),
        }),
        HirExprKind::Closure {
            params,
            body,
            uses_shorthand_params,
            is_unsafe,
        } => json!({
            "kind": "closure",
            "params": params
                .iter()
                .map(|param| {
                    json!({
                        "name": param.name,
                        "type_id": param.ty.map(|ty| ty.raw()),
                    })
                })
                .collect::<Vec<_>>(),
            "body_id": body.raw(),
            "uses_shorthand_params": uses_shorthand_params,
            "is_unsafe": is_unsafe,
        }),
        HirExprKind::ForceUnwrap { expr } => json!({
            "kind": "force_unwrap",
            "expr": expr.raw(),
        }),
        HirExprKind::Cast {
            expr,
            ty,
            is_optional,
        } => json!({
            "kind": "cast",
            "expr": expr.raw(),
            "type_id": ty.raw(),
            "is_optional": is_optional,
        }),
        HirExprKind::Range {
            start,
            end,
            inclusive,
        } => json!({
            "kind": "range",
            "start": start.map(|expr| expr.raw()),
            "end": end.map(|expr| expr.raw()),
            "inclusive": inclusive,
        }),
        HirExprKind::Spread { expr } => json!({
            "kind": "spread",
            "expr": expr.raw(),
        }),
        HirExprKind::Try { expr } => json!({
            "kind": "try",
            "expr": expr.raw(),
        }),
        HirExprKind::Break => json!({"kind": "break"}),
        HirExprKind::Continue => json!({"kind": "continue"}),
    }
}

fn serialize_hir_stmt_kind(kind: &HirStmtKind) -> Value {
    match kind {
        HirStmtKind::Let(let_stmt) => json!({
            "kind": "let",
            "pat_id": let_stmt.pat.raw(),
            "type_id": let_stmt.ty.map(|ty| ty.raw()),
            "value_expr_id": let_stmt.value.map(|value| value.raw()),
            "mutability": format!("{:?}", let_stmt.mutability),
        }),
        HirStmtKind::Expr { expr } => json!({
            "kind": "expr",
            "expr_id": expr.raw(),
        }),
        HirStmtKind::Semi { expr } => json!({
            "kind": "semi",
            "expr_id": expr.raw(),
        }),
        HirStmtKind::Item { item } => json!({
            "kind": "item",
            "item_id": item.raw(),
        }),
    }
}

fn serialize_hir_type_kind(kind: &HirTypeKind) -> Value {
    match kind {
        HirTypeKind::Path(path) => json!({
            "kind": "path",
            "segments": path.segments,
        }),
        HirTypeKind::Lifetime(name) => json!({
            "kind": "lifetime",
            "name": name,
        }),
        HirTypeKind::Reference {
            mutable,
            lifetime,
            inner,
        } => json!({
            "kind": "reference",
            "mutable": mutable,
            "lifetime": lifetime,
            "inner_type_id": inner.raw(),
        }),
        HirTypeKind::Pointer { mutable, inner } => json!({
            "kind": "pointer",
            "mutable": mutable,
            "inner_type_id": inner.raw(),
        }),
        HirTypeKind::Optional { inner } => json!({
            "kind": "optional",
            "inner_type_id": inner.raw(),
        }),
        HirTypeKind::Result { ok, err } => json!({
            "kind": "result",
            "ok_type_id": ok.raw(),
            "err_type_id": err.raw(),
        }),
        HirTypeKind::GenericApplication { base, args } => json!({
            "kind": "generic_application",
            "base_type_id": base.raw(),
            "arg_type_ids": args.iter().map(|arg| arg.raw()).collect::<Vec<_>>(),
        }),
        HirTypeKind::SelfType => json!({"kind": "self"}),
        HirTypeKind::Tuple(elems) => json!({
            "kind": "tuple",
            "element_type_ids": elems.iter().map(|e| e.raw()).collect::<Vec<_>>(),
        }),
    }
}

fn serialize_hir_pattern_kind(kind: &HirPatKind) -> Value {
    match kind {
        HirPatKind::Binding { name } => json!({
            "kind": "binding",
            "name": name,
        }),
        HirPatKind::Wildcard => json!({"kind": "wildcard"}),
        HirPatKind::Tuple { elements } => json!({
            "kind": "tuple",
            "element_pattern_ids": elements
                .iter()
                .map(|pat| pat.raw())
                .collect::<Vec<_>>(),
        }),
        HirPatKind::Struct {
            path,
            fields,
            has_rest,
        } => json!({
            "kind": "struct",
            "path": path.segments,
            "fields": fields
                .iter()
                .map(|field| {
                    json!({
                        "name": field.name,
                        "pattern_id": field.pat.map(|pat| pat.raw()),
                    })
                })
                .collect::<Vec<_>>(),
            "has_rest": has_rest,
        }),
        HirPatKind::EnumVariant {
            path,
            shorthand,
            args,
            has_rest,
        } => json!({
            "kind": "enum_variant",
            "path": path.segments,
            "shorthand": shorthand,
            "arg_pattern_ids": args.iter().map(|pat| pat.raw()).collect::<Vec<_>>(),
            "has_rest": has_rest,
        }),
        HirPatKind::Literal(literal) => json!({
            "kind": "literal",
            "literal": format!("{:?}", literal),
        }),
    }
}

fn namespace_segments_for_expr(
    module: &core_x::frontend::HirModule,
    expr_id: core_x::frontend::HirExprId,
) -> Option<Vec<String>> {
    let expr = module.exprs.get(&expr_id)?;
    match &expr.kind {
        HirExprKind::Path(path) => Some(path.segments.clone()),
        HirExprKind::NamespaceField { base, name, .. } => {
            let mut segments = namespace_segments_for_expr(module, *base)?;
            segments.push(name.clone());
            Some(segments)
        }
        _ => None,
    }
}

fn serialize_hir_path_resolution(resolution: &HirPathResolution) -> Value {
    match resolution {
        HirPathResolution::Local(local_id) => json!({
            "kind": "local",
            "local_id": local_id.raw(),
        }),
        HirPathResolution::Item(item_ref) => json!({
            "kind": "item",
            "file_id": item_ref.file_id.raw(),
            "item_id": item_ref.item_id.raw(),
        }),
        HirPathResolution::AssociatedMember {
            type_item_ref,
            member_name,
            member_kind,
        } => json!({
            "kind": "associated_member",
            "type_item_file_id": type_item_ref.file_id.raw(),
            "type_item_id": type_item_ref.item_id.raw(),
            "member_name": member_name,
            "member_kind": associated_member_kind_name(*member_kind),
        }),
    }
}

fn associated_member_kind_name(kind: AssociatedMemberKind) -> &'static str {
    match kind {
        AssociatedMemberKind::Method => "method",
        AssociatedMemberKind::Initializer => "initializer",
    }
}

fn resolver_diagnostics_for_file(
    file_id: FileId,
    semantic: &core_x::frontend::SemanticAnalysis,
) -> Vec<Value> {
    let mut diagnostics = Vec::new();

    diagnostics.extend(
        semantic
            .hir
            .hir_path_table
            .unresolved_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.file_id == file_id)
            .map(|diagnostic| {
                json!({
                    "kind": "unresolved_hir_path",
                    "file_id": diagnostic.file_id.raw(),
                    "expr_id": diagnostic.expr_id.raw(),
                    "span": span_to_json(diagnostic.span),
                    "segments": diagnostic.segments,
                })
            }),
    );

    diagnostics.extend(
        semantic
            .declarations
            .unresolved_paths
            .iter()
            .filter(|path| path.containing_scope_file_id == file_id)
            .map(|path| {
                json!({
                    "kind": "unresolved_declaration_path",
                    "owner": declaration_owner_to_json(&path.owner),
                    "containing_scope_file_id": path.containing_scope_file_id.raw(),
                    "path": path.path,
                })
            }),
    );

    for body in semantic
        .resolved_bodies
        .iter()
        .filter(|body| body.containing_scope_file_id == file_id)
    {
        diagnostics.extend(body.unresolved_references.iter().map(
            |reference| {
                json!({
                    "kind": "unresolved_body_reference",
                    "owner": declaration_owner_to_json(&body.owner),
                    "body_index": body.body_index,
                    "span": span_to_json(reference.span),
                    "segments": reference.segments,
                })
            },
        ));
    }

    diagnostics
}

fn serialize_import_binding_kind(
    kind: &core_x::frontend::ImportBindingKind,
) -> Value {
    match kind {
        core_x::frontend::ImportBindingKind::Scope => {
            json!({"kind": "scope"})
        }
        core_x::frontend::ImportBindingKind::Symbol(symbol_kind) => {
            json!({
                "kind": "symbol",
                "symbol_kind": item_kind_name(*symbol_kind),
            })
        }
    }
}

fn typed_function_signature_to_json(
    signature: &core_x::frontend::TypedFunctionSignature,
) -> Value {
    json!({
        "param_labels": signature
            .param_labels
            .iter()
            .map(typed_param_label_to_json)
            .collect::<Vec<_>>(),
        "param_types": signature
            .param_types
            .iter()
            .map(type_to_json)
            .collect::<Vec<_>>(),
        "return_type": signature.return_type.as_ref().map(type_to_json),
    })
}

fn typed_named_function_signature_to_json(
    signature: &core_x::frontend::TypedNamedFunctionSignature,
) -> Value {
    json!({
        "name": signature.name,
        "signature": typed_function_signature_to_json(&signature.signature),
    })
}

fn typed_struct_signature_to_json(
    signature: &core_x::frontend::TypedStructSignatureData,
) -> Value {
    json!({
        "fields": signature
            .fields
            .iter()
            .map(|field| {
                json!({
                    "name": field.name,
                    "type": type_to_json(&field.ty),
                })
            })
            .collect::<Vec<_>>(),
        "methods": signature
            .method_signatures
            .iter()
            .map(typed_named_function_signature_to_json)
            .collect::<Vec<_>>(),
        "initializers": signature
            .initializer_signatures
            .iter()
            .map(typed_function_signature_to_json)
            .collect::<Vec<_>>(),
    })
}

fn typed_enum_signature_to_json(
    signature: &core_x::frontend::TypedEnumSignatureData,
) -> Value {
    json!({
        "cases": signature
            .case_signatures
            .iter()
            .map(|case_| {
                json!({
                    "name": case_.name,
                    "payload_types": case_
                        .payload_types
                        .iter()
                        .map(type_to_json)
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
        "methods": signature
            .method_signatures
            .iter()
            .map(typed_named_function_signature_to_json)
            .collect::<Vec<_>>(),
        "initializers": signature
            .initializer_signatures
            .iter()
            .map(typed_function_signature_to_json)
            .collect::<Vec<_>>(),
    })
}

fn typed_protocol_signature_to_json(
    signature: &core_x::frontend::TypedProtocolSignatureData,
) -> Value {
    json!({
        "inheritance_types": signature
            .inheritance_types
            .iter()
            .map(type_to_json)
            .collect::<Vec<_>>(),
        "properties": signature
            .properties
            .iter()
            .map(|property| {
                json!({
                    "name": property.name,
                    "type": type_to_json(&property.ty),
                })
            })
            .collect::<Vec<_>>(),
        "methods": signature
            .method_signatures
            .iter()
            .map(typed_named_function_signature_to_json)
            .collect::<Vec<_>>(),
        "initializers": signature
            .initializer_signatures
            .iter()
            .map(typed_function_signature_to_json)
            .collect::<Vec<_>>(),
        "associated_type_bounds": signature
            .associated_type_bounds
            .iter()
            .map(|assoc| {
                json!({
                    "name": assoc.name,
                    "bounds": assoc
                        .bounds
                        .iter()
                        .map(type_to_json)
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
    })
}

fn typed_impl_signature_to_json(
    signature: &core_x::frontend::TypedImplSignature,
) -> Value {
    json!({
        "owner": declaration_owner_to_json(&signature.owner),
        "containing_scope_file_id": signature.containing_scope_file_id.raw(),
        "target": type_to_json(&signature.target),
        "conformance": signature.conformance.as_ref().map(type_to_json),
        "methods": signature
            .method_signatures
            .iter()
            .map(typed_named_function_signature_to_json)
            .collect::<Vec<_>>(),
        "initializers": signature
            .initializer_signatures
            .iter()
            .map(typed_function_signature_to_json)
            .collect::<Vec<_>>(),
    })
}

fn type_to_json(ty: &Type) -> Value {
    match ty {
        Type::Builtin(builtin) => json!({
            "kind": "builtin",
            "name": builtin.to_string(),
        }),
        Type::Named { item_id, kind } => json!({
            "kind": "named",
            "item_id": item_id.raw(),
            "named_kind": format!("{:?}", kind),
        }),
        Type::Pointer {
            pointee,
            mutability,
        } => json!({
            "kind": "pointer",
            "mutability": mutability.to_string(),
            "pointee": type_to_json(pointee),
        }),
        Type::Error => json!({"kind": "error"}),
    }
}

fn typed_param_label_to_json(label: &TypedParamLabel) -> Value {
    match label {
        TypedParamLabel::None => json!({"kind": "none"}),
        TypedParamLabel::Explicit(label) => json!({
            "kind": "explicit",
            "label": label,
        }),
        TypedParamLabel::FromName => json!({"kind": "from_name"}),
    }
}

fn inferred_call_target_to_json(target: &InferredCallTarget) -> Value {
    match target {
        InferredCallTarget::Function { path } => json!({
            "kind": "function",
            "path": path,
        }),
        InferredCallTarget::AssociatedMember {
            type_item_id,
            member_name,
            member_kind,
        } => json!({
            "kind": "associated_member",
            "type_item_id": type_item_id.map(|item_id| item_id.raw()),
            "member_name": member_name,
            "member_kind": associated_member_kind_name(*member_kind),
        }),
        InferredCallTarget::Method {
            receiver_item_id,
            method_name,
        } => json!({
            "kind": "method",
            "receiver_item_id": receiver_item_id.map(|item_id| item_id.raw()),
            "method_name": method_name,
        }),
        InferredCallTarget::EnumCase {
            enum_item_id,
            case_name,
        } => json!({
            "kind": "enum_case",
            "enum_item_id": enum_item_id.raw(),
            "case_name": case_name,
        }),
    }
}

fn body_infer_issue_to_json(issue: &BodyInferIssue) -> Value {
    json!({
        "owner": declaration_owner_to_json(&issue.owner),
        "body_index": issue.body_index,
        "span": span_to_json(issue.span),
        "kind": body_infer_issue_kind_to_json(&issue.kind),
    })
}

fn body_infer_issue_kind_to_json(kind: &BodyInferIssueKind) -> Value {
    match kind {
        BodyInferIssueKind::MissingBodyAst => {
            json!({"kind": "missing_body_ast"})
        }
        BodyInferIssueKind::MissingBodyEnvironment => {
            json!({"kind": "missing_body_environment"})
        }
        BodyInferIssueKind::MissingResolvedPath { expr_id } => json!({
            "kind": "missing_resolved_path",
            "expr_id": expr_id.raw(),
        }),
        BodyInferIssueKind::MissingElseBranch => {
            json!({"kind": "missing_else_branch"})
        }
        BodyInferIssueKind::InvalidCallTarget => {
            json!({"kind": "invalid_call_target"})
        }
        BodyInferIssueKind::NoMatchingCallCandidate { candidate_count } => {
            json!({
                "kind": "no_matching_call_candidate",
                "candidate_count": candidate_count,
            })
        }
        BodyInferIssueKind::AmbiguousCallCandidate { candidate_count } => {
            json!({
                "kind": "ambiguous_call_candidate",
                "candidate_count": candidate_count,
            })
        }
        BodyInferIssueKind::MissingLocalBinding { pat_id } => json!({
            "kind": "missing_local_binding",
            "pat_id": pat_id.raw(),
        }),
        BodyInferIssueKind::RequiresExplicitLocalTypeAnnotation {
            hir_local_id,
            resolved_local_id,
        } => json!({
            "kind": "requires_explicit_local_type_annotation",
            "hir_local_id": hir_local_id.raw(),
            "resolved_local_id": resolved_local_id.map(|local_id| local_id.raw()),
        }),
        BodyInferIssueKind::AmbiguousEnumCase {
            case_name,
            candidates,
        } => json!({
            "kind": "ambiguous_enum_case",
            "case_name": case_name,
            "candidates": candidates
                .iter()
                .map(|(item_id, name)| {
                    json!({
                        "item_id": item_id.raw(),
                        "name": name,
                    })
                })
                .collect::<Vec<_>>(),
        }),
        BodyInferIssueKind::MissingEnumCase {
            case_name,
            available_enums,
        } => json!({
            "kind": "missing_enum_case",
            "case_name": case_name,
            "available_enums": available_enums,
        }),
        BodyInferIssueKind::CoreInferenceIssue { kind } => json!({
            "kind": "core_inference_issue",
            "detail": format!("{:?}", kind),
        }),
    }
}

fn span_to_json(span: core_x::frontend::ast::Span) -> Value {
    json!({
        "start": span.start,
        "end": span.end,
    })
}

fn declaration_owner_to_json(owner: &DeclarationOwner) -> Value {
    match owner {
        DeclarationOwner::Item(item_id) => json!({
            "kind": "item",
            "item_id": item_id.raw(),
        }),
        DeclarationOwner::Impl {
            scope_file_id,
            impl_index,
        } => json!({
            "kind": "impl",
            "scope_file_id": scope_file_id.raw(),
            "impl_index": impl_index,
        }),
    }
}

fn item_kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Scope => "scope",
        ItemKind::Function => "function",
        ItemKind::Struct => "struct",
        ItemKind::Enum => "enum",
        ItemKind::Protocol => "protocol",
    }
}

pub(crate) fn canonical_pipeline_stage_order() -> Vec<DumpKind> {
    vec![
        DumpKind::Parsed,
        DumpKind::Expanded,
        DumpKind::Desugared,
        DumpKind::Hir,
        DumpKind::Resolved,
        DumpKind::Typed,
    ]
}

pub(crate) fn normalize_canonical_stage_list(
    stages: &[DumpKind],
) -> Vec<DumpKind> {
    if stages.iter().any(|stage| *stage == DumpKind::Pipeline) {
        return canonical_pipeline_stage_order();
    }

    let mut normalized = Vec::new();
    let mut seen = BTreeSet::new();

    for stage in stages {
        let canonical = match stage {
            DumpKind::Inferred => DumpKind::Typed,
            other => *other,
        };
        if seen.insert(canonical) {
            normalized.push(canonical);
        }
    }

    normalized
}
