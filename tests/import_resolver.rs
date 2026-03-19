use core_x::frontend::DesugaredFile;
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    ImportBindingKind, ImportResolveError, ImportResolver, ResolvedScopeKind,
    ScopeResolver, SymbolKind, resolve_project_imports,
};
use core_x::frontend::source::{FileId, SourceDb};
use std::collections::BTreeMap;

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
    assert!(
        parsed.diagnostics.is_empty(),
        "strict parse should not emit diagnostics"
    );
    parsed_to_desugared(parsed)
}

fn resolve_library_graph(
    db: &SourceDb,
    parsed_files: &[DesugaredFile],
    root_file_id: FileId,
) -> core_x::frontend::ScopeGraph {
    ScopeResolver::new(db, parsed_files)
        .resolve_library_root(root_file_id)
        .expect("library scope resolution should succeed")
}

#[test]
fn collect_scope_symbols_from_root_and_children() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope net; fn top() {} struct S {}",
    );
    let net = add_and_parse(&mut db, "src/net.cx", "fn child_fn() {}");
    let parsed_files = vec![root.clone(), net.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let empty = BTreeMap::new();
    let resolver = ImportResolver::new(&graph, &parsed_files, &empty);
    let symbols = resolver.collect_scope_symbols();

    let root_symbols = symbols.get(&root.file_id).expect("root symbols");
    assert_eq!(
        root_symbols.get("net").map(|symbol| symbol.kind),
        Some(SymbolKind::Scope)
    );
    assert_eq!(
        root_symbols.get("top").map(|symbol| symbol.kind),
        Some(SymbolKind::Function)
    );
    assert_eq!(
        root_symbols.get("S").map(|symbol| symbol.kind),
        Some(SymbolKind::Struct)
    );

    let net_symbols = symbols.get(&net.file_id).expect("net symbols");
    assert_eq!(
        net_symbols.get("child_fn").map(|symbol| symbol.kind),
        Some(SymbolKind::Function)
    );
}

#[test]
fn resolve_simple_root_import() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let importer =
        add_and_parse(&mut db, "src/importer.cx", "use root::net::Client;");
    let parsed_files = vec![root.clone(), net.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let importer_imports = imports
        .get(&importer.file_id)
        .expect("importer imports should exist");
    let binding = importer_imports.get("Client").expect("Client binding");

    assert_eq!(binding.local_name, "Client");
    assert_eq!(binding.kind, ImportBindingKind::Symbol(SymbolKind::Struct));
    assert_eq!(binding.target_file_id, net.file_id);
    assert_eq!(
        binding.target_path,
        vec!["net".to_string(), "Client".to_string()]
    );
}

#[test]
fn resolve_alias_import() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use root::net::Client as RemoteClient;",
    );
    let parsed_files = vec![root.clone(), net.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let importer_imports = imports
        .get(&importer.file_id)
        .expect("importer imports should exist");
    let binding = importer_imports
        .get("RemoteClient")
        .expect("RemoteClient binding");

    assert_eq!(binding.kind, ImportBindingKind::Symbol(SymbolKind::Struct));
    assert_eq!(binding.target_file_id, net.file_id);
}

#[test]
fn resolve_group_import() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let net = add_and_parse(
        &mut db,
        "src/net.cx",
        "struct Client {} struct Server {}",
    );
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use root::net::{Client, Server};",
    );
    let parsed_files = vec![root.clone(), net.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let importer_imports = imports
        .get(&importer.file_id)
        .expect("importer imports should exist");

    assert!(importer_imports.get("Client").is_some());
    assert!(importer_imports.get("Server").is_some());
}

