use core_x::frontend::DesugaredFile;
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    GlobalItemTable, ItemId, ResolvedDeclaration, ResolvedTypeRef, ScopeGraph,
    ScopeResolver, resolve_declaration_types, resolve_project_imports,
    scope_symbols_from_global_item_table,
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
) {
    let item_table = GlobalItemTable::collect(graph, parsed_files);
    let (_, imports) =
        resolve_project_imports(graph, parsed_files).expect("imports");
    (item_table, imports)
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

fn assert_named_type(
    ty: &ResolvedTypeRef,
    expected_type_segments: &[&str],
    expected_full_path: &[&str],
    expected_item_id: ItemId,
) {
    match ty {
        ResolvedTypeRef::Named {
            segments,
            resolved: Some(resolved),
        } => {
            let expected_segments = expected_type_segments
                .iter()
                .map(|segment| (*segment).to_string())
                .collect::<Vec<_>>();
            let expected_full_path = expected_full_path
                .iter()
                .map(|segment| (*segment).to_string())
                .collect::<Vec<_>>();
            assert_eq!(segments, &expected_segments);
            assert_eq!(resolved.item_id, expected_item_id);
            assert_eq!(resolved.full_path, expected_full_path);
        }
        other => panic!("expected resolved named type, got {other:?}"),
    }
}

#[test]
fn resolves_struct_field_types() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Client {} struct Wrapper { client: Client }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);
    let resolved =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);

    let wrapper_id = get_item_id(&item_table, &["Wrapper"]);
    let client_id = get_item_id(&item_table, &["Client"]);
    match resolved
        .get(wrapper_id)
        .expect("wrapper declaration should exist")
    {
        ResolvedDeclaration::Struct(struct_decl) => {
            assert_eq!(struct_decl.fields.len(), 1);
            assert_eq!(struct_decl.fields[0].name, "client");
            assert_named_type(
                &struct_decl.fields[0].ty,
                &["Client"],
                &["Client"],
                client_id,
            );
        }
        other => panic!("expected struct declaration, got {other:?}"),
    }
}

#[test]
fn resolves_function_parameter_and_return_types() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Request {} struct Response {} fn handle(_ req: Request) -> Response {}",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);
    let resolved =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);

    let handle_id = get_item_id(&item_table, &["handle"]);
    let request_id = get_item_id(&item_table, &["Request"]);
    let response_id = get_item_id(&item_table, &["Response"]);
    match resolved
        .get(handle_id)
        .expect("handle declaration should exist")
    {
        ResolvedDeclaration::Function(signature) => {
            assert_eq!(signature.params.len(), 1);
            assert_named_type(
                &signature.params[0].ty,
                &["Request"],
                &["Request"],
                request_id,
            );
            assert_named_type(
                signature.return_type.as_ref().expect("return type"),
                &["Response"],
                &["Response"],
                response_id,
            );
        }
        other => panic!("expected function declaration, got {other:?}"),
    }
}

#[test]
fn resolves_enum_payload_and_member_function_types() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Payload {} struct ResultT {} enum Message { Data(Payload), fn make(_ value: Payload) -> ResultT {} }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);
    let resolved =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);

    let message_id = get_item_id(&item_table, &["Message"]);
    let payload_id = get_item_id(&item_table, &["Payload"]);
    let result_id = get_item_id(&item_table, &["ResultT"]);
    match resolved
        .get(message_id)
        .expect("enum declaration should exist")
    {
        ResolvedDeclaration::Enum(enum_decl) => {
            assert_eq!(enum_decl.cases.len(), 1);
            assert_named_type(
                &enum_decl.cases[0].payload[0].ty,
                &["Payload"],
                &["Payload"],
                payload_id,
            );
            assert_eq!(enum_decl.methods.len(), 1);
            assert_named_type(
                &enum_decl.methods[0].signature.params[0].ty,
                &["Payload"],
                &["Payload"],
                payload_id,
            );
            assert_named_type(
                enum_decl.methods[0]
                    .signature
                    .return_type
                    .as_ref()
                    .expect("return type"),
                &["ResultT"],
                &["ResultT"],
                result_id,
            );
        }
        other => panic!("expected enum declaration, got {other:?}"),
    }
}

#[test]
fn resolves_protocol_method_signature_types() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Request {} struct Response {} protocol Service { fn call(_ req: Request) -> Response; }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);
    let resolved =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);

    let service_id = get_item_id(&item_table, &["Service"]);
    let request_id = get_item_id(&item_table, &["Request"]);
    let response_id = get_item_id(&item_table, &["Response"]);
    match resolved
        .get(service_id)
        .expect("protocol declaration should exist")
    {
        ResolvedDeclaration::Protocol(protocol_decl) => {
            assert_eq!(protocol_decl.methods.len(), 1);
            assert_named_type(
                &protocol_decl.methods[0].signature.params[0].ty,
                &["Request"],
                &["Request"],
                request_id,
            );
            assert_named_type(
                protocol_decl.methods[0]
                    .signature
                    .return_type
                    .as_ref()
                    .expect("return type"),
                &["Response"],
                &["Response"],
                response_id,
            );
        }
        other => panic!("expected protocol declaration, got {other:?}"),
    }
}

