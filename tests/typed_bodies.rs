use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    BodyKind, DeclarationOwner, GlobalItemTable, ResolvedBodyTable, ScopeGraph,
    ScopeResolver, resolve_bodies, resolve_declaration_types,
    resolve_project_imports,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    BuiltinType, Type, build_body_type_environments, build_typed_body_table,
    build_typed_item_table, check_control_flow_with_tables,
    check_expression_types, check_statements_with_expression_types,
    type_declaration_signatures,
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

fn add_and_parse(
    db: &mut SourceDb,
    path: &str,
    source: &str,
) -> core_x::frontend::DesugaredFile {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(parsed.diagnostics.is_empty(), "strict parse diagnostics");
    parsed_to_desugared(parsed)
}

fn resolve_library_graph(
    db: &SourceDb,
    parsed_files: &[core_x::frontend::DesugaredFile],
    root_file_id: FileId,
) -> ScopeGraph {
    ScopeResolver::new(db, parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph")
}

struct Pipeline {
    item_table: GlobalItemTable,
    bodies: ResolvedBodyTable,
    typed_bodies: core_x::frontend::TypedBodyTable,
}

fn run_pipeline(
    db: &SourceDb,
    parsed_files: &[core_x::frontend::DesugaredFile],
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
    let expr_types = check_expression_types(
        &graph,
        parsed_files,
        &item_table,
        &typed_items,
        &bodies,
        &body_envs,
    );
    let stmt_types = check_statements_with_expression_types(
        &graph,
        parsed_files,
        &item_table,
        &bodies,
        &body_envs,
        &expr_types,
    );
    let control_flow = check_control_flow_with_tables(
        &graph,
        parsed_files,
        &item_table,
        &bodies,
        &body_envs,
        &expr_types,
        &stmt_types,
    );
    let typed_bodies = build_typed_body_table(
        &bodies,
        &body_envs,
        &expr_types,
        &stmt_types,
        &control_flow,
    );
    Pipeline {
        item_table,
        bodies,
        typed_bodies,
    }
}

fn owner_for(item_table: &GlobalItemTable, path: &[&str]) -> DeclarationOwner {
    let full_path = path
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    let item_id = item_table
        .item_id_by_full_path(&full_path)
        .unwrap_or_else(|| panic!("missing item path {full_path:?}"));
    DeclarationOwner::Item(item_id)
}

#[test]
fn typed_body_stored_for_functions_and_methods_with_bodies() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { 1 } struct S { fn m() -> i32 { 2 } }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    let function_owner = owner_for(&pipeline.item_table, &["f"]);
    let function_bodies =
        pipeline.typed_bodies.bodies_for_owner(&function_owner);
    assert_eq!(function_bodies.len(), 1);
    assert_eq!(function_bodies[0].kind, BodyKind::Function);

    let struct_owner = owner_for(&pipeline.item_table, &["S"]);
    let struct_bodies = pipeline.typed_bodies.bodies_for_owner(&struct_owner);
    assert_eq!(struct_bodies.len(), 1);
    assert_eq!(struct_bodies[0].kind, BodyKind::Function);
}

#[test]
fn expression_type_lookup() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { let x = 1; x + 2 }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    let owner = owner_for(&pipeline.item_table, &["f"]);
    let body = &pipeline.typed_bodies.bodies_for_owner(&owner)[0];
    assert!(!body.expression_types.is_empty());
    let (expr_id, ty) = body
        .expression_types
        .iter()
        .next()
        .expect("at least one expression type");
    assert_eq!(pipeline.typed_bodies.expression_type(expr_id), Some(ty));
}

#[test]
fn local_type_lookup() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { let x = 1; x }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    let owner = owner_for(&pipeline.item_table, &["f"]);
    let body = &pipeline.bodies.bodies_for_owner(&owner)[0];
    let x_local = body
        .locals
        .iter()
        .find(|local| local.name == "x")
        .expect("x local binding");
    assert_eq!(
        pipeline.typed_bodies.local_type(&owner, 0, x_local.id),
        Some(&Type::builtin(BuiltinType::I32))
    );
}

#[test]
fn body_result_type_lookup() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { 1 }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    let owner = owner_for(&pipeline.item_table, &["f"]);
    assert_eq!(
        pipeline.typed_bodies.body_result_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32))
    );
}

#[test]
fn deterministic_keyed_storage() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn a() -> i32 { 1 } fn b() -> i32 { 2 }",
    );
    let parsed_files = vec![root.clone()];
    let first = run_pipeline(&db, &parsed_files, root.file_id);
    let second = run_pipeline(&db, &parsed_files, root.file_id);
    assert_eq!(first.typed_bodies, second.typed_bodies);
}

#[test]
fn compatibility_with_existing_body_and_owner_identities() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { 1 } struct S { fn m() -> i32 { 2 } }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    for body in pipeline.bodies.iter() {
        let typed_body = pipeline
            .typed_bodies
            .body(&body.owner, body.body_index)
            .expect("typed body should exist for resolved body");
        assert_eq!(typed_body.kind, body.kind);
        assert_eq!(
            typed_body.containing_scope_file_id,
            body.containing_scope_file_id
        );
    }
}