#[test]
fn resolve_nested_group_import_with_self() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let net = add_and_parse(&mut db, "src/net.cx", "scope http;");
    let http = add_and_parse(&mut db, "src/net/http.cx", "");
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use root::net::{self, http};",
    );
    let parsed_files =
        vec![root.clone(), net.clone(), http.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let importer_imports = imports
        .get(&importer.file_id)
        .expect("importer imports should exist");

    let net_binding = importer_imports.get("net").expect("net binding");
    let http_binding = importer_imports.get("http").expect("http binding");
    assert_eq!(net_binding.kind, ImportBindingKind::Scope);
    assert_eq!(http_binding.kind, ImportBindingKind::Scope);
    assert_eq!(net_binding.target_path, vec!["net".to_string()]);
    assert_eq!(
        http_binding.target_path,
        vec!["net".to_string(), "http".to_string()]
    );
}

#[test]
fn resolve_group_import_with_self_alias() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let net = add_and_parse(&mut db, "src/net.cx", "scope http;");
    let http = add_and_parse(&mut db, "src/net/http.cx", "");
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use root::net::{self as net_root, http};",
    );
    let parsed_files =
        vec![root.clone(), net.clone(), http.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let importer_imports = imports
        .get(&importer.file_id)
        .expect("importer imports should exist");

    let net_binding =
        importer_imports.get("net_root").expect("net_root binding");
    let http_binding = importer_imports.get("http").expect("http binding");
    assert_eq!(net_binding.kind, ImportBindingKind::Scope);
    assert_eq!(http_binding.kind, ImportBindingKind::Scope);
    assert_eq!(net_binding.target_path, vec!["net".to_string()]);
    assert_eq!(
        http_binding.target_path,
        vec!["net".to_string(), "http".to_string()]
    );
}

#[test]
fn resolve_duplicate_binding_from_self_alias_reports_error() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let net = add_and_parse(&mut db, "src/net.cx", "scope http;");
    let http = add_and_parse(&mut db, "src/net/http.cx", "");
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use root::net::{self as http, http};",
    );
    let parsed_files =
        vec![root.clone(), net.clone(), http.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let error = resolve_project_imports(&graph, &parsed_files)
        .expect_err("resolution should fail");

    match error {
        ImportResolveError::DuplicateBinding {
            file_id,
            binding_name,
        } => {
            assert_eq!(file_id, importer.file_id);
            assert_eq!(binding_name, "http");
        }
        other => panic!("expected DuplicateBinding error, got {other:?}"),
    }
}

#[test]
fn resolve_glob_import() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let _net = add_and_parse(
        &mut db,
        "src/net.cx",
        "scope http; struct Client {} struct Server {}",
    );
    let _http = add_and_parse(&mut db, "src/net/http.cx", "");
    let importer =
        add_and_parse(&mut db, "src/importer.cx", "use root::net::*;");
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            let parsed = parse_source_file_from_source_file(file)
                .expect("parse should succeed");
            parsed_to_desugared(parsed)
        })
        .collect::<Vec<_>>();

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let importer_imports = imports
        .get(&importer.file_id)
        .expect("importer imports should exist");

    assert_eq!(importer_imports.len(), 3);
    assert_eq!(
        importer_imports.get("http").map(|binding| &binding.kind),
        Some(&ImportBindingKind::Scope)
    );
    assert_eq!(
        importer_imports.get("Client").map(|binding| &binding.kind),
        Some(&ImportBindingKind::Symbol(SymbolKind::Struct))
    );
    assert_eq!(
        importer_imports.get("Server").map(|binding| &binding.kind),
        Some(&ImportBindingKind::Symbol(SymbolKind::Struct))
    );
}

#[test]
fn resolve_grouped_self_import_from_group_base() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope app;");
    let app = add_and_parse(
        &mut db,
        "src/app.cx",
        "scope http; use root::app::{self, http};",
    );
    let http = add_and_parse(&mut db, "src/app/http.cx", "");
    let parsed_files = vec![root.clone(), app.clone(), http.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let app_imports = imports.get(&app.file_id).expect("app imports");
    let app_binding = app_imports.get("app").expect("app binding");
    let http_binding = app_imports.get("http").expect("http binding");

    assert_eq!(app_binding.kind, ImportBindingKind::Scope);
    assert_eq!(http_binding.kind, ImportBindingKind::Scope);
    assert_eq!(app_binding.target_path, vec!["app".to_string()]);
    assert_eq!(
        http_binding.target_path,
        vec!["app".to_string(), "http".to_string()]
    );
}

