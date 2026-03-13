use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{ScopeResolver, resolve_project_imports};
use core_x::frontend::source::SourceDb;
use core_x::frontend::{Diagnostic, DiagnosticsBag, analyze_semantics};
use std::collections::BTreeMap;

fn semantic_diagnostics_for_source(source: &str) -> DiagnosticsBag {
    let mut db = SourceDb::new();
    let file_id = db.add_file("src/root.cx", source);
    let file = db.file(file_id).expect("source file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(parsed.diagnostics.is_empty(), "strict parse diagnostics");
    let parsed_files = vec![parsed];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(file_id)
        .expect("scope graph");
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    analyze_semantics(&db, &graph, &parsed_files, &imports).diagnostics
}

fn has_message(diagnostics: &DiagnosticsBag, message: &str) -> bool {
    diagnostics
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.message == message)
}

fn message_counts(diagnostics: &DiagnosticsBag) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for diagnostic in diagnostics.as_slice() {
        *counts.entry(diagnostic.message.clone()).or_insert(0) += 1;
    }
    counts
}

fn duplicate_diagnostic_count(diagnostics: &[Diagnostic]) -> usize {
    let mut duplicates = 0usize;
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if diagnostics[..index].contains(diagnostic) {
            duplicates = duplicates.saturating_add(1);
        }
    }
    duplicates
}

#[test]
fn multiple_semantic_errors_reported_from_one_body() {
    let diagnostics = semantic_diagnostics_for_source(
        "fn g(x: i32) -> i32 { x } fn f() -> i32 { let x: bool = 1; g(); while 1 { break; } true }",
    );

    assert!(diagnostics.len() >= 4);
    assert!(has_message(&diagnostics, "type mismatch"));
    assert!(has_message(&diagnostics, "invalid call arity"));
    assert!(has_message(&diagnostics, "invalid condition type"));
}

#[test]
fn later_statements_still_checked_after_earlier_error() {
    let diagnostics = semantic_diagnostics_for_source(
        "fn g(x: i32) -> i32 { x } fn f() { g(); while 1 { break; } }",
    );

    assert!(has_message(&diagnostics, "invalid call arity"));
    assert!(has_message(&diagnostics, "invalid condition type"));
}

#[test]
fn diagnostics_deduplicate_repeated_root_cause_reports() {
    let diagnostics =
        semantic_diagnostics_for_source("fn f() -> i32 { return true; }");
    let counts = message_counts(&diagnostics);
    assert_eq!(counts.get("invalid return type"), Some(&1));
    assert_eq!(duplicate_diagnostic_count(diagnostics.as_slice()), 0);
}

#[test]
fn semantic_recovery_diagnostics_are_deterministic() {
    let source = "fn g(x: i32) -> i32 { x } fn f() -> i32 { let x: bool = 1; g(); while 1 { break; } true }";
    let first = semantic_diagnostics_for_source(source);
    let second = semantic_diagnostics_for_source(source);
    assert_eq!(first, second);
}
