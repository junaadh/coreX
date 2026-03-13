use core_x::frontend::ParsedFile;
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    GlobalItemTable, ItemId, ScopeGraph, ScopeResolver,
    resolve_declaration_types, resolve_project_imports,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    NamedTypeKind, Type, TypedItemData, TypedItemKind, TypedItemTableIssueKind,
    build_typed_item_table, type_declaration_signatures,
};

fn add_and_parse(db: &mut SourceDb, path: &str, source: &str) -> ParsedFile {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(parsed.diagnostics.is_empty(), "strict parse diagnostics");
    parsed
}

fn resolve_library_graph(
    db: &SourceDb,
    parsed_files: &[ParsedFile],
    root_file_id: FileId,
) -> ScopeGraph {
    ScopeResolver::new(db, parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph")
}

fn build_tables(
    graph: &ScopeGraph,
    parsed_files: &[ParsedFile],
) -> (GlobalItemTable, core_x::frontend::TypedSignatureTable) {
    let global = GlobalItemTable::collect(graph, parsed_files);
    let (_, imports) =
        resolve_project_imports(graph, parsed_files).expect("imports");
    let declarations =
        resolve_declaration_types(graph, parsed_files, &imports, &global);
    let signatures = type_declaration_signatures(&declarations, &global);
    (global, signatures)
}

fn get_item_id(table: &GlobalItemTable, path: &[&str]) -> ItemId {
    let full_path = path
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    table
        .item_id_by_full_path(&full_path)
        .unwrap_or_else(|| panic!("missing item for path {full_path:?}"))
}

#[test]
fn typed_item_table_construction() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Client {} enum Mode { Fast } protocol Service { fn run(_ c: Client) -> Mode; } fn start(_ c: Client) -> Mode {}",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (global, signatures) = build_tables(&graph, &parsed_files);
    let typed = build_typed_item_table(&global, &signatures);

    assert_eq!(typed.len(), 4);
    assert!(typed.issues.is_empty());
}

#[test]
fn typed_item_table_lookup_by_item_id() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Client {} fn start(_ c: Client) -> Client {}",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (global, signatures) = build_tables(&graph, &parsed_files);
    let typed = build_typed_item_table(&global, &signatures);

    let start_id = get_item_id(&global, &["start"]);
    let client_id = get_item_id(&global, &["Client"]);

    let signature = typed.function(start_id).expect("typed function signature");
    assert_eq!(signature.param_types.len(), 1);
    assert_eq!(
        signature.param_types[0],
        Type::Named {
            item_id: client_id,
            kind: NamedTypeKind::Struct,
        }
    );

    match typed.get(start_id) {
        Some(TypedItemData::Function(_)) => {}
        other => panic!("expected typed function, got {other:?}"),
    }
}

#[test]
fn typed_item_table_iteration_is_deterministic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct A {} struct B {} fn f(_ a: A) -> B {} protocol P { fn g(_ a: A) -> B; } enum E { C(A) }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (global, signatures) = build_tables(&graph, &parsed_files);

    let first = build_typed_item_table(&global, &signatures);
    let second = build_typed_item_table(&global, &signatures);
    assert_eq!(first, second);

    let first_ids =
        first.iter().map(|(item_id, _)| item_id).collect::<Vec<_>>();
    let second_ids = second
        .iter()
        .map(|(item_id, _)| item_id)
        .collect::<Vec<_>>();
    assert_eq!(first_ids, second_ids);
}

#[test]
fn typed_item_table_compatibility_with_global_item_table() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope net; fn root_fn() -> i32 {}",
    );
    let net = add_and_parse(
        &mut db,
        "src/net.cx",
        "struct Client {} fn connect(_ c: Client) -> Client {}",
    );
    let parsed_files = vec![root.clone(), net.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (global, signatures) = build_tables(&graph, &parsed_files);
    let typed = build_typed_item_table(&global, &signatures);

    let root_ids = global
        .ids_in_scope(root.file_id)
        .iter()
        .copied()
        .filter(|item_id| {
            global
                .get(*item_id)
                .and_then(|item| TypedItemKind::from_item_kind(item.kind))
                .is_some()
        })
        .collect::<Vec<_>>();
    let typed_root_ids = typed.ids_in_scope(root.file_id).to_vec();
    assert_eq!(typed_root_ids, root_ids);

    let net_ids = global
        .ids_in_scope(net.file_id)
        .iter()
        .copied()
        .filter(|item_id| {
            global
                .get(*item_id)
                .and_then(|item| TypedItemKind::from_item_kind(item.kind))
                .is_some()
        })
        .collect::<Vec<_>>();
    let typed_net_ids = typed.ids_in_scope(net.file_id).to_vec();
    assert_eq!(typed_net_ids, net_ids);

    assert!(
        typed
            .ids_for_kind(TypedItemKind::Struct)
            .iter()
            .all(|item_id| {
                matches!(typed.get(*item_id), Some(TypedItemData::Struct(_)))
            })
    );
}

#[test]
fn typed_item_table_records_impl_attachment_metadata() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "protocol Drawable { fn draw() -> Canvas; } struct Canvas {} struct Client {} impl Drawable for Client { fn draw() -> Canvas {} }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (global, signatures) = build_tables(&graph, &parsed_files);
    let typed = build_typed_item_table(&global, &signatures);

    let client_id = get_item_id(&global, &["Client"]);
    let drawable_id = get_item_id(&global, &["Drawable"]);

    let owners = typed.impl_owners_for_target(client_id);
    assert_eq!(owners.len(), 1);

    let attachment = typed
        .impl_attachment(&owners[0])
        .expect("impl attachment should exist");
    assert_eq!(attachment.target_item_id, Some(client_id));
    assert_eq!(attachment.conformance_item_id, Some(drawable_id));

    let impl_signature = typed
        .impl_signature(&owners[0])
        .expect("impl signature should exist");
    assert_eq!(impl_signature.method_signatures[0].name, "draw");
}

#[test]
fn typed_item_table_reports_signature_without_global_item() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn ok() -> i32 {}");
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (global, mut signatures) = build_tables(&graph, &parsed_files);

    signatures.functions.insert(
        ItemId::new(9_001),
        signatures.functions[&get_item_id(&global, &["ok"])].clone(),
    );

    let typed = build_typed_item_table(&global, &signatures);
    assert!(typed.issues.iter().any(|issue| {
        issue.associated_item_id == Some(ItemId::new(9_001))
            && matches!(
                issue.kind,
                TypedItemTableIssueKind::SignatureWithoutGlobalItem {
                    signature_kind: TypedItemKind::Function,
                }
            )
    }));
}
