use core_x::frontend::ParsedFile;
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    ImportBindingKind, ImportResolver, ScopeResolver, SymbolKind,
    build_global_item_table, resolve_project_imports,
    scope_symbols_from_global_item_table,
};
use core_x::frontend::source::{FileId, SourceDb};
use std::collections::BTreeMap;

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

fn resolve_library_graph(
    db: &SourceDb,
    parsed_files: &[ParsedFile],
    root_file_id: FileId,
) -> core_x::frontend::ScopeGraph {
    ScopeResolver::new(db, parsed_files)
        .resolve_library_root(root_file_id)
        .expect("library scope resolution should succeed")
}

#[test]
fn collect_items_from_root_and_nested_scopes() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope net; fn top() {} struct App {}",
    );
    let net = add_and_parse(
        &mut db,
        "src/net.cx",
        "enum Status { Ready } protocol Service {}",
    );
    let parsed_files = vec![root.clone(), net.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let table = build_global_item_table(&graph, &parsed_files);

    let collected = table
        .iter()
        .map(|item| (item.kind, item.full_path.clone()))
        .collect::<Vec<_>>();
    assert_eq!(collected.len(), 5);
    assert!(collected.contains(&(SymbolKind::Scope, vec!["net".to_string()],)));
    assert!(
        collected.contains(&(SymbolKind::Function, vec!["top".to_string()],))
    );
    assert!(
        collected.contains(&(SymbolKind::Struct, vec!["App".to_string()],))
    );
    assert!(collected.contains(&(
        SymbolKind::Enum,
        vec!["net".to_string(), "Status".to_string()],
    )));
    assert!(collected.contains(&(
        SymbolKind::Protocol,
        vec!["net".to_string(), "Service".to_string()],
    )));
}

#[test]
fn item_id_assignment_is_deterministic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net; fn top() {}");
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let parsed_files = vec![root.clone(), net.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);

    let first = build_global_item_table(&graph, &parsed_files);
    let second = build_global_item_table(&graph, &parsed_files);

    let first_projection = first
        .iter()
        .map(|item| (item.id.raw(), item.full_path.clone()))
        .collect::<Vec<_>>();
    let second_projection = second
        .iter()
        .map(|item| (item.id.raw(), item.full_path.clone()))
        .collect::<Vec<_>>();
    assert_eq!(first_projection, second_projection);
    assert_eq!(
        first_projection,
        vec![
            (0, vec!["net".to_string()]),
            (1, vec!["top".to_string()]),
            (2, vec!["net".to_string(), "Client".to_string()]),
        ]
    );
}

#[test]
fn full_path_construction_uses_scope_path_prefix() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net;");
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let parsed_files = vec![root.clone(), net.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let table = build_global_item_table(&graph, &parsed_files);

    let client = table
        .get_by_full_path(&["net".to_string(), "Client".to_string()])
        .expect("client item");
    assert_eq!(client.scope_path, vec!["net".to_string()]);
    assert_eq!(
        client.full_path,
        vec!["net".to_string(), "Client".to_string()]
    );
    assert_eq!(client.containing_scope_file_id, net.file_id);
}

#[test]
fn lookup_by_full_path_returns_expected_item() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; struct RootType {}");
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let parsed_files = vec![root.clone(), net.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let table = build_global_item_table(&graph, &parsed_files);

    let root_item = table
        .get_by_full_path(&["RootType".to_string()])
        .expect("root item");
    assert_eq!(root_item.kind, SymbolKind::Struct);

    let net_item = table
        .get_by_full_path(&["net".to_string(), "Client".to_string()])
        .expect("net item");
    assert_eq!(net_item.kind, SymbolKind::Struct);
}

#[test]
fn lookup_by_containing_scope_preserves_declaration_order() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope net; fn first() {} struct Second {} enum Third { T }",
    );
    let net = add_and_parse(&mut db, "src/net.cx", "");
    let parsed_files = vec![root.clone(), net];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let table = build_global_item_table(&graph, &parsed_files);
    let names = table
        .items_in_scope(root.file_id)
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["net", "first", "Second", "Third"]);
}

#[test]
fn import_resolver_symbol_collection_is_backed_by_item_table() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "scope net; scope importer;");
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let importer =
        add_and_parse(&mut db, "src/importer.cx", "use root::net::Client;");
    let parsed_files = vec![root.clone(), net.clone(), importer.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let table = build_global_item_table(&graph, &parsed_files);
    let symbols_from_table = scope_symbols_from_global_item_table(&table);

    let empty = BTreeMap::new();
    let resolver = ImportResolver::new(&graph, &parsed_files, &empty);
    let symbols_from_resolver = resolver.collect_scope_symbols();
    assert_eq!(symbols_from_resolver, symbols_from_table);

    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let importer_imports = imports
        .get(&importer.file_id)
        .expect("importer imports should exist");
    let binding = importer_imports.get("Client").expect("Client binding");
    assert_eq!(binding.kind, ImportBindingKind::Symbol(SymbolKind::Struct));
    assert_eq!(binding.target_file_id, net.file_id);
}

#[test]
fn full_path_lookup_uses_first_definition_when_duplicates_exist() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn dup() {} fn dup() {} struct Other {}",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let table = build_global_item_table(&graph, &parsed_files);

    let duplicate_ids = table
        .iter()
        .filter(|item| item.name == "dup")
        .map(|item| item.id)
        .collect::<Vec<_>>();
    assert_eq!(duplicate_ids.len(), 2);

    let lookup_id = table
        .item_id_by_full_path(&["dup".to_string()])
        .expect("dup lookup id");
    assert_eq!(lookup_id, duplicate_ids[0]);
}
