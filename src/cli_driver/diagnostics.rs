use crate::cli_driver::project::{ProjectContext, parsed_by_id};
use core_x::frontend::source::SourceDb;
use core_x::frontend::{DiagnosticRenderer, DiagnosticsBag, ParsedFile};

pub fn emit_diagnostics_bag(db: &SourceDb, bag: &DiagnosticsBag) {
    if bag.is_empty() {
        return;
    }
    let diagnostic_renderer = DiagnosticRenderer::new(db);
    let rendered_output = diagnostic_renderer.render_all(bag.as_slice());
    if !rendered_output.is_empty() {
        eprintln!("{rendered_output}");
    }
}

pub fn emit_file_diagnostics(db: &SourceDb, parsed: &ParsedFile) {
    if parsed.diagnostics.is_empty() {
        return;
    }
    let diagnostic_renderer = DiagnosticRenderer::new(db);
    let rendered_output =
        diagnostic_renderer.render_all(parsed.diagnostics.as_slice());
    if !rendered_output.is_empty() {
        eprintln!("{rendered_output}");
    }
}

pub fn emit_context_diagnostics(context: &ProjectContext) {
    let parsed_by_id = parsed_by_id(&context.parsed_files);
    let diagnostic_renderer = DiagnosticRenderer::new(&context.db);
    let mut rendered_per_file = Vec::new();

    for file_id in &context.ordered_file_ids {
        let Some(parsed) = parsed_by_id.get(file_id) else {
            continue;
        };
        if parsed.diagnostics.is_empty() {
            continue;
        }
        let rendered_output =
            diagnostic_renderer.render_all(parsed.diagnostics.as_slice());
        if !rendered_output.is_empty() {
            rendered_per_file.push(rendered_output);
        }
    }

    if !rendered_per_file.is_empty() {
        eprintln!("{}", rendered_per_file.join("\n\n"));
    }
}
