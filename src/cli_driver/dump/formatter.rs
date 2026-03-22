use crate::cli_driver::dump::model::{
    FileAstDump, FileDesugaredDump, FileExpandedDump, FileHirDump,
    FileParsedDump, FilePipelineDump, FileResolvedDump, FileTokenDump,
    FileTypedDump, ResolvedImportDump, ResolvedScopeDump, ResolvedSemanticDump,
};
use crate::cli_driver::project::{ProjectContext, path_for_file_id};
use crate::cli_driver::ui::{ui_header, ui_section};
use core_x::frontend::DiagnosticsBag;
use serde_json::{Value, json};
use std::fmt::Write as _;

pub fn format_tokens_text(files: &[FileTokenDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!("== file: {} ==", file.path))
        );
        for token in &file.tokens {
            let _ = writeln!(
                out,
                "{} {}..{} {:?}",
                token.kind, token.start, token.end, token.text
            );
        }
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_ast_text(files: &[FileAstDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!("== file: {} ==", file.path))
        );
        let _ =
            writeln!(out, "{} {}", ui_section("file_id:"), file.file_id.raw());
        let _ =
            writeln!(out, "{} {}", ui_section("item_count:"), file.item_count);
        out.push_str(&file.ast_debug);
        if !file.ast_debug.ends_with('\n') {
            out.push('\n');
        }
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_parsed_text(files: &[FileParsedDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!("== file: {} ==", file.path))
        );
        let _ =
            writeln!(out, "{} {}", ui_section("file_id:"), file.file_id.raw());
        let _ =
            writeln!(out, "{} {}", ui_section("item_count:"), file.item_count);
        let _ = writeln!(
            out,
            "{} {}",
            ui_section("diagnostics_count:"),
            file.diagnostics_count
        );
        out.push_str(&file.parsed_debug);
        if !file.parsed_debug.ends_with('\n') {
            out.push('\n');
        }
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_scopes_text(
    context: &ProjectContext,
    resolved: &[ResolvedScopeDump],
) -> String {
    let mut out = String::new();
    for (index, item) in resolved.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!(
                "== target: {} ({}) ==",
                item.target.label,
                path_for_file_id(context, item.target.root_file_id)
            ))
        );
        let _ = writeln!(out, "{:#?}", item.graph);
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_imports_text(
    context: &ProjectContext,
    resolved: &[ResolvedImportDump],
) -> String {
    let mut out = String::new();
    for (index, item) in resolved.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!(
                "== target: {} ({}) ==",
                item.target.label,
                path_for_file_id(context, item.target.root_file_id)
            ))
        );
        let _ = writeln!(out, "{}", ui_section("scope_graph:"));
        let _ = writeln!(out, "{:#?}", item.graph);
        let _ = writeln!(out, "{}", ui_section("scope_symbols:"));
        let _ = writeln!(out, "{:#?}", item.symbols);
        let _ = writeln!(out, "{}", ui_section("resolved_imports:"));
        let _ = writeln!(out, "{:#?}", item.imports);
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_semantic_text(
    context: &ProjectContext,
    resolved: &[ResolvedSemanticDump],
) -> String {
    let mut out = String::new();
    for (index, item) in resolved.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!(
                "== target: {} ({}) ==",
                item.target.label,
                path_for_file_id(context, item.target.root_file_id)
            ))
        );
        let _ = writeln!(out, "{}", ui_section("scope_graph:"));
        let _ = writeln!(out, "{:#?}", item.graph);
        let _ = writeln!(out, "{}", ui_section("scope_symbols:"));
        let _ = writeln!(out, "{:#?}", item.symbols);
        let _ = writeln!(out, "{}", ui_section("resolved_imports:"));
        let _ = writeln!(out, "{:#?}", item.imports);
        let _ = writeln!(out, "{}", ui_section("semantic_summary:"));
        let _ =
            writeln!(out, "global_items: {}", item.semantic.global_items.len());
        let _ =
            writeln!(out, "typed_items: {}", item.semantic.typed_items.len());
        let _ =
            writeln!(out, "typed_bodies: {}", item.semantic.typed_bodies.len());
        let _ = writeln!(
            out,
            "semantic_diagnostics: {}",
            item.semantic.diagnostics.len()
        );
        let _ = writeln!(out, "{}", ui_section("inference_summary:"));
        let _ =
            writeln!(out, "expr_types: {}", item.inference.expr_type_count());
        let _ = writeln!(
            out,
            "local_types: {}",
            item.inference.inferred_hir_local_count()
        );
        let _ =
            writeln!(out, "root_types: {}", item.inference.root_type_count());
        let _ = writeln!(
            out,
            "selected_call_targets: {}",
            item.inference.call_target_count()
        );
        let _ = writeln!(
            out,
            "inference_diagnostics: {}",
            item.inference.issues.len()
        );
        let _ = writeln!(out, "{}", ui_section("inferred_locals:"));
        for body in item.semantic.resolved_bodies.iter() {
            for local in &body.locals {
                let Some(ty) = item.inference.local_type_for_resolved_local(
                    &body.owner,
                    body.body_index,
                    local.id,
                ) else {
                    continue;
                };
                if ty.is_error() {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "[{:?} body {}] {}: {}",
                    body.owner, body.body_index, local.name, ty
                );
            }
        }
        let _ = writeln!(out, "{}", ui_section("selected_call_targets:"));
        for body in item.semantic.resolved_bodies.iter() {
            for (expr_id, target) in item
                .inference
                .call_targets_for_body(&body.owner, body.body_index)
            {
                let _ = writeln!(
                    out,
                    "[{:?} body {} expr #{}] {}",
                    body.owner,
                    body.body_index,
                    expr_id.raw(),
                    format_inferred_call_target(&target)
                );
            }
        }
    }
    out.trim_end_matches('\n').to_string()
}

