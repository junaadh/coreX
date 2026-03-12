use core_x::frontend::source::SourceDb;
use core_x::frontend::{
    DiagnosticLabelKind, DiagnosticRenderer, DiagnosticSeverity,
    FileParseError, ParseSession, ParseSessionError,
    diagnostic_from_file_parse_error,
};

fn parse_error_for_source(
    path: &str,
    source: &str,
) -> (SourceDb, core_x::frontend::source::FileId, FileParseError) {
    let mut db = SourceDb::new();
    let file_id = db.add_file(path, source);
    let session = ParseSession::new(db);
    let error = match session.parse_file(file_id) {
        Err(ParseSessionError::Parse(error)) => error,
        other => panic!("expected parse error, got {other:?}"),
    };
    (session.into_db(), file_id, error)
}

#[test]
fn diagnostic_from_unexpected_token_parse_error() {
    let (_, _, file_error) = parse_error_for_source("bad.cx", "fn {");
    let diagnostic = diagnostic_from_file_parse_error(&file_error);

    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert!(
        diagnostic.message == "unexpected token"
            || diagnostic.message == "parse failed"
    );
    assert!(!diagnostic.labels.is_empty());
}

#[test]
fn diagnostic_from_unexpected_eof_parse_error() {
    let (_, _, file_error) = parse_error_for_source("eof.cx", "struct Foo {");
    let diagnostic = diagnostic_from_file_parse_error(&file_error);

    assert!(
        diagnostic.message == "unexpected end of file"
            || diagnostic.message == "parse failed"
    );
}

#[test]
fn diagnostic_from_file_parse_error_preserves_file_id() {
    let (_, file_id, file_error) = parse_error_for_source("id.cx", "fn {");
    let diagnostic = diagnostic_from_file_parse_error(&file_error);

    let primary = diagnostic
        .labels
        .iter()
        .find(|label| label.kind == DiagnosticLabelKind::Primary)
        .unwrap_or_else(|| panic!("expected primary label in diagnostic"));
    assert_eq!(primary.span.file_id, file_id);
}

#[test]
fn parse_session_error_can_be_rendered_for_invalid_source() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("bad.cx", "fn {");
    let session = ParseSession::new(db);

    let file_error = match session.parse_file(file_id) {
        Err(ParseSessionError::Parse(error)) => error,
        other => panic!("expected parse error, got {other:?}"),
    };

    let diagnostic = diagnostic_from_file_parse_error(&file_error);
    let rendered = DiagnosticRenderer::new(session.db()).render(&diagnostic);

    assert!(rendered.contains("bad.cx"));
    assert!(rendered.contains("error:"));
    assert!(
        rendered.contains("unexpected token")
            || rendered.contains("unexpected end of file")
            || rendered.contains("parse failed")
            || rendered.contains("lexing failed")
    );
}

#[test]
fn rendered_parse_diagnostic_mentions_expected_text() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("expected.cx", "fn {");
    let session = ParseSession::new(db);

    let file_error = match session.parse_file(file_id) {
        Err(ParseSessionError::Parse(error)) => error,
        other => panic!("expected parse error, got {other:?}"),
    };

    let diagnostic = diagnostic_from_file_parse_error(&file_error);
    let rendered = DiagnosticRenderer::new(session.db()).render(&diagnostic);

    assert!(rendered.contains("expected "));
}
