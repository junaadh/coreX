use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{ImportResolver, ScopeResolver};
use core_x::frontend::source::SourceDb;
use core_x::frontend::{
    DiagnosticRenderer, DiagnosticSeverity, ImportResolveError, ParsedFile,
    diagnostic_from_import_resolve_error,
};
use std::collections::BTreeMap;

fn add_and_parse(db: &mut SourceDb, path: &str, source: &str) -> ParsedFile {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(parsed.diagnostics.is_empty(), "expected clean parse");
    parsed
}

fn resolve_library_graph(
    db: &SourceDb,
    parsed_files: &[ParsedFile],
    root_file_id: core_x::frontend::source::FileId,
) -> core_x::frontend::ScopeGraph {
    ScopeResolver::new(db, parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope resolution should succeed")
}

#[test]
fn unknown_root_converts_to_structured_diagnostic() {
    let mut db = SourceDb::new();
    let importer = add_and_parse(&mut db, "src/importer.cx", "use missing::X;");
    let error = ImportResolveError::UnknownRoot {
        from_file_id: importer.file_id,
        root: "missing".to_string(),
    };

    let diagnostic = diagnostic_from_import_resolve_error(&db, &error);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "unknown import root");
    assert!(!diagnostic.labels.is_empty());
    assert!(
        diagnostic
            .notes
            .iter()
            .any(|note| note.contains("root, super"))
    );
}

#[test]
fn unresolved_path_converts_to_structured_diagnostic() {
    let mut db = SourceDb::new();
    let importer =
        add_and_parse(&mut db, "src/importer.cx", "use root::net::Missing;");
    let error = ImportResolveError::UnresolvedPath {
        from_file_id: importer.file_id,
        path: vec![
            "root".to_string(),
            "net".to_string(),
            "Missing".to_string(),
        ],
    };

    let diagnostic = diagnostic_from_import_resolve_error(&db, &error);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "unresolved import path");
    assert!(!diagnostic.labels.is_empty());
    assert!(
        diagnostic.labels[0]
            .message
            .as_ref()
            .is_some_and(|message| message.contains("root::net::Missing"))
    );
}

#[test]
fn duplicate_binding_converts_to_structured_diagnostic() {
    let mut db = SourceDb::new();
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use root::a::X; use root::b::X;",
    );
    let error = ImportResolveError::DuplicateBinding {
        file_id: importer.file_id,
        binding_name: "X".to_string(),
    };

    let diagnostic = diagnostic_from_import_resolve_error(&db, &error);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "duplicate imported binding");
    assert!(!diagnostic.labels.is_empty());
    assert!(
        diagnostic
            .notes
            .iter()
            .any(|note| note.contains("already introduced"))
    );
}

#[test]
fn invalid_glob_target_converts_to_structured_diagnostic() {
    let mut db = SourceDb::new();
    let importer =
        add_and_parse(&mut db, "src/importer.cx", "use root::Thing::*;");
    let error = ImportResolveError::InvalidGlobTarget {
        from_file_id: importer.file_id,
        path: vec!["root".to_string(), "Thing".to_string()],
    };

    let diagnostic = diagnostic_from_import_resolve_error(&db, &error);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "invalid glob import target");
    assert!(!diagnostic.labels.is_empty());
    assert!(
        diagnostic.labels[0]
            .message
            .as_ref()
            .is_some_and(|message| message.contains("glob import target"))
    );
}

#[test]
fn resolve_imports_with_diagnostics_continues_after_file_error() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let _net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use missing::Thing; use root::net::Client;",
    );
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            parse_source_file_from_source_file(file)
                .expect("parse should succeed")
        })
        .collect::<Vec<_>>();

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let empty = BTreeMap::new();
    let collector = ImportResolver::new(&graph, &parsed_files, &empty);
    let symbols = collector.collect_scope_symbols();
    let resolver = ImportResolver::new(&graph, &parsed_files, &symbols);
    let (imports, diagnostics) = resolver.resolve_imports_with_diagnostics(&db);

    assert_eq!(diagnostics.len(), 1);
    let importer_imports = imports
        .get(&importer.file_id)
        .expect("importer imports should exist");
    assert!(importer_imports.get("Client").is_some());
}

#[test]
fn resolve_imports_with_diagnostics_continues_across_files() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope net; scope importer_a; scope importer_b;",
    );
    let _net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let _importer_a =
        add_and_parse(&mut db, "src/importer_a.cx", "use missing::Thing;");
    let importer_b =
        add_and_parse(&mut db, "src/importer_b.cx", "use root::net::Client;");
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            parse_source_file_from_source_file(file)
                .expect("parse should succeed")
        })
        .collect::<Vec<_>>();

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let empty = BTreeMap::new();
    let collector = ImportResolver::new(&graph, &parsed_files, &empty);
    let symbols = collector.collect_scope_symbols();
    let resolver = ImportResolver::new(&graph, &parsed_files, &symbols);
    let (imports, diagnostics) = resolver.resolve_imports_with_diagnostics(&db);

    assert_eq!(diagnostics.len(), 1);
    let importer_b_imports = imports
        .get(&importer_b.file_id)
        .expect("importer_b imports should exist");
    assert!(importer_b_imports.get("Client").is_some());
}

#[test]
fn import_diagnostics_render_through_renderer() {
    let mut db = SourceDb::new();
    let importer = add_and_parse(&mut db, "src/importer.cx", "use missing::X;");
    let error = ImportResolveError::UnknownRoot {
        from_file_id: importer.file_id,
        root: "missing".to_string(),
    };
    let diagnostic = diagnostic_from_import_resolve_error(&db, &error);

    let rendered = DiagnosticRenderer::new(&db).render(&diagnostic);
    assert!(rendered.contains("error: unknown import root"));
    assert!(rendered.contains("src/importer.cx"));
}

#[test]
fn unloaded_dependency_root_converts_to_structured_diagnostic() {
    let mut db = SourceDb::new();
    let importer =
        add_and_parse(&mut db, "src/importer.cx", "use http::Client;");
    let error = ImportResolveError::UnloadedDependencyRoot {
        from_file_id: importer.file_id,
        root: "http".to_string(),
    };

    let diagnostic = diagnostic_from_import_resolve_error(&db, &error);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "dependency root is not loaded");
    assert!(!diagnostic.labels.is_empty());
    assert!(
        diagnostic.labels[0]
            .message
            .as_ref()
            .is_some_and(|message| message.contains("declared but not loaded"))
    );
}