fn format_inferred_call_target(
    target: &core_x::midend::InferredCallTarget,
) -> String {
    match target {
        core_x::midend::InferredCallTarget::Function { path } => {
            format!("function {}", path.join("::"))
        }
        core_x::midend::InferredCallTarget::AssociatedMember {
            type_item_id,
            member_name,
            member_kind,
        } => format!(
            "associated {:?} {} (type item id: {})",
            member_kind,
            member_name,
            type_item_id
                .map(|item_id| item_id.raw().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
        core_x::midend::InferredCallTarget::Method {
            receiver_item_id,
            method_name,
        } => format!(
            "method {} (receiver item id: {})",
            method_name,
            receiver_item_id
                .map(|item_id| item_id.raw().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
        core_x::midend::InferredCallTarget::EnumCase {
            enum_item_id,
            case_name,
        } => format!(
            "enum case {} (enum item id: {})",
            case_name,
            enum_item_id.raw()
        ),
    }
}

pub fn diagnostics_to_json(diagnostics: &DiagnosticsBag) -> Vec<Value> {
    diagnostics
        .as_slice()
        .iter()
        .map(|diagnostic| {
            json!({
                "severity": format!("{:?}", diagnostic.severity),
                "message": diagnostic.message,
                "labels": diagnostic.labels.iter().map(|label| {
                    json!({
                        "kind": format!("{:?}", label.kind),
                        "span": {
                            "file_id": label.span.file_id.raw(),
                            "start": label.span.span.start,
                            "end": label.span.span.end,
                        },
                        "message": label.message,
                    })
                }).collect::<Vec<_>>(),
                "notes": diagnostic.notes,
                "help": diagnostic.help,
            })
        })
        .collect()
}

pub fn format_expanded_text(files: &[FileExpandedDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!("== file: {} ==", file.path))
        );
        let _ = writeln!(out, "file_id: {}", file.file_id.raw());
        let _ = writeln!(out, "item_count: {}", file.item_count);
        let _ = writeln!(out, "diagnostics_count: {}", file.diagnostics_count);
        if let Some(summary) = &file.provenance_summary {
            let _ = writeln!(out, "provenance_summary: {}", summary);
        }
        out.push_str(&file.expanded_debug);
        if !file.expanded_debug.ends_with('\n') {
            out.push('\n');
        }
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_desugared_text(files: &[FileDesugaredDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!("== file: {} ==", file.path))
        );
        let _ = writeln!(out, "file_id: {}", file.file_id.raw());
        let _ = writeln!(out, "item_count: {}", file.item_count);
        let _ = writeln!(out, "diagnostics_count: {}", file.diagnostics_count);
        if let Some(summary) = &file.normalized_forms_summary {
            let _ = writeln!(out, "normalized_forms: {summary}");
        }
        out.push_str(&file.desugared_debug);
        if !file.desugared_debug.ends_with('\n') {
            out.push('\n');
        }
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_hir_text(files: &[FileHirDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!("== file: {} ==", file.path))
        );
        let _ = writeln!(out, "file_id: {}", file.file_id.raw());
        let _ = writeln!(out, "root_items: {}", file.root_items_count);
        let _ = writeln!(out, "bodies: {}", file.bodies_count);
        let _ = writeln!(out, "exprs: {}", file.exprs_count);
        let _ = writeln!(out, "stmts: {}", file.stmts_count);
        let _ = writeln!(out, "types: {}", file.types_count);
        let _ = writeln!(out, "patterns: {}", file.patterns_count);
        let _ = writeln!(out, "diagnostics_count: {}", file.diagnostics_count);
        let _ = writeln!(
            out,
            "{} {}",
            ui_section("origin_summary:"),
            pretty_json(&file.origin_summary_json)
        );
        out.push_str(&file.hir_debug);
        if !file.hir_debug.ends_with('\n') {
            out.push('\n');
        }
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_resolved_text(files: &[FileResolvedDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!("== file: {} ==", file.path))
        );
        let _ = writeln!(out, "file_id: {}", file.file_id.raw());
        let _ = writeln!(out, "global_items: {}", file.global_items_count);
        let _ = writeln!(out, "local_bindings: {}", file.local_bindings_count);
        let _ =
            writeln!(out, "path_resolutions: {}", file.path_resolutions_count);
        let _ =
            writeln!(out, "import_bindings: {}", file.import_bindings_count);
        let _ = writeln!(
            out,
            "associated_member_resolutions: {}",
            file.associated_member_resolutions_count
        );
        let _ =
            writeln!(out, "resolved_bodies: {}", file.resolved_bodies_count);
        let _ = writeln!(out, "diagnostics_count: {}", file.diagnostics_count);
        let _ = writeln!(out, "{}", ui_section("item_table:"));
        let _ = writeln!(out, "{}", pretty_json(&file.item_table_json));
        let _ = writeln!(out, "{}", ui_section("resolver_diagnostics:"));
        let _ = writeln!(out, "{}", pretty_json(&json!(file.diagnostics_json)));
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_typed_text(files: &[FileTypedDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!("== file: {} ==", file.path))
        );
        let _ = writeln!(out, "file_id: {}", file.file_id.raw());
        let _ = writeln!(out, "typed_items: {}", file.typed_items_count);
        let _ = writeln!(out, "typed_impls: {}", file.typed_impls_count);
        let _ = writeln!(out, "expr_types: {}", file.expr_types_count);
        let _ = writeln!(out, "local_types: {}", file.local_types_count);
        let _ = writeln!(
            out,
            "selected_call_targets: {}",
            file.selected_call_targets_count
        );
        let _ = writeln!(out, "diagnostics_count: {}", file.diagnostics_count);
        let _ = writeln!(out, "{}", ui_section("typed_signatures:"));
        let _ = writeln!(out, "{}", pretty_json(&file.typed_signatures_json));
        let _ = writeln!(out, "{}", ui_section("inferred_locals:"));
        let _ = writeln!(
            out,
            "{}",
            pretty_json(&json!(file.inferred_local_types_json))
        );
        let _ = writeln!(out, "{}", ui_section("inferred_expr_types:"));
        let _ = writeln!(
            out,
            "{}",
            pretty_json(&json!(file.inferred_expr_types_json))
        );
    }
    out.trim_end_matches('\n').to_string()
}

pub fn format_pipeline_text(
    files: &[FilePipelineDump],
    stage_labels: &[&str],
) -> String {
    let mut out = String::new();

    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }

        let _ = writeln!(
            out,
            "{}",
            ui_header(&format!("== file: {} ==", file.path))
        );
        let _ = writeln!(out, "file_id: {}", file.file_id.raw());

        for stage in stage_labels {
            match *stage {
                "parsed" => {
                    if let Some(parsed) = &file.parsed {
                        let _ = writeln!(out, "{}", ui_section("[parsed]"));
                        let _ =
                            writeln!(out, "item_count: {}", parsed.item_count);
                        let _ = writeln!(
                            out,
                            "diagnostics_count: {}",
                            parsed.diagnostics_count
                        );
                    }
                }
                "expanded" => {
                    if let Some(expanded) = &file.expanded {
                        let _ = writeln!(out, "{}", ui_section("[expanded]"));
                        let _ = writeln!(
                            out,
                            "item_count: {}",
                            expanded.item_count
                        );
                        let _ = writeln!(
                            out,
                            "diagnostics_count: {}",
                            expanded.diagnostics_count
                        );
                        if let Some(summary) = &expanded.provenance_summary {
                            let _ =
                                writeln!(out, "provenance_summary: {summary}");
                        }
                    }
                }
                "desugared" => {
                    if let Some(desugared) = &file.desugared {
                        let _ = writeln!(out, "{}", ui_section("[desugared]"));
                        let _ = writeln!(
                            out,
                            "item_count: {}",
                            desugared.item_count
                        );
                        let _ = writeln!(
                            out,
                            "diagnostics_count: {}",
                            desugared.diagnostics_count
                        );
                        if let Some(summary) =
                            &desugared.normalized_forms_summary
                        {
                            let _ =
                                writeln!(out, "normalized_forms: {summary}");
                        }
                    }
                }
                "hir" => {
                    if let Some(hir) = &file.hir {
                        let _ = writeln!(out, "{}", ui_section("[hir]"));
                        let _ = writeln!(
                            out,
                            "root_items: {}",
                            hir.root_items_count
                        );
                        let _ = writeln!(out, "bodies: {}", hir.bodies_count);
                        let _ = writeln!(out, "exprs: {}", hir.exprs_count);
                        let _ = writeln!(out, "stmts: {}", hir.stmts_count);
                        let _ = writeln!(out, "types: {}", hir.types_count);
                        let _ =
                            writeln!(out, "patterns: {}", hir.patterns_count);
                    }
                }
                "resolved" => {
                    if let Some(resolved) = &file.resolved {
                        let _ = writeln!(out, "{}", ui_section("[resolved]"));
                        let _ = writeln!(
                            out,
                            "global_items: {}",
                            resolved.global_items_count
                        );
                        let _ = writeln!(
                            out,
                            "local_bindings: {}",
                            resolved.local_bindings_count
                        );
                        let _ = writeln!(
                            out,
                            "path_resolutions: {}",
                            resolved.path_resolutions_count
                        );
                        let _ = writeln!(
                            out,
                            "import_bindings: {}",
                            resolved.import_bindings_count
                        );
                    }
                }
                "typed" => {
                    if let Some(typed) = &file.typed {
                        let _ = writeln!(out, "{}", ui_section("[typed]"));
                        let _ = writeln!(
                            out,
                            "typed_items: {}",
                            typed.typed_items_count
                        );
                        let _ = writeln!(
                            out,
                            "typed_impls: {}",
                            typed.typed_impls_count
                        );
                        let _ = writeln!(
                            out,
                            "expr_types: {}",
                            typed.expr_types_count
                        );
                        let _ = writeln!(
                            out,
                            "local_types: {}",
                            typed.local_types_count
                        );
                        let _ = writeln!(
                            out,
                            "selected_call_targets: {}",
                            typed.selected_call_targets_count
                        );
                    }
                }
                _ => {}
            }
        }
    }

    out.trim_end_matches('\n').to_string()
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| "<json-encode-error>".to_string())
}
