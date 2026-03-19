use core_x::frontend::DesugaredFile;
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    GlobalItemTable, ItemId, ResolvedDeclarationTable, ScopeGraph,
    ScopeResolver, resolve_declaration_types, resolve_project_imports,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    NamedTypeKind, SignatureTypingIssueKind, Type, type_declaration_signatures,
};
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
) -> ScopeGraph {
    ScopeResolver::new(db, parsed_files)
        .resolve_library_root(root_file_id)
        .expect("library scope resolution should succeed")
}

fn resolve_tables(
    graph: &ScopeGraph,
    parsed_files: &[DesugaredFile],
) -> (
    GlobalItemTable,
    BTreeMap<FileId, core_x::frontend::ResolvedImports>,
    ResolvedDeclarationTable,
) {
    let item_table = GlobalItemTable::collect(graph, parsed_files);
    let (_, imports) =
        resolve_project_imports(graph, parsed_files).expect("imports");
    let declarations =
        resolve_declaration_types(graph, parsed_files, &imports, &item_table);
    (item_table, imports, declarations)
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

fn assert_named_type(ty: &Type, expected_item_id: ItemId, kind: NamedTypeKind) {
    assert_eq!(
        ty,
        &Type::Named {
            item_id: expected_item_id,
            kind,
        }
    );
}

#[test]
fn typed_function_signatures() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Request {} struct Response {} fn handle(_ req: Request) -> Response {}",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, _, declarations) = resolve_tables(&graph, &parsed_files);
    let typed = type_declaration_signatures(&declarations, &item_table);

    let handle_id = get_item_id(&item_table, &["handle"]);
    let request_id = get_item_id(&item_table, &["Request"]);
    let response_id = get_item_id(&item_table, &["Response"]);

    let signature = typed
        .function(handle_id)
        .expect("handle signature should exist");
    assert_eq!(signature.param_types.len(), 1);
    assert_named_type(
        &signature.param_types[0],
        request_id,
        NamedTypeKind::Struct,
    );
    assert_named_type(
        signature.return_type.as_ref().expect("return type"),
        response_id,
        NamedTypeKind::Struct,
    );
    assert!(typed.issues.is_empty());
}

#[test]
fn typed_struct_fields_keep_field_names() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Inner {} struct Wrapper { item: Inner }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, _, declarations) = resolve_tables(&graph, &parsed_files);
    let typed = type_declaration_signatures(&declarations, &item_table);

    let wrapper_id = get_item_id(&item_table, &["Wrapper"]);
    let inner_id = get_item_id(&item_table, &["Inner"]);

    let struct_sig = typed
        .struct_data(wrapper_id)
        .expect("wrapper signature should exist");
    assert_eq!(struct_sig.fields.len(), 1);
    assert_eq!(struct_sig.fields[0].name, "item");
    assert_named_type(
        &struct_sig.fields[0].ty,
        inner_id,
        NamedTypeKind::Struct,
    );
}

#[test]
fn typed_enum_payloads_keep_case_and_method_names() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Payload {} enum Message { Data(Payload), fn make(_ value: Payload) -> Payload {} }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, _, declarations) = resolve_tables(&graph, &parsed_files);
    let typed = type_declaration_signatures(&declarations, &item_table);

    let message_id = get_item_id(&item_table, &["Message"]);
    let payload_id = get_item_id(&item_table, &["Payload"]);

    let enum_sig = typed
        .enum_data(message_id)
        .expect("message signature should exist");
    assert_eq!(enum_sig.case_signatures.len(), 1);
    assert_eq!(enum_sig.case_signatures[0].name, "Data");
    assert_named_type(
        &enum_sig.case_signatures[0].payload_types[0],
        payload_id,
        NamedTypeKind::Struct,
    );
    assert_eq!(enum_sig.method_signatures[0].name, "make");
    assert_named_type(
        &enum_sig.method_signatures[0].signature.param_types[0],
        payload_id,
        NamedTypeKind::Struct,
    );
    assert_named_type(
        enum_sig.method_signatures[0]
            .signature
            .return_type
            .as_ref()
            .expect("return type"),
        payload_id,
        NamedTypeKind::Struct,
    );
}