#[test]
fn resolve_super_import_from_child_scope() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope parent;");
    let parent = add_and_parse(
        &mut db,
        "src/parent/parent.cx",
        "scope child; struct Helper {}",
    );
    let child =
        add_and_parse(&mut db, "src/parent/child.cx", "use super::Helper;");
    let parsed_files = vec![root.clone(), parent.clone(), child.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let child_imports = imports.get(&child.file_id).expect("child imports");
    let helper_binding = child_imports.get("Helper").expect("Helper binding");

    assert_eq!(
        helper_binding.kind,
        ImportBindingKind::Symbol(SymbolKind::Struct)
    );
    assert_eq!(helper_binding.target_file_id, parent.file_id);
    assert_eq!(
        helper_binding.target_path,
        vec!["parent".to_string(), "Helper".to_string()]
    );
}

#[test]
fn resolve_unknown_external_root_reports_error() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope importer;");
    let importer =
        add_and_parse(&mut db, "src/importer.cx", "use serde::json;");
    let parsed_files = vec![root.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let error = resolve_project_imports(&graph, &parsed_files)
        .expect_err("resolution should fail");

    match error {
        ImportResolveError::UnknownRoot { from_file_id, root } => {
            assert_eq!(from_file_id, importer.file_id);
            assert_eq!(root, "serde");
        }
        other => panic!("expected UnknownRoot error, got {other:?}"),
    }
}

#[test]
fn resolve_missing_path_reports_error() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let net = add_and_parse(&mut db, "src/net.cx", "");
    let importer =
        add_and_parse(&mut db, "src/importer.cx", "use root::net::Missing;");
    let parsed_files = vec![root.clone(), net.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let error = resolve_project_imports(&graph, &parsed_files)
        .expect_err("resolution should fail");

    match error {
        ImportResolveError::UnresolvedPath { from_file_id, path } => {
            assert_eq!(from_file_id, importer.file_id);
            assert_eq!(path, vec!["root", "net", "Missing"]);
        }
        other => panic!("expected UnresolvedPath error, got {other:?}"),
    }
}

#[test]
fn resolve_duplicate_import_binding_reports_error() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope a; scope b; scope importer;",
    );
    let a = add_and_parse(&mut db, "src/a.cx", "struct X {}");
    let b = add_and_parse(&mut db, "src/b.cx", "struct X {}");
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use root::a::X; use root::b::X;",
    );
    let parsed_files =
        vec![root.clone(), a.clone(), b.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let error = resolve_project_imports(&graph, &parsed_files)
        .expect_err("resolution should fail");

    match error {
        ImportResolveError::DuplicateBinding {
            file_id,
            binding_name,
        } => {
            assert_eq!(file_id, importer.file_id);
            assert_eq!(binding_name, "X");
        }
        other => panic!("expected DuplicateBinding error, got {other:?}"),
    }
}

#[test]
fn resolve_binary_root_imports_separately_from_library_root() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net;");
    let _net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let main = add_and_parse(&mut db, "src/main.cx", "use root::net::Client;");
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
    let library_graph = resolver
        .resolve_library_root(root.file_id)
        .expect("library graph should resolve");
    let binary_graph = resolver
        .resolve_binary_root(main.file_id)
        .expect("binary graph should resolve");

    assert_eq!(
        library_graph.scope(root.file_id).map(|scope| scope.kind),
        Some(ResolvedScopeKind::Root)
    );
    assert_eq!(
        binary_graph.scope(main.file_id).map(|scope| scope.kind),
        Some(ResolvedScopeKind::BinaryRoot)
    );
    assert!(binary_graph.scope(root.file_id).is_none());

    let binary_error = resolve_project_imports(&binary_graph, &parsed_files)
        .expect_err("binary import resolution should fail");
    match binary_error {
        ImportResolveError::UnresolvedPath { from_file_id, path } => {
            assert_eq!(from_file_id, main.file_id);
            assert_eq!(path, vec!["root", "net", "Client"]);
        }
        other => panic!("expected UnresolvedPath error, got {other:?}"),
    }
}

