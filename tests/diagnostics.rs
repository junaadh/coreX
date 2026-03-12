use core_x::frontend::ast::Span;
use core_x::frontend::source::FileId;
use core_x::frontend::{
    Diagnostic, DiagnosticLabel, DiagnosticLabelKind, DiagnosticSeverity,
    DiagnosticsBag, FileSpan,
};

#[test]
fn file_span_new_stores_file_id_and_span() {
    let file_id = FileId::new(3);
    let span = Span::new(10, 20);

    let file_span = FileSpan::new(file_id, span);
    assert_eq!(file_span.file_id, file_id);
    assert_eq!(file_span.span, span);
}

#[test]
fn diagnostic_label_primary_sets_kind_message_and_span() {
    let span = FileSpan::new(FileId::new(0), Span::new(1, 4));
    let label = DiagnosticLabel::primary(span, "here");

    assert_eq!(label.kind, DiagnosticLabelKind::Primary);
    assert_eq!(label.span, span);
    assert_eq!(label.message, Some("here".to_string()));
}

#[test]
fn diagnostic_label_primary_span_omits_message() {
    let span = FileSpan::new(FileId::new(1), Span::new(2, 5));
    let label = DiagnosticLabel::primary_span(span);

    assert_eq!(label.kind, DiagnosticLabelKind::Primary);
    assert_eq!(label.span, span);
    assert_eq!(label.message, None);
}

#[test]
fn diagnostic_label_secondary_sets_kind_message_and_span() {
    let span = FileSpan::new(FileId::new(2), Span::new(7, 9));
    let label = DiagnosticLabel::secondary(span, "related");

    assert_eq!(label.kind, DiagnosticLabelKind::Secondary);
    assert_eq!(label.span, span);
    assert_eq!(label.message, Some("related".to_string()));
}

#[test]
fn diagnostic_label_secondary_span_omits_message() {
    let span = FileSpan::new(FileId::new(4), Span::new(11, 12));
    let label = DiagnosticLabel::secondary_span(span);

    assert_eq!(label.kind, DiagnosticLabelKind::Secondary);
    assert_eq!(label.span, span);
    assert_eq!(label.message, None);
}

#[test]
fn diagnostic_new_initializes_empty_collections() {
    let diagnostic = Diagnostic::new(DiagnosticSeverity::Error, "unexpected");

    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "unexpected");
    assert!(diagnostic.labels.is_empty());
    assert!(diagnostic.notes.is_empty());
    assert!(diagnostic.help.is_none());
}

#[test]
fn diagnostic_error_constructor_sets_error_severity() {
    let diagnostic = Diagnostic::error("unexpected token");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
}

#[test]
fn diagnostic_warning_constructor_sets_warning_severity() {
    let diagnostic = Diagnostic::warning("unused binding");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
}

#[test]
fn diagnostic_note_constructor_sets_note_severity() {
    let diagnostic = Diagnostic::note("while parsing declaration");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Note);
}

#[test]
fn diagnostic_help_diag_constructor_sets_help_severity() {
    let diagnostic = Diagnostic::help_diag("try adding a semicolon");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Help);
}

#[test]
fn diagnostic_with_label_appends_label() {
    let label = DiagnosticLabel::primary(
        FileSpan::new(FileId::new(0), Span::new(0, 1)),
        "here",
    );
    let diagnostic = Diagnostic::error("oops").with_label(label.clone());

    assert_eq!(diagnostic.labels.len(), 1);
    assert_eq!(diagnostic.labels[0], label);
}

#[test]
fn diagnostic_with_labels_preserves_order() {
    let first = DiagnosticLabel::primary(
        FileSpan::new(FileId::new(0), Span::new(1, 2)),
        "first",
    );
    let second = DiagnosticLabel::secondary(
        FileSpan::new(FileId::new(0), Span::new(3, 4)),
        "second",
    );
    let diagnostic = Diagnostic::error("ordered")
        .with_labels([first.clone(), second.clone()]);

    assert_eq!(diagnostic.labels, vec![first, second]);
}

#[test]
fn diagnostic_with_note_appends_note() {
    let diagnostic = Diagnostic::warning("warn").with_note("note one");
    assert_eq!(diagnostic.notes, vec!["note one".to_string()]);
}

#[test]
fn diagnostic_with_notes_preserves_order() {
    let diagnostic =
        Diagnostic::note("note").with_notes(["first note", "second note"]);
    assert_eq!(
        diagnostic.notes,
        vec!["first note".to_string(), "second note".to_string()]
    );
}

#[test]
fn diagnostic_with_help_sets_help_text() {
    let diagnostic = Diagnostic::error("err").with_help("insert `}`");
    assert_eq!(diagnostic.help, Some("insert `}`".to_string()));
}

#[test]
fn diagnostics_bag_new_is_empty() {
    let bag = DiagnosticsBag::new();
    assert!(bag.is_empty());
    assert_eq!(bag.len(), 0);
    assert!(bag.as_slice().is_empty());
}

#[test]
fn diagnostics_bag_push_increases_len() {
    let mut bag = DiagnosticsBag::new();
    bag.push(Diagnostic::error("first"));

    assert_eq!(bag.len(), 1);
    assert!(!bag.is_empty());
}

#[test]
fn diagnostics_bag_extend_preserves_order() {
    let first = Diagnostic::error("first");
    let second = Diagnostic::warning("second");

    let mut bag = DiagnosticsBag::new();
    bag.extend([first.clone(), second.clone()]);

    assert_eq!(bag.as_slice(), &[first, second]);
}

#[test]
fn diagnostics_bag_into_vec_round_trips() {
    let first = Diagnostic::error("first");
    let second = Diagnostic::note("second");

    let mut bag = DiagnosticsBag::new();
    bag.push(first.clone());
    bag.push(second.clone());

    let diagnostics = bag.into_vec();
    assert_eq!(diagnostics, vec![first, second]);
}
