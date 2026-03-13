use crate::cli_driver::dump::model::{
    FileAstDump, FileParsedDump, FileTokenDump, ResolvedImportDump,
    ResolvedScopeDump, ResolvedSemanticDump,
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
    }
    out.trim_end_matches('\n').to_string()
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
