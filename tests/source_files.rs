use core_x::frontend::ast::Span;
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::source::{
    FileId, LineCol, LineIndex, SourceDb, SourceFile,
};
use std::path::PathBuf;

#[test]
fn line_index_empty_source_has_single_zero_start() {
    let index = LineIndex::new("");
    assert_eq!(index.line_count(), 1);
    assert_eq!(index.line_starts(), &[0]);
    assert_eq!(index.line_start(0), Some(0));
    assert_eq!(index.line_start(1), None);
}

#[test]
fn line_index_single_line() {
    let index = LineIndex::new("hello");
    assert_eq!(index.line_count(), 1);
    assert_eq!(index.line_starts(), &[0]);
    assert_eq!(index.line_start(0), Some(0));
}

#[test]
fn line_index_multiple_lines() {
    let index = LineIndex::new("ab\ncd\nef");
    assert_eq!(index.line_count(), 3);
    assert_eq!(index.line_starts(), &[0, 3, 6]);
    assert_eq!(index.line_start(0), Some(0));
    assert_eq!(index.line_start(1), Some(3));
    assert_eq!(index.line_start(2), Some(6));
    assert_eq!(index.line_start(3), None);
}

#[test]
fn line_index_line_col_maps_offsets_correctly() {
    let index = LineIndex::new("ab\ncd\nef");

    assert_eq!(index.line_col(0), Some(LineCol { line: 0, column: 0 }));
    assert_eq!(index.line_col(2), Some(LineCol { line: 0, column: 2 }));
    assert_eq!(index.line_col(3), Some(LineCol { line: 1, column: 0 }));
    assert_eq!(index.line_col(4), Some(LineCol { line: 1, column: 1 }));
    assert_eq!(index.line_col(6), Some(LineCol { line: 2, column: 0 }));
    assert_eq!(index.line_col(8), Some(LineCol { line: 2, column: 2 }));
    assert_eq!(index.line_col(9), None);
}

#[test]
fn source_file_slice_valid_span() {
    let file = SourceFile::new(
        FileId::new(0),
        PathBuf::from("examples/hello_world.cx"),
        "fn main() {}".to_string(),
    );

    let span = Span::new(3, 7);
    assert_eq!(file.slice(span), Some("main"));
}

#[test]
fn source_file_slice_rejects_invalid_span() {
    let file = SourceFile::new(
        FileId::new(1),
        PathBuf::from("examples/unicode.cx"),
        "aéz".to_string(),
    );

    assert_eq!(file.slice(Span::new(3, 2)), None);
    assert_eq!(file.slice(Span::new(0, 99)), None);
    assert_eq!(file.slice(Span::new(2, 3)), None);
}

#[test]
fn source_db_add_file_returns_stable_ids() {
    let mut db = SourceDb::new();
    let a = db.add_file("a.cx", "fn a() {}");
    let b = db.add_file("b.cx", "fn b() {}");
    let c = db.add_file("c.cx", "fn c() {}");

    assert_eq!(a, FileId::new(0));
    assert_eq!(b, FileId::new(1));
    assert_eq!(c, FileId::new(2));
}

#[test]
fn source_db_retrieves_files_by_id() {
    let mut db = SourceDb::new();
    let id = db.add_file("examples/sample.cx", "fn sample() {}");

    let file = db.file(id).expect("file should exist");
    assert_eq!(file.id(), id);
    assert_eq!(file.path(), PathBuf::from("examples/sample.cx").as_path());
    assert_eq!(file.source(), "fn sample() {}");
}

#[test]
fn source_db_preserves_insertion_order() {
    let mut db = SourceDb::new();
    db.add_file("a.cx", "fn a() {}");
    db.add_file("b.cx", "fn b() {}");
    db.add_file("c.cx", "fn c() {}");

    let paths = db
        .files()
        .iter()
        .map(|file| file.path().to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(paths, vec!["a.cx", "b.cx", "c.cx"]);
}

#[test]
fn parse_source_file_from_source_file_parses_valid_file() {
    let mut db = SourceDb::new();
    let id = db.add_file("example.cx", "use core::fmt;\nfn f() {}");
    let file = db.file(id).expect("file should exist");

    let parsed = parse_source_file_from_source_file(file);
    assert!(parsed.is_ok(), "expected parse success, got: {parsed:?}");
}

#[test]
fn parse_source_file_from_source_db_file_parses_multiple_files() {
    let mut db = SourceDb::new();
    let first = db.add_file("first.cx", "fn f() {}");
    let second = db.add_file("second.cx", "struct Foo {}");

    let parsed_first = parse_source_file_from_source_file(
        db.file(first).expect("first file should exist"),
    );
    let parsed_second = parse_source_file_from_source_file(
        db.file(second).expect("second file should exist"),
    );

    assert!(
        parsed_first.is_ok(),
        "expected first parse success, got: {parsed_first:?}"
    );
    assert!(
        parsed_second.is_ok(),
        "expected second parse success, got: {parsed_second:?}"
    );
}
