use core_x::frontend::ParsedFile;
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    ResolveError, ResolvedScopeKind, ScopeResolver,
};
use core_x::frontend::source::{FileId, SourceDb};

fn add_and_parse(db: &mut SourceDb, path: &str, source: &str) -> ParsedFile {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(
        parsed.diagnostics.is_empty(),
        "strict parse should not emit diagnostics"
    );
    parsed
}

fn parsed_by_id(parsed_files: &[ParsedFile], file_id: FileId) -> &ParsedFile {
    parsed_files
        .iter()
        .find(|parsed| parsed.file_id == file_id)
        .unwrap_or_else(|| panic!("missing parsed file id {}", file_id.raw()))
}

#[test]
fn resolve_root_with_single_file_backed_child_scope() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope util;");
    let util = add_and_parse(&mut db, "src/util.cx", "");
    let parsed_files = vec![root.clone(), util.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let graph = resolver
        .resolve_library_root(root.file_id)
        .expect("resolution should succeed");

    assert_eq!(graph.len(), 2);
    let root_scope = graph.scope(root.file_id).expect("root scope");
    assert_eq!(root_scope.child_scope_ids, vec![util.file_id]);
    let util_scope = graph.scope(util.file_id).expect("util scope");
    assert_eq!(util_scope.kind, ResolvedScopeKind::FileBacked);
    assert_eq!(util_scope.scope_path, vec!["util".to_string()]);
}

#[test]
fn resolve_root_with_single_directory_backed_child_scope() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net;");
    let net = add_and_parse(&mut db, "src/net/net.cx", "");
    let parsed_files = vec![root.clone(), net.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let graph = resolver
        .resolve_library_root(root.file_id)
        .expect("resolution should succeed");

    let net_scope = graph.scope(net.file_id).expect("net scope");
    assert_eq!(net_scope.kind, ResolvedScopeKind::DirectoryBacked);
    assert_eq!(net_scope.scope_path, vec!["net".to_string()]);
}

#[test]
fn resolve_nested_directory_backed_scope_chain() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net;");
    let net = add_and_parse(&mut db, "src/net/net.cx", "scope http;");
    let http = add_and_parse(&mut db, "src/net/http/http.cx", "");
    let parsed_files = vec![root.clone(), net.clone(), http.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let graph = resolver
        .resolve_library_root(root.file_id)
        .expect("resolution should succeed");

    let root_scope = graph.scope(root.file_id).expect("root scope");
    assert_eq!(root_scope.child_scope_ids, vec![net.file_id]);
    let net_scope = graph.scope(net.file_id).expect("net scope");
    assert_eq!(net_scope.child_scope_ids, vec![http.file_id]);
    let http_scope = graph.scope(http.file_id).expect("http scope");
    assert_eq!(
        http_scope.scope_path,
        vec!["net".to_string(), "http".to_string()]
    );
}

#[test]
fn resolve_missing_declared_scope_reports_error() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope missing;");
    let parsed_files = vec![root.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let error = resolver
        .resolve_library_root(root.file_id)
        .expect_err("resolution should fail");

    match error {
        ResolveError::MissingDeclaredScope {
            parent_file_id,
            declared_name,
            ..
        } => {
            assert_eq!(parent_file_id, root.file_id);
            assert_eq!(declared_name, "missing");
        }
        other => panic!("expected MissingDeclaredScope, got {other:?}"),
    }
}

#[test]
fn resolve_ambiguous_declared_scope_reports_error() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope foo;");
    let _foo_file = add_and_parse(&mut db, "src/foo.cx", "");
    let _foo_dir = add_and_parse(&mut db, "src/foo/foo.cx", "");
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            parse_source_file_from_source_file(file)
                .expect("parse should succeed")
        })
        .collect::<Vec<_>>();

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let error = resolver
        .resolve_library_root(root.file_id)
        .expect_err("resolution should fail");

    match error {
        ResolveError::AmbiguousDeclaredScope {
            parent_file_id,
            declared_name,
            ..
        } => {
            assert_eq!(parent_file_id, root.file_id);
            assert_eq!(declared_name, "foo");
        }
        other => panic!("expected AmbiguousDeclaredScope, got {other:?}"),
    }
}

#[test]
fn resolve_ignores_undeclared_files() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "");
    let orphan = add_and_parse(&mut db, "src/orphan.cx", "");
    let parsed_files = vec![root.clone(), orphan.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let graph = resolver
        .resolve_library_root(root.file_id)
        .expect("resolution should succeed");

    assert_eq!(graph.len(), 1);
    assert!(graph.scope(orphan.file_id).is_none());
}

#[test]
fn resolve_preserves_child_declaration_order() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope b; scope a;");
    let a = add_and_parse(&mut db, "src/a.cx", "");
    let b = add_and_parse(&mut db, "src/b.cx", "");
    let parsed_files = vec![root.clone(), a.clone(), b.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let graph = resolver
        .resolve_library_root(root.file_id)
        .expect("resolution should succeed");

    let root_scope = graph.scope(root.file_id).expect("root scope");
    assert_eq!(root_scope.child_scope_ids, vec![b.file_id, a.file_id]);
}

#[test]
fn resolve_binary_root_is_separate_from_library_root() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope util;");
    let util = add_and_parse(&mut db, "src/util.cx", "");
    let main = add_and_parse(&mut db, "src/main.cx", "");
    let parsed_files = vec![root.clone(), util.clone(), main.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let lib_graph = resolver
        .resolve_library_root(root.file_id)
        .expect("library resolution should succeed");
    let bin_graph = resolver
        .resolve_binary_root(main.file_id)
        .expect("binary resolution should succeed");

    assert_eq!(lib_graph.len(), 2);
    assert_eq!(bin_graph.len(), 1);
    assert!(bin_graph.scope(util.file_id).is_none());
}

#[test]
fn resolve_detects_scope_cycle() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope a;");
    let a = add_and_parse(&mut db, "src/a/a.cx", "scope a;");
    let parsed_files = vec![root.clone(), a.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let error = resolver
        .resolve_library_root(root.file_id)
        .expect_err("resolution should fail");

    match error {
        ResolveError::ScopeCycle { cycle } => {
            assert!(!cycle.is_empty());
            assert_eq!(cycle.first(), Some(&a.file_id));
            assert_eq!(cycle.last(), Some(&a.file_id));
        }
        other => panic!("expected ScopeCycle, got {other:?}"),
    }
}

#[test]
fn resolve_scope_paths_are_built_correctly() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net; scope util;");
    let net = add_and_parse(&mut db, "src/net/net.cx", "scope http;");
    let http = add_and_parse(&mut db, "src/net/http/http.cx", "");
    let util = add_and_parse(&mut db, "src/util.cx", "");
    let parsed_files =
        vec![root.clone(), net.clone(), http.clone(), util.clone()];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let graph = resolver
        .resolve_library_root(root.file_id)
        .expect("resolution should succeed");

    let root_scope = graph.scope(root.file_id).expect("root scope");
    let net_scope = graph.scope(net.file_id).expect("net scope");
    let http_scope = graph.scope(http.file_id).expect("http scope");
    let util_scope = graph.scope(util.file_id).expect("util scope");

    assert!(root_scope.scope_path.is_empty());
    assert_eq!(net_scope.scope_path, vec!["net".to_string()]);
    assert_eq!(
        http_scope.scope_path,
        vec!["net".to_string(), "http".to_string()]
    );
    assert_eq!(util_scope.scope_path, vec!["util".to_string()]);

    let root_parsed = parsed_by_id(&parsed_files, root.file_id);
    assert_eq!(root_parsed.ast.items.len(), 2);
}