#[test]
fn resolve_recursive_group_import_with_alias_and_glob() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope api; scope importer;");
    let _api =
        add_and_parse(&mut db, "src/api.cx", "scope client; scope server;");
    let _client = add_and_parse(
        &mut db,
        "src/api/client.cx",
        "struct Client {} struct Request {}",
    );
    let server =
        add_and_parse(&mut db, "src/api/server.cx", "struct HttpServer {}");
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use root::api::{client::*, server::{self, HttpServer as ServerType}};",
    );
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            let parsed = parse_source_file_from_source_file(file)
                .expect("parse should succeed");
            parsed_to_desugared(parsed)
        })
        .collect::<Vec<_>>();

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let importer_imports = imports
        .get(&importer.file_id)
        .expect("importer imports should exist");

    assert_eq!(
        importer_imports.get("Client").map(|binding| &binding.kind),
        Some(&ImportBindingKind::Symbol(SymbolKind::Struct))
    );
    assert_eq!(
        importer_imports.get("Request").map(|binding| &binding.kind),
        Some(&ImportBindingKind::Symbol(SymbolKind::Struct))
    );
    let server_binding =
        importer_imports.get("server").expect("server binding");
    assert_eq!(server_binding.kind, ImportBindingKind::Scope);
    assert_eq!(
        server_binding.target_path,
        vec!["api".to_string(), "server".to_string()]
    );
    let alias_binding = importer_imports
        .get("ServerType")
        .expect("aliased server binding");
    assert_eq!(
        alias_binding.kind,
        ImportBindingKind::Symbol(SymbolKind::Struct)
    );
    assert_eq!(alias_binding.target_file_id, server.file_id);
}

#[test]
fn resolve_duplicate_binding_from_recursive_group_reports_error() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope api; scope importer;");
    let _api = add_and_parse(&mut db, "src/api.cx", "scope server;");
    let _server =
        add_and_parse(&mut db, "src/api/server.cx", "struct HttpServer {}");
    let importer = add_and_parse(
        &mut db,
        "src/importer.cx",
        "use root::api::{server::{HttpServer}, server::HttpServer};",
    );
    let parsed_files = db
        .files()
        .iter()
        .map(|file| {
            let parsed = parse_source_file_from_source_file(file)
                .expect("parse should succeed");
            parsed_to_desugared(parsed)
        })
        .collect::<Vec<_>>();

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let error = resolve_project_imports(&graph, &parsed_files)
        .expect_err("resolution should fail");

    match error {
        ImportResolveError::DuplicateBinding {
            file_id,
            binding_name,
        } => {
            assert_eq!(file_id, importer.file_id);
            assert_eq!(binding_name, "HttpServer");
        }
        other => panic!("expected DuplicateBinding error, got {other:?}"),
    }
}

#[test]
fn resolve_scope_keyword_root_segment_reports_unknown_root_when_unbound() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope importer;");
    let importer =
        add_and_parse(&mut db, "src/importer.cx", "use scope::Thing;");
    let parsed_files = vec![root.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let error = resolve_project_imports(&graph, &parsed_files)
        .expect_err("resolution should fail");

    match error {
        ImportResolveError::UnknownRoot { from_file_id, root } => {
            assert_eq!(from_file_id, importer.file_id);
            assert_eq!(root, "scope");
        }
        other => panic!("expected UnknownRoot error, got {other:?}"),
    }
}
