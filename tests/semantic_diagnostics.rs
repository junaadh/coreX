use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{ScopeResolver, resolve_project_imports};
use core_x::frontend::source::SourceDb;
use core_x::frontend::{DiagnosticRenderer, DiagnosticsBag, analyze_semantics};

fn parsed_to_desugared(
    parsed: core_x::frontend::ParsedFile,
) -> core_x::frontend::DesugaredFile {
    core_x::frontend::DesugaredFile {
        file_id: parsed.file_id,
        ast: parsed.ast,
        diagnostics: parsed.diagnostics,
        provenance_map: core_x::frontend::expansion::ProvenanceMap::new(
            parsed.file_id,
        ),
    }
}

fn semantic_diagnostics_for_source(source: &str) -> (SourceDb, DiagnosticsBag) {
    let mut db = SourceDb::new();
    let file_id = db.add_file("src/root.cx", source);
    let file = db.file(file_id).expect("source file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(parsed.diagnostics.is_empty(), "strict parse diagnostics");
    let parsed_files = vec![parsed_to_desugared(parsed)];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(file_id)
        .expect("scope graph");
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let analysis = analyze_semantics(&db, &graph, &parsed_files, &imports);
    (db, analysis.diagnostics)
}

fn has_message(diagnostics: &DiagnosticsBag, message: &str) -> bool {
    diagnostics
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.message == message)
}

#[test]
fn type_mismatch_converts_to_structured_diagnostic() {
    let (_, diagnostics) =
        semantic_diagnostics_for_source("fn f() -> i32 { let x: bool = 1; 0 }");
    assert!(has_message(&diagnostics, "type mismatch"));
}

#[test]
fn invalid_assignment_converts_to_structured_diagnostic() {
    let (_, diagnostics) = semantic_diagnostics_for_source(
        "fn g() -> i32 { 1 } fn f() { g = 1; }",
    );
    assert!(has_message(&diagnostics, "invalid assignment"));
}

#[test]
fn invalid_call_arity_converts_to_structured_diagnostic() {
    let (_, diagnostics) = semantic_diagnostics_for_source(
        "fn g(x: i32) -> i32 { x } fn f() { g(); }",
    );
    assert!(has_message(&diagnostics, "invalid call arity"));
}

#[test]
fn invalid_argument_type_converts_to_structured_diagnostic() {
    let (_, diagnostics) = semantic_diagnostics_for_source(
        "fn g(x: i32) -> i32 { x } fn f() { g(true); }",
    );
    assert!(has_message(&diagnostics, "invalid argument type"));
}

#[test]
fn invalid_return_type_converts_to_structured_diagnostic() {
    let (_, diagnostics) =
        semantic_diagnostics_for_source("fn f() -> i32 { return true; }");
    assert!(has_message(&diagnostics, "invalid return type"));
}

#[test]
fn invalid_condition_type_converts_to_structured_diagnostic() {
    let (_, diagnostics) =
        semantic_diagnostics_for_source("fn f() { while 1 { break; } }");
    assert!(has_message(&diagnostics, "invalid condition type"));
}

#[test]
fn mutability_violation_converts_to_structured_diagnostic() {
    let (_, diagnostics) =
        semantic_diagnostics_for_source("fn f() { let x = 1; x = 2; }");
    assert!(has_message(&diagnostics, "mutability violation"));
}

#[test]
fn unresolved_semantic_type_converts_to_structured_diagnostic() {
    let (_, diagnostics) = semantic_diagnostics_for_source(
        "fn f(x: Missing) -> Missing { return x; }",
    );
    assert!(has_message(&diagnostics, "unresolved semantic type"));
}

#[test]
fn semantic_diagnostics_render_through_renderer() {
    let (db, diagnostics) = semantic_diagnostics_for_source(
        "fn g(x: i32) -> i32 { x } fn f() -> i32 { let x: bool = 1; g(); true }",
    );
    let renderer = DiagnosticRenderer::new(&db);
    let rendered = renderer.render_all(diagnostics.as_slice());
    assert!(rendered.contains("type mismatch"));
    assert!(rendered.contains("invalid call arity"));
}

#[test]
fn semantic_diagnostics_accumulate_multiple_errors_in_one_file() {
    let (_, diagnostics) = semantic_diagnostics_for_source(
        "fn g(x: i32) -> i32 { x } fn f() -> i32 { let x: bool = 1; g(); while 1 { break; } true }",
    );
    assert!(diagnostics.len() >= 3);
    assert!(has_message(&diagnostics, "type mismatch"));
    assert!(has_message(&diagnostics, "invalid call arity"));
    assert!(has_message(&diagnostics, "invalid condition type"));
}