#[test]
fn typed_protocol_signatures_keep_method_names() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Request {} struct Response {} protocol Service { fn call(_ req: Request) -> Response; }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, _, declarations) = resolve_tables(&graph, &parsed_files);
    let typed = type_declaration_signatures(&declarations, &item_table);

    let service_id = get_item_id(&item_table, &["Service"]);
    let request_id = get_item_id(&item_table, &["Request"]);
    let response_id = get_item_id(&item_table, &["Response"]);

    let protocol_sig = typed
        .protocol(service_id)
        .expect("service signature should exist");
    assert_eq!(protocol_sig.method_signatures.len(), 1);
    assert_eq!(protocol_sig.method_signatures[0].name, "call");
    assert_named_type(
        &protocol_sig.method_signatures[0].signature.param_types[0],
        request_id,
        NamedTypeKind::Struct,
    );
    assert_named_type(
        protocol_sig.method_signatures[0]
            .signature
            .return_type
            .as_ref()
            .expect("return type"),
        response_id,
        NamedTypeKind::Struct,
    );
}

#[test]
fn typed_impl_target_and_conformance_keep_method_names() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "protocol Drawable { fn draw() -> Canvas; } struct Canvas {} struct Client {} impl Drawable for Client { fn draw() -> Canvas {} }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, _, declarations) = resolve_tables(&graph, &parsed_files);
    let typed = type_declaration_signatures(&declarations, &item_table);

    let client_id = get_item_id(&item_table, &["Client"]);
    let drawable_id = get_item_id(&item_table, &["Drawable"]);
    let canvas_id = get_item_id(&item_table, &["Canvas"]);
    let impls = typed.impls_in_scope(root.file_id);

    assert_eq!(impls.len(), 1);
    assert_named_type(&impls[0].target, client_id, NamedTypeKind::Struct);
    assert_named_type(
        impls[0].conformance.as_ref().expect("conformance"),
        drawable_id,
        NamedTypeKind::Protocol,
    );
    assert_eq!(impls[0].method_signatures[0].name, "draw");
    assert_named_type(
        impls[0].method_signatures[0]
            .signature
            .return_type
            .as_ref()
            .expect("return type"),
        canvas_id,
        NamedTypeKind::Struct,
    );
}

#[test]
fn unresolved_declaration_type_paths_become_structured_issues() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn helper() {} fn broken(_ value: Missing) -> helper {}",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, _, declarations) = resolve_tables(&graph, &parsed_files);
    let typed = type_declaration_signatures(&declarations, &item_table);

    let broken_id = get_item_id(&item_table, &["broken"]);
    let signature = typed
        .function(broken_id)
        .expect("broken signature should exist");
    assert_eq!(signature.param_types[0], Type::Error);
    assert_eq!(signature.return_type, Some(Type::Error));

    assert_eq!(typed.issues.len(), 2);
    assert!(typed.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            SignatureTypingIssueKind::UnresolvedPath { ref path }
                if path == &vec!["Missing".to_string()]
        )
    }));
    assert!(typed.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            SignatureTypingIssueKind::InvalidTypeItem { ref path, .. }
                if path == &vec!["helper".to_string()]
        )
    }));
}

#[test]
fn signature_tables_are_item_id_keyed_and_deterministic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct A {} struct B { a: A } fn f(_ a: A) -> B {} protocol P { fn g(_ a: A) -> B; } enum E { C(A) }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, _, declarations) = resolve_tables(&graph, &parsed_files);

    let first = type_declaration_signatures(&declarations, &item_table);
    let second = type_declaration_signatures(&declarations, &item_table);
    assert_eq!(first, second);

    let f_id = get_item_id(&item_table, &["f"]);
    let b_id = get_item_id(&item_table, &["B"]);
    let p_id = get_item_id(&item_table, &["P"]);
    let e_id = get_item_id(&item_table, &["E"]);

    assert!(first.functions.contains_key(&f_id));
    assert!(first.structs.contains_key(&b_id));
    assert!(first.protocols.contains_key(&p_id));
    assert!(first.enums.contains_key(&e_id));
}

#[test]
fn missing_item_metadata_uses_structured_issue_without_fake_file_id() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 {}");
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, _, mut declarations) =
        resolve_tables(&graph, &parsed_files);

    declarations.by_item_id.insert(
        ItemId::new(9_999),
        declarations.by_item_id[&get_item_id(&item_table, &["f"])].clone(),
    );

    let typed = type_declaration_signatures(&declarations, &item_table);
    assert!(typed.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            SignatureTypingIssueKind::MissingGlobalItemMetadata { item_id }
                if item_id == ItemId::new(9_999)
        ) && issue.containing_scope_file_id.is_none()
    }));
}