#[test]
fn resolves_impl_target_and_conformance_types() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "protocol Drawable { fn draw() -> Canvas; } struct Canvas {} struct Client {} impl Drawable for Client { fn draw() -> Canvas {} }",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);
    let resolved =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);

    let client_id = get_item_id(&item_table, &["Client"]);
    let drawable_id = get_item_id(&item_table, &["Drawable"]);
    let canvas_id = get_item_id(&item_table, &["Canvas"]);
    let impls = resolved.impls_in_scope(root.file_id);
    assert_eq!(impls.len(), 1);
    assert_named_type(&impls[0].target, &["Client"], &["Client"], client_id);
    assert_named_type(
        impls[0].conformance.as_ref().expect("conformance type"),
        &["Drawable"],
        &["Drawable"],
        drawable_id,
    );
    assert_named_type(
        impls[0].methods[0]
            .signature
            .return_type
            .as_ref()
            .expect("return type"),
        &["Canvas"],
        &["Canvas"],
        canvas_id,
    );
}

#[test]
fn resolves_declaration_types_through_imports() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net; scope app;");
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let app = add_and_parse(
        &mut db,
        "src/app.cx",
        "use root::net::Client; fn build(_ value: Client) -> Client {}",
    );
    let parsed_files = vec![root.clone(), net.clone(), app.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);
    let resolved =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);

    let build_id = get_item_id(&item_table, &["app", "build"]);
    let client_id = get_item_id(&item_table, &["net", "Client"]);
    match resolved
        .get(build_id)
        .expect("build declaration should exist")
    {
        ResolvedDeclaration::Function(signature) => {
            assert_named_type(
                &signature.params[0].ty,
                &["Client"],
                &["net", "Client"],
                client_id,
            );
            assert_named_type(
                signature.return_type.as_ref().expect("return type"),
                &["Client"],
                &["net", "Client"],
                client_id,
            );
        }
        other => panic!("expected function declaration, got {other:?}"),
    }
}

#[test]
fn resolves_nested_scope_paths() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope net; fn connect(_ c: net::http::Client) -> net::http::Client {}",
    );
    let net = add_and_parse(&mut db, "src/net.cx", "scope http;");
    let http = add_and_parse(&mut db, "src/net/http.cx", "struct Client {}");
    let parsed_files = vec![root.clone(), net.clone(), http.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);
    let resolved =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);

    let connect_id = get_item_id(&item_table, &["connect"]);
    let client_id = get_item_id(&item_table, &["net", "http", "Client"]);
    match resolved
        .get(connect_id)
        .expect("connect declaration should exist")
    {
        ResolvedDeclaration::Function(signature) => {
            assert_named_type(
                &signature.params[0].ty,
                &["net", "http", "Client"],
                &["net", "http", "Client"],
                client_id,
            );
            assert_named_type(
                signature.return_type.as_ref().expect("return type"),
                &["net", "http", "Client"],
                &["net", "http", "Client"],
                client_id,
            );
        }
        other => panic!("expected function declaration, got {other:?}"),
    }
}

#[test]
fn declaration_resolution_is_deterministic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope net; scope app; struct RootType {}",
    );
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let app = add_and_parse(
        &mut db,
        "src/app.cx",
        "use root::net::Client; struct Holder { value: Client }",
    );
    let parsed_files = vec![root.clone(), net.clone(), app.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);

    let first =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);
    let second =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);
    assert_eq!(first, second);
}

#[test]
fn unresolved_declaration_type_paths_are_recorded() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn broken(_ value: Missing) -> Missing {}",
    );
    let parsed_files = vec![root.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);
    let resolved =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);

    let broken_id = get_item_id(&item_table, &["broken"]);
    match resolved
        .get(broken_id)
        .expect("broken declaration should exist")
    {
        ResolvedDeclaration::Function(signature) => {
            match &signature.params[0].ty {
                ResolvedTypeRef::Named { resolved: None, .. } => {}
                other => {
                    panic!(
                        "expected unresolved named param type, got {other:?}"
                    )
                }
            }
            match signature.return_type.as_ref().expect("return type") {
                ResolvedTypeRef::Named { resolved: None, .. } => {}
                other => panic!(
                    "expected unresolved named return type, got {other:?}"
                ),
            }
        }
        other => panic!("expected function declaration, got {other:?}"),
    }

    assert_eq!(resolved.unresolved_paths.len(), 2);
    for unresolved in &resolved.unresolved_paths {
        assert_eq!(unresolved.containing_scope_file_id, root.file_id);
        assert_eq!(unresolved.path, vec!["Missing".to_string()]);
    }
}

#[test]
fn declaration_resolution_and_scope_symbols_share_item_table_identity() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net; scope app;");
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let app = add_and_parse(
        &mut db,
        "src/app.cx",
        "use root::net::Client; struct Holder { value: Client }",
    );
    let parsed_files = vec![root.clone(), net.clone(), app.clone()];
    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (item_table, imports) = resolve_tables(&graph, &parsed_files);
    let resolved =
        resolve_declaration_types(&graph, &parsed_files, &imports, &item_table);

    let holder_id = get_item_id(&item_table, &["app", "Holder"]);
    let client_id = get_item_id(&item_table, &["net", "Client"]);
    let symbols = scope_symbols_from_global_item_table(&item_table);
    let net_symbols = symbols.get(&net.file_id).expect("net symbols");
    let client_symbol = net_symbols.get("Client").expect("client symbol");
    let client_item = item_table.get(client_id).expect("client item");

    assert_eq!(client_symbol.defining_file_id, client_item.defining_file_id);
    match resolved
        .get(holder_id)
        .expect("holder declaration should exist")
    {
        ResolvedDeclaration::Struct(struct_decl) => {
            assert_named_type(
                &struct_decl.fields[0].ty,
                &["Client"],
                &["net", "Client"],
                client_id,
            );
        }
        other => panic!("expected struct declaration, got {other:?}"),
    }
}
