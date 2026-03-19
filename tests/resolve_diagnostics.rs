use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::ScopeResolver;
use core_x::frontend::source::SourceDb;
use core_x::frontend::{
    DesugaredFile, DiagnosticRenderer, DiagnosticSeverity,
    diagnostic_from_resolve_error,
};

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

fn add_and_parse(db: &mut SourceDb, path: &str, source: &str) -> DesugaredFile {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(parsed.diagnostics.is_empty(), "expected clean parse");
    parsed_to_desugared(parsed)
}

#[test]
fn missing_declared_scope_converts_to_structured_diagnostic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope missing;");
    let parsed_files = vec![root.clone()];

    let error = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root.file_id)
        .expect_err("expected resolve error");
    let diagnostic = diagnostic_from_resolve_error(&db, &error);

    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "missing declared scope");
    assert!(!diagnostic.labels.is_empty());
    assert!(
        diagnostic
            .notes
            .iter()
            .any(|note| note.contains("src/missing.cx"))
    );
    assert!(
        diagnostic
            .notes
            .iter()
            .any(|note| note.contains("src/missing/missing.cx"))
    );
}

#[test]
fn ambiguous_declared_scope_converts_to_structured_diagnostic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope foo;");
    let _foo_file = add_and_parse(&mut db, "src/foo.cx", "");
    let _foo_dir = add_and_parse(&mut db, "src/foo/foo.cx", "");
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            let parsed = parse_source_file_from_source_file(file)
                .expect("parse should succeed");
            parsed_to_desugared(parsed)
        })
        .collect::<Vec<_>>();

    let error = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root.file_id)
        .expect_err("expected resolve error");
    let diagnostic = diagnostic_from_resolve_error(&db, &error);

    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "ambiguous declared scope");
    assert!(!diagnostic.labels.is_empty());
    assert!(
        diagnostic
            .notes
            .iter()
            .any(|note| note.contains("src/foo.cx"))
    );
    assert!(
        diagnostic
            .notes
            .iter()
            .any(|note| note.contains("src/foo/foo.cx"))
    );
}

#[test]
fn scope_cycle_converts_to_structured_diagnostic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope a;");
    let _a = add_and_parse(&mut db, "src/a/a.cx", "scope a;");
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            let parsed = parse_source_file_from_source_file(file)
                .expect("parse should succeed");
            parsed_to_desugared(parsed)
        })
        .collect::<Vec<_>>();

    let error = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root.file_id)
        .expect_err("expected resolve error");
    let diagnostic = diagnostic_from_resolve_error(&db, &error);

    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "scope cycle detected");
    assert!(!diagnostic.notes.is_empty());
    assert!(diagnostic.notes.iter().any(|note| note.contains("cycle:")));
}

#[test]
fn resolve_with_diagnostics_skips_missing_child_and_continues() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope missing; scope util;");
    let util = add_and_parse(&mut db, "src/util.cx", "struct Helper {}");
    let parsed_files = vec![root.clone(), util.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let (graph, diagnostics) =
        resolver.resolve_library_root_with_diagnostics(root.file_id, &db);
    let graph = graph.expect("graph should still resolve");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(graph.len(), 2);
    let root_scope = graph.scope(root.file_id).expect("root scope");
    assert_eq!(root_scope.child_scope_ids, vec![util.file_id]);
}

#[test]
fn resolve_with_diagnostics_skips_ambiguous_child_and_continues() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope foo; scope util;");
    let _foo_file = add_and_parse(&mut db, "src/foo.cx", "");
    let _foo_dir = add_and_parse(&mut db, "src/foo/foo.cx", "");
    let util = add_and_parse(&mut db, "src/util.cx", "");
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            let parsed = parse_source_file_from_source_file(file)
                .expect("parse should succeed");
            parsed_to_desugared(parsed)
        })
        .collect::<Vec<_>>();

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let (graph, diagnostics) =
        resolver.resolve_library_root_with_diagnostics(root.file_id, &db);
    let graph = graph.expect("graph should still resolve");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(graph.len(), 2);
    let root_scope = graph.scope(root.file_id).expect("root scope");
    assert_eq!(root_scope.child_scope_ids, vec![util.file_id]);
}

#[test]
fn resolve_with_diagnostics_returns_none_on_cycle() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope a;");
    let _a = add_and_parse(&mut db, "src/a/a.cx", "scope a;");
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            let parsed = parse_source_file_from_source_file(file)
                .expect("parse should succeed");
            parsed_to_desugared(parsed)
        })
        .collect::<Vec<_>>();

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let (graph, diagnostics) =
        resolver.resolve_library_root_with_diagnostics(root.file_id, &db);

    assert!(graph.is_none(), "cycle should prevent graph construction");
    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics.as_slice()[0]
            .message
            .contains("scope cycle detected")
    );
}

#[test]
fn resolve_diagnostics_render_through_renderer() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope missing;");
    let parsed_files = vec![root.clone()];

    let error = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root.file_id)
        .expect_err("expected resolve error");
    let diagnostic = diagnostic_from_resolve_error(&db, &error);
    let rendered = DiagnosticRenderer::new(&db).render(&diagnostic);

    assert!(rendered.contains("error: missing declared scope"));
    assert!(rendered.contains("src/root.cx"));
}
