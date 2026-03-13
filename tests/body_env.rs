use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, LocalKind, LocalMutability, ScopeGraph,
    ScopeResolver, resolve_bodies, resolve_declaration_types,
    resolve_project_imports,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    BodyTypeEnvironmentTable, BuiltinType, NamedTypeKind, Type,
    build_body_type_environments, build_typed_item_table,
    type_declaration_signatures,
};

fn add_and_parse(
    db: &mut SourceDb,
    path: &str,
    source: &str,
) -> core_x::frontend::ParsedFile {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(parsed.diagnostics.is_empty(), "strict parse diagnostics");
    parsed
}

fn resolve_library_graph(
    db: &SourceDb,
    parsed_files: &[core_x::frontend::ParsedFile],
    root_file_id: FileId,
) -> ScopeGraph {
    ScopeResolver::new(db, parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph")
}

struct Pipeline {
    item_table: GlobalItemTable,
    body_envs: BodyTypeEnvironmentTable,
}

fn run_pipeline(
    db: &SourceDb,
    parsed_files: &[core_x::frontend::ParsedFile],
    root_file_id: FileId,
) -> Pipeline {
    let graph = resolve_library_graph(db, parsed_files, root_file_id);
    let item_table = GlobalItemTable::collect(&graph, parsed_files);
    let (_, imports) =
        resolve_project_imports(&graph, parsed_files).expect("imports");
    let declarations =
        resolve_declaration_types(&graph, parsed_files, &imports, &item_table);
    let signatures = type_declaration_signatures(&declarations, &item_table);
    let typed_items = build_typed_item_table(&item_table, &signatures);
    let bodies = resolve_bodies(
        &graph,
        parsed_files,
        &imports,
        &item_table,
        &declarations,
    );
    let body_envs = build_body_type_environments(&bodies, &typed_items);
    Pipeline {
        item_table,
        body_envs,
    }
}

fn owner_for(item_table: &GlobalItemTable, path: &[&str]) -> DeclarationOwner {
    let full_path = path
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    let item_id = item_table
        .item_id_by_full_path(&full_path)
        .unwrap_or_else(|| panic!("missing item for path {full_path:?}"));
    DeclarationOwner::Item(item_id)
}

#[test]
fn environment_creation_for_function_body() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn add(_ x: i32, _ y: i32) -> i32 { let z: i32 = x; z; }",
    );
    let parsed_files = vec![root.clone()];

    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["add"]);
    let envs = pipeline.body_envs.envs_for_owner(&owner);
    assert_eq!(envs.len(), 1);
    assert_eq!(envs[0].owner, owner);
    assert_eq!(envs[0].containing_scope_file_id, root.file_id);
    assert_eq!(envs[0].kind, core_x::frontend::BodyKind::Function);
    assert!(!envs[0].local_types.is_empty());
    assert!(pipeline.body_envs.issues.is_empty());
}

#[test]
fn parameter_types_loaded_correctly() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn pair(_ left: i32, _ right: bool) -> i32 { left; right; 0 }",
    );
    let parsed_files = vec![root.clone()];

    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["pair"]);
    let env = &pipeline.body_envs.envs_for_owner(&owner)[0];

    let left = env
        .local_bindings
        .iter()
        .find_map(|(id, info)| {
            (info.kind == LocalKind::Parameter
                && env.local_types.get(id)
                    == Some(&Type::builtin(BuiltinType::I32)))
            .then_some(*id)
        })
        .expect("left parameter type should be loaded");
    let right = env
        .local_bindings
        .iter()
        .find_map(|(id, info)| {
            (info.kind == LocalKind::Parameter
                && env.local_types.get(id)
                    == Some(&Type::builtin(BuiltinType::Bool)))
            .then_some(*id)
        })
        .expect("right parameter type should be loaded");

    assert_ne!(left, right);
}

#[test]
fn local_type_mutability_loaded_correctly() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Client {} fn f() { let imm: i32 = 1; var mutc: Client = Client {}; }",
    );
    let parsed_files = vec![root.clone()];

    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);
    let client_id = pipeline
        .item_table
        .item_id_by_full_path(&["Client".to_string()])
        .expect("client item should exist");
    let env = &pipeline.body_envs.envs_for_owner(&owner)[0];

    let immutable_local = env
        .local_bindings
        .iter()
        .find(|(_, info)| {
            info.kind == LocalKind::LocalBinding
                && info.mutability == LocalMutability::Immutable
        })
        .map(|(id, _)| *id)
        .expect("expected immutable local");
    assert_eq!(
        env.local_types[&immutable_local],
        Type::builtin(BuiltinType::I32)
    );

    let mutable_local = env
        .local_bindings
        .iter()
        .find(|(_, info)| {
            info.kind == LocalKind::LocalBinding
                && info.mutability == LocalMutability::Mutable
        })
        .map(|(id, _)| *id)
        .expect("expected mutable local");
    assert_eq!(
        env.local_types[&mutable_local],
        Type::named(client_id, NamedTypeKind::Struct)
    );
}

#[test]
fn expected_return_type_propagation() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct S { init(_ x: i32) { x; } } fn mk() -> i32 { 1 }",
    );
    let parsed_files = vec![root.clone()];

    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    let mk_owner = owner_for(&pipeline.item_table, &["mk"]);
    let mk_env = &pipeline.body_envs.envs_for_owner(&mk_owner)[0];
    assert_eq!(mk_env.expected_return_type, Type::builtin(BuiltinType::I32));

    let s_owner = owner_for(&pipeline.item_table, &["S"]);
    let init_env = pipeline
        .body_envs
        .envs_for_owner(&s_owner)
        .iter()
        .find(|env| env.kind == core_x::frontend::BodyKind::Initializer)
        .expect("initializer env should exist");
    assert_eq!(init_env.expected_return_type, Type::void());
}

#[test]
fn owner_keyed_storage_is_stable() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn a(_ x: i32) -> i32 { x } fn b(_ y: bool) -> bool { y }",
    );
    let parsed_files = vec![root.clone()];

    let first = run_pipeline(&db, &parsed_files, root.file_id);
    let second = run_pipeline(&db, &parsed_files, root.file_id);
    assert_eq!(first.body_envs, second.body_envs);

    let a_owner = owner_for(&first.item_table, &["a"]);
    let b_owner = owner_for(&first.item_table, &["b"]);
    assert_eq!(first.body_envs.envs_for_owner(&a_owner).len(), 1);
    assert_eq!(first.body_envs.envs_for_owner(&b_owner).len(), 1);
    assert_eq!(first.body_envs.len(), 2);
}
