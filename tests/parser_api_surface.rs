use core_x::frontend::ParseSession;
use core_x::frontend::parser::{
    parse_source_file, parse_source_file_from_source_file,
    parse_source_file_from_source_file_with_recovery,
    parse_source_file_with_recovery,
};
use core_x::frontend::source::SourceDb;

#[test]
fn strict_string_and_source_file_entry_points_match_on_valid_input() {
    let source = "fn a() {} struct Foo {}";
    let string_result =
        parse_source_file(source).expect("string parse should succeed");

    let mut db = SourceDb::new();
    let file_id = db.add_file("valid.cx", source);
    let file = db.file(file_id).expect("file should exist");
    let file_result = parse_source_file_from_source_file(file)
        .expect("file parse should succeed");

    assert_eq!(string_result.ast.items.len(), file_result.ast.items.len());
    assert!(string_result.diagnostics.is_empty());
    assert!(file_result.diagnostics.is_empty());
}

#[test]
fn recovery_string_and_source_file_entry_points_match_on_valid_input() {
    let source = "fn a() {} struct Foo {}";
    let string_report = parse_source_file_with_recovery(source)
        .expect("string recovery should succeed");

    let mut db = SourceDb::new();
    let file_id = db.add_file("valid.cx", source);
    let file = db.file(file_id).expect("file should exist");
    let file_report = parse_source_file_from_source_file_with_recovery(file)
        .expect("file recovery should succeed");

    assert_eq!(string_report.ast.items.len(), file_report.ast.items.len());
    assert!(string_report.diagnostics.is_empty());
    assert!(file_report.diagnostics.is_empty());
}

#[test]
fn recovery_string_and_source_file_entry_points_match_diagnostic_counts_on_invalid_input()
 {
    let source = "fn {";
    let string_report = parse_source_file_with_recovery(source)
        .expect("string recovery should succeed");

    let mut db = SourceDb::new();
    let file_id = db.add_file("invalid.cx", source);
    let file = db.file(file_id).expect("file should exist");
    let file_report = parse_source_file_from_source_file_with_recovery(file)
        .expect("file recovery should succeed");

    assert_eq!(
        string_report.diagnostics.len(),
        file_report.diagnostics.len()
    );
    assert_eq!(string_report.ast.items.len(), file_report.ast.items.len());
}

#[test]
fn parse_session_uses_normalized_file_based_entry_points_consistently() {
    let mut db = SourceDb::new();
    let valid_id = db.add_file("valid.cx", "fn f() {}");
    let invalid_id = db.add_file("invalid.cx", "fn {");
    let session = ParseSession::new(db);

    let strict_valid = session.parse_file(valid_id);
    assert!(strict_valid.is_ok());

    let strict_invalid = session.parse_file(invalid_id);
    assert!(strict_invalid.is_err());

    let recovery_valid = session
        .parse_file_with_recovery(valid_id)
        .expect("valid recovery parse should succeed");
    assert!(recovery_valid.diagnostics.is_empty());

    let recovery_invalid = session
        .parse_file_with_recovery(invalid_id)
        .expect("invalid recovery parse should still succeed");
    assert!(!recovery_invalid.diagnostics.is_empty());
}
