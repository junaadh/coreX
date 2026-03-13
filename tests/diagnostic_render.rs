use core_x::frontend::ast::Span;
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    Diagnostic, DiagnosticLabel, DiagnosticRenderer, FileSpan,
};

#[test]
fn render_diagnostic_without_labels() {
    let db = SourceDb::new();
    let renderer = DiagnosticRenderer::new(&db);
    let diagnostic = Diagnostic::warning("unused variable");

    let rendered_output = renderer.render(&diagnostic);
    assert!(rendered_output.contains("warning: unused variable"));
    assert!(!rendered_output.contains(" --> "));
}

#[test]
fn render_primary_label_includes_path_line_and_column() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("example.cx", "fn f() {}\n");
    let renderer = DiagnosticRenderer::new(&db);

    let diagnostic = Diagnostic::error("unexpected token").with_label(
        DiagnosticLabel::primary(
            FileSpan::new(file_id, Span::new(3, 4)),
            "here",
        ),
    );

    let rendered_output = renderer.render(&diagnostic);
    assert!(rendered_output.contains(" --> example.cx:1:4"));
}

#[test]
fn render_primary_label_shows_snippet_and_caret() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("bad.cx", "fn {");
    let renderer = DiagnosticRenderer::new(&db);

    let diagnostic = Diagnostic::error("unexpected token").with_label(
        DiagnosticLabel::primary(
            FileSpan::new(file_id, Span::new(3, 4)),
            "expected identifier",
        ),
    );

    let rendered_output = renderer.render(&diagnostic);
    assert!(rendered_output.contains("1 | fn {"));
    assert!(rendered_output.contains("^ expected identifier"));
}

#[test]
fn render_secondary_label_uses_dash_markers() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("secondary.cx", "abc");
    let renderer = DiagnosticRenderer::new(&db);

    let diagnostic =
        Diagnostic::warning("related").with_label(DiagnosticLabel::secondary(
            FileSpan::new(file_id, Span::new(1, 2)),
            "related",
        ));

    let rendered_output = renderer.render(&diagnostic);
    assert!(rendered_output.contains("- related"));
    assert!(!rendered_output.contains("^ related"));
}

#[test]
fn render_zero_width_span_shows_single_marker() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("zero.cx", "abc");
    let renderer = DiagnosticRenderer::new(&db);

    let diagnostic = Diagnostic::error("missing token").with_label(
        DiagnosticLabel::primary_span(FileSpan::new(file_id, Span::new(2, 2))),
    );

    let rendered_output = renderer.render(&diagnostic);
    assert_eq!(rendered_output.matches('^').count(), 1);
}

#[test]
fn render_includes_label_message_note_and_help() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("notes.cx", "let x;");
    let renderer = DiagnosticRenderer::new(&db);

    let diagnostic = Diagnostic::error("unexpected token")
        .with_label(DiagnosticLabel::primary(
            FileSpan::new(file_id, Span::new(4, 5)),
            "here",
        ))
        .with_note("while parsing declaration")
        .with_help("insert a valid expression");

    let rendered_output = renderer.render(&diagnostic);
    assert!(rendered_output.contains("here"));
    assert!(rendered_output.contains("note: while parsing declaration"));
    assert!(rendered_output.contains("help: insert a valid expression"));
}

#[test]
fn render_all_joins_diagnostics_with_blank_line() {
    let db = SourceDb::new();
    let renderer = DiagnosticRenderer::new(&db);

    let diagnostics =
        vec![Diagnostic::error("first"), Diagnostic::warning("second")];
    let rendered_output = renderer.render_all(&diagnostics);

    assert_eq!(rendered_output, "error: first\n\nwarning: second");
}

#[test]
fn render_skips_unresolvable_labels_without_panicking() {
    let db = SourceDb::new();
    let renderer = DiagnosticRenderer::new(&db);

    let diagnostic =
        Diagnostic::error("missing file").with_label(DiagnosticLabel::primary(
            FileSpan::new(FileId::new(99), Span::new(0, 1)),
            "here",
        ));
    let rendered_output = renderer.render(&diagnostic);

    assert!(rendered_output.contains("error: missing file"));
    assert!(!rendered_output.contains(" --> "));
}
