use core_x::frontend::ast::Item;
use core_x::frontend::parser::{
    parse_source_file_from_source_file_with_recovery,
    parse_source_file_with_recovery,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{ParseSession, ParseSessionError};

#[test]
fn parse_source_file_from_source_file_with_recovery_uses_real_file_id_in_diagnostics()
 {
    let mut db = SourceDb::new();
    let file_id = db.add_file("bad.cx", "fn {");
    let file = db.file(file_id).expect("file should exist");

    let parsed = parse_source_file_from_source_file_with_recovery(file)
        .expect("recovery parse should succeed");
    assert!(!parsed.diagnostics.is_empty());

    let first_diag = parsed
        .diagnostics
        .as_slice()
        .first()
        .expect("expected at least one diagnostic");
    let first_label = first_diag
        .labels
        .first()
        .expect("expected at least one label in first diagnostic");
    assert_eq!(first_label.span.file_id, file.id());
}

#[test]
fn parse_source_file_with_recovery_without_file_id_uses_zero_file_id() {
    let parsed = parse_source_file_with_recovery("fn {")
        .expect("recovery parse should succeed");
    assert!(!parsed.diagnostics.is_empty());

    let first_diag = parsed
        .diagnostics
        .as_slice()
        .first()
        .expect("expected at least one diagnostic");
    let first_label = first_diag
        .labels
        .first()
        .expect("expected at least one label in first diagnostic");
    assert_eq!(first_label.span.file_id, FileId::new(0));
}

#[test]
fn parse_session_parse_file_with_recovery_success() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("ok.cx", "fn f() {}");
    let session = ParseSession::new(db);

    let parsed = session
        .parse_file_with_recovery(file_id)
        .expect("recovery parse should succeed");

    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.ast.items.len(), 1);
}

#[test]
fn parse_session_parse_file_with_recovery_preserves_real_file_id_in_diagnostics()
 {
    let mut db = SourceDb::new();
    let file_id = db.add_file("bad.cx", "fn {");
    let session = ParseSession::new(db);

    let parsed = session
        .parse_file_with_recovery(file_id)
        .expect("recovery parse should succeed");
    assert!(!parsed.diagnostics.is_empty());

    let first_diag = parsed
        .diagnostics
        .as_slice()
        .first()
        .expect("expected at least one diagnostic");
    let first_label = first_diag
        .labels
        .first()
        .expect("expected at least one label in first diagnostic");
    assert_eq!(first_label.span.file_id, file_id);
}

#[test]
fn parse_session_parse_file_with_recovery_reports_missing_file() {
    let session = ParseSession::new(SourceDb::new());
    let missing = FileId::new(99);

    let error = session
        .parse_file_with_recovery(missing)
        .expect_err("missing file should return an error");
    assert_eq!(error, ParseSessionError::MissingFile { file_id: missing });
}

#[test]
fn parse_session_parse_all_files_with_recovery_preserves_insertion_order() {
    let mut db = SourceDb::new();
    db.add_file("first.cx", "fn first() {}");
    db.add_file("second.cx", "fn second() {}");
    db.add_file("third.cx", "fn third() {}");
    let session = ParseSession::new(db);

    let results = session.parse_all_files_with_recovery();
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(Result::is_ok));

    let expected_names = session
        .db()
        .files()
        .iter()
        .map(|file| {
            file.path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("file stem should be utf-8")
                .to_string()
        })
        .collect::<Vec<_>>();

    let parsed_names = results
        .iter()
        .map(|result| {
            let report = result.as_ref().expect("expected successful parse");
            let first_item = report
                .ast
                .items
                .first()
                .expect("expected one top-level item");
            match &first_item.node {
                Item::Function(function) => function.node.name.clone(),
                other => panic!("expected function item, got {other:?}"),
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(parsed_names, expected_names);
}

#[test]
fn parse_session_parse_all_files_with_recovery_preserves_per_file_failures() {
    let mut db = SourceDb::new();
    db.add_file("ok_a.cx", "fn a() {}");
    db.add_file("bad.cx", "fn {");
    db.add_file("ok_c.cx", "fn c() {}");
    let session = ParseSession::new(db);

    let results = session.parse_all_files_with_recovery();
    assert_eq!(results.len(), 3);
    assert!(results[0].as_ref().is_ok());
    assert!(results[2].as_ref().is_ok());

    match &results[1] {
        Ok(report) => assert!(!report.diagnostics.is_empty()),
        Err(error) => {
            panic!("expected recovery report in middle slot, got {error:?}")
        }
    }
}

#[test]
fn parse_session_parse_all_files_with_recovery_reports_real_file_ids_in_diagnostics()
 {
    let mut db = SourceDb::new();
    db.add_file("bad_one.cx", "fn {");
    db.add_file("bad_two.cx", "struct {");
    let session = ParseSession::new(db);

    let results = session.parse_all_files_with_recovery();
    assert_eq!(results.len(), 2);

    for (index, result) in results.iter().enumerate() {
        let report = result.as_ref().expect("expected recovery parse report");
        let file_id = session.db().files()[index].id();

        for diagnostic in report.diagnostics.as_slice() {
            if diagnostic.labels.is_empty() {
                continue;
            }
            for label in &diagnostic.labels {
                assert_eq!(label.span.file_id, file_id);
            }
        }
    }
}
