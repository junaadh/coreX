use core_x::frontend::{
    FileParseError, ParseSession, ParseSessionError,
    source::{FileId, SourceDb},
};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn parse_session_new_exposes_db() {
    let mut db = SourceDb::new();
    db.add_file("one.cx", "fn f() {}");

    let session = ParseSession::new(db);
    assert_eq!(session.db().len(), 1);
}

#[test]
fn parse_session_into_db_round_trips_source_db() {
    let mut db = SourceDb::new();
    let id = db.add_file("round_trip.cx", "fn f() {}");

    let session = ParseSession::new(db);
    let round_tripped = session.into_db();
    let file = round_tripped.file(id).expect("file should exist");

    assert_eq!(round_tripped.len(), 1);
    assert_eq!(file.path(), Path::new("round_trip.cx"));
    assert_eq!(file.source(), "fn f() {}");
}

#[test]
fn parse_session_parse_file_success() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("ok.cx", "fn f() {}");
    let session = ParseSession::new(db);

    let parsed = session.parse_file(file_id).expect("parse should succeed");
    assert_eq!(parsed.file_id, file_id);
}

#[test]
fn parse_session_parse_file_reports_parse_error_with_file_id() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("bad.cx", "fn {");
    let session = ParseSession::new(db);

    let err = session.parse_file(file_id).expect_err("parse should fail");
    match err {
        ParseSessionError::Parse(FileParseError {
            file_id: err_id, ..
        }) => {
            assert_eq!(err_id, file_id);
        }
        other @ ParseSessionError::MissingFile { .. } => {
            panic!("expected parse error, got {other:?}");
        }
    }
}

#[test]
fn parse_session_parse_file_reports_missing_file() {
    let session = ParseSession::new(SourceDb::new());
    let missing = FileId::new(99);

    let err = session
        .parse_file(missing)
        .expect_err("file should be missing");
    assert_eq!(err, ParseSessionError::MissingFile { file_id: missing });
}

#[test]
fn parse_session_parse_all_files_success_in_insertion_order() {
    let mut db = SourceDb::new();
    let a = db.add_file("a.cx", "fn a() {}");
    let b = db.add_file("b.cx", "fn b() {}");
    let c = db.add_file("c.cx", "struct Foo {}");
    let session = ParseSession::new(db);

    let results = session.parse_all_files();
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(Result::is_ok));

    let file_ids = results
        .into_iter()
        .map(|res| res.expect("expected ok").file_id)
        .collect::<Vec<_>>();
    assert_eq!(file_ids, vec![a, b, c]);
}

#[test]
fn parse_session_parse_all_files_preserves_per_file_failures() {
    let mut db = SourceDb::new();
    let good_a = db.add_file("good_a.cx", "fn a() {}");
    let bad = db.add_file("bad.cx", "fn {");
    let good_b = db.add_file("good_b.cx", "struct Foo {}");
    let session = ParseSession::new(db);

    let results = session.parse_all_files();
    assert_eq!(results.len(), 3);

    match &results[0] {
        Ok(parsed) => assert_eq!(parsed.file_id, good_a),
        Err(err) => panic!("expected first parse ok, got {err:?}"),
    }
    match &results[1] {
        Err(FileParseError { file_id, .. }) => assert_eq!(*file_id, bad),
        Ok(parsed) => panic!("expected middle parse error, got {parsed:?}"),
    }
    match &results[2] {
        Ok(parsed) => assert_eq!(parsed.file_id, good_b),
        Err(err) => panic!("expected third parse ok, got {err:?}"),
    }
}

#[test]
fn parse_session_parses_all_example_files() {
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let entries = fs::read_dir(&examples_dir).unwrap_or_else(|err| {
        panic!(
            "failed to read examples directory {}: {err}",
            examples_dir.display()
        )
    });

    let mut paths = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("cx"))
        .collect::<Vec<PathBuf>>();
    paths.sort();

    assert!(
        !paths.is_empty(),
        "no .cx files found in examples directory {}",
        examples_dir.display()
    );

    let mut db = SourceDb::new();
    for path in &paths {
        let source = fs::read_to_string(path).unwrap_or_else(|err| {
            panic!("failed to read example {}: {err}", path.display())
        });
        db.add_file(path.clone(), source);
    }

    let session = ParseSession::new(db);
    let results = session.parse_all_files();
    assert_eq!(results.len(), paths.len());

    for result in results {
        if let Err(err) = result {
            let display_path = session.db().file(err.file_id).map_or_else(
                || format!("<missing file id {}>", err.file_id.raw()),
                |f| f.path().display().to_string(),
            );
            panic!("failed to parse example {}: {}", display_path, err.error);
        }
    }
}
