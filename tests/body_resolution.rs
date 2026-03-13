use core_x::frontend::ParsedFile;
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, LocalKind, ResolvedBodyRef, ScopeGraph,
    ScopeResolver, resolve_bodies, resolve_declaration_types,
    resolve_project_imports,
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

fn resolve_library_graph(
    db: &SourceDb,
    parsed_files: &[ParsedFile],
    root_file_id: FileId,
) -> ScopeGraph {
    ScopeResolver::new(db, parsed_files)
        .resolve_library_root(root_file_id)
        .expect("library scope resolution should succeed")
}

struct ResolvedPipeline {
    item_table: GlobalItemTable,
    bodies: core_x::frontend::ResolvedBodyTable,
}

fn resolve_pipeline(
    db: &SourceDb,
    parsed_files: &[ParsedFile],
    root_file_id: FileId,
) -> ResolvedPipeline {
    let graph = resolve_library_graph(db, parsed_files, root_file_id);
    let item_table = GlobalItemTable::collect(&graph, parsed_files);
    let (_, imports) =
        resolve_project_imports(&graph, parsed_files).expect("imports");
    let declarations =
        resolve_declaration_types(&graph, parsed_files, &imports, &item_table);
    let bodies = resolve_bodies(
        &graph,
        parsed_files,
        &imports,
        &item_table,
        &declarations,
    );
    ResolvedPipeline { item_table, bodies }
}

fn item_owner(table: &GlobalItemTable, path: &[&str]) -> DeclarationOwner {
    let path = path
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    let item_id = table
        .item_id_by_full_path(&path)
        .unwrap_or_else(|| panic!("missing item path {path:?}"));
    DeclarationOwner::Item(item_id)
}

#[test]
fn resolves_function_parameters() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn f(_ x: i32) { x; }");
    let parsed_files = vec![root.clone()];

    let resolved = resolve_pipeline(&db, &parsed_files, root.file_id);
    let owner = item_owner(&resolved.item_table, &["f"]);
    let bodies = resolved.bodies.bodies_for_owner(&owner);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].locals.len(), 1);
    assert_eq!(bodies[0].locals[0].kind, LocalKind::Parameter);
    assert_eq!(bodies[0].locals[0].name, "x");
    let x_ref = bodies[0]
        .references
        .iter()
        .find(|reference| reference.segments == vec!["x".to_string()])
        .expect("x reference");
    assert_eq!(
        x_ref.resolved,
        ResolvedBodyRef::Local(bodies[0].locals[0].id)
    );
}

#[test]
fn resolves_local_bindings() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() { let x = 1; var y = x; x; y; }",
    );
    let parsed_files = vec![root.clone()];

    let resolved = resolve_pipeline(&db, &parsed_files, root.file_id);
    let owner = item_owner(&resolved.item_table, &["f"]);
    let body = &resolved.bodies.bodies_for_owner(&owner)[0];
    let x_local = body
        .locals
        .iter()
        .find(|local| local.name == "x")
        .expect("x local");
    let y_local = body
        .locals
        .iter()
        .find(|local| local.name == "y")
        .expect("y local");
    assert_eq!(x_local.kind, LocalKind::LocalBinding);
    assert_eq!(y_local.kind, LocalKind::LocalBinding);

    let y_ref = body
        .references
        .iter()
        .find(|reference| reference.segments == vec!["y".to_string()])
        .expect("y ref");
    assert_eq!(y_ref.resolved, ResolvedBodyRef::Local(y_local.id));
}

#[test]
fn nested_block_shadowing_uses_nearest_binding() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f(_ x: i32) { let x = 1; { let x = 2; x; }; x; }",
    );
    let parsed_files = vec![root.clone()];

    let resolved = resolve_pipeline(&db, &parsed_files, root.file_id);
    let owner = item_owner(&resolved.item_table, &["f"]);
    let body = &resolved.bodies.bodies_for_owner(&owner)[0];
    let mut x_locals = body
        .locals
        .iter()
        .filter(|local| local.name == "x")
        .collect::<Vec<_>>();
    x_locals.sort_by_key(|local| local.declared_span.start);
    assert_eq!(x_locals.len(), 3);

    let mut x_refs = body
        .references
        .iter()
        .filter(|reference| reference.segments == vec!["x".to_string()])
        .collect::<Vec<_>>();
    x_refs.sort_by_key(|reference| reference.span.start);
    assert_eq!(x_refs.len(), 2);
    assert_eq!(x_refs[0].resolved, ResolvedBodyRef::Local(x_locals[2].id));
    assert_eq!(x_refs[1].resolved, ResolvedBodyRef::Local(x_locals[1].id));
}

#[test]
fn falls_back_to_imported_and_top_level_items() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope net; scope app; struct LocalType {} fn check() { LocalType; }",
    );
    let net = add_and_parse(&mut db, "src/net.cx", "struct Client {}");
    let app = add_and_parse(
        &mut db,
        "src/app.cx",
        "use root::net::Client; fn f() { Client; }",
    );
    let parsed_files = vec![root.clone(), net.clone(), app.clone()];

    let resolved = resolve_pipeline(&db, &parsed_files, root.file_id);
    let check_owner = item_owner(&resolved.item_table, &["check"]);
    let check_body = &resolved.bodies.bodies_for_owner(&check_owner)[0];
    let local_ref = check_body
        .references
        .iter()
        .find(|reference| reference.segments == vec!["LocalType".to_string()])
        .expect("LocalType ref");
    let local_item = resolved
        .item_table
        .item_id_by_full_path(&["LocalType".to_string()])
        .expect("LocalType item");
    assert_eq!(local_ref.resolved, ResolvedBodyRef::Item(local_item));

    let f_owner = item_owner(&resolved.item_table, &["app", "f"]);
    let f_body = &resolved.bodies.bodies_for_owner(&f_owner)[0];
    let import_ref = f_body
        .references
        .iter()
        .find(|reference| reference.segments == vec!["Client".to_string()])
        .expect("Client ref");
    let import_item = resolved
        .item_table
        .item_id_by_full_path(&["net".to_string(), "Client".to_string()])
        .expect("Client item");
    assert_eq!(import_ref.resolved, ResolvedBodyRef::Import(import_item));
}

#[test]
fn resolves_body_references_inside_impl_methods() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Client {} impl Client { fn make(&self, _ input: Client) { input; Client; self; } }",
    );
    let parsed_files = vec![root.clone()];
    let resolved = resolve_pipeline(&db, &parsed_files, root.file_id);

    let impl_owner = DeclarationOwner::Impl {
        scope_file_id: root.file_id,
        impl_index: 0,
    };
    let impl_bodies = resolved.bodies.bodies_for_owner(&impl_owner);
    assert_eq!(impl_bodies.len(), 1);
    let body = &impl_bodies[0];
    let input_local = body
        .locals
        .iter()
        .find(|local| local.name == "input")
        .expect("input local");
    let self_local = body
        .locals
        .iter()
        .find(|local| local.name == "self")
        .expect("self local");
    let input_ref = body
        .references
        .iter()
        .find(|reference| reference.segments == vec!["input".to_string()])
        .expect("input ref");
    assert_eq!(input_ref.resolved, ResolvedBodyRef::Local(input_local.id));

    let self_ref = body
        .references
        .iter()
        .find(|reference| reference.segments == vec!["self".to_string()])
        .expect("self ref");
    assert_eq!(self_ref.resolved, ResolvedBodyRef::Local(self_local.id));
}

#[test]
fn resolves_body_references_in_nested_scope_files() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope app;");
    let app = add_and_parse(&mut db, "src/app.cx", "scope inner;");
    let inner = add_and_parse(
        &mut db,
        "src/app/inner.cx",
        "struct Value {} fn g() { Value; }",
    );
    let parsed_files = vec![root.clone(), app.clone(), inner.clone()];

    let resolved = resolve_pipeline(&db, &parsed_files, root.file_id);
    let g_owner = item_owner(&resolved.item_table, &["app", "inner", "g"]);
    let g_body = &resolved.bodies.bodies_for_owner(&g_owner)[0];
    let value_item = resolved
        .item_table
        .item_id_by_full_path(&[
            "app".to_string(),
            "inner".to_string(),
            "Value".to_string(),
        ])
        .expect("Value item");
    let value_ref = g_body
        .references
        .iter()
        .find(|reference| reference.segments == vec!["Value".to_string()])
        .expect("Value ref");
    assert_eq!(value_ref.resolved, ResolvedBodyRef::Item(value_item));
}

#[test]
fn local_id_assignment_is_deterministic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn a(_ p: i32) { let x = p; } fn b() { let y = 1; }",
    );
    let parsed_files = vec![root.clone()];

    let first = resolve_pipeline(&db, &parsed_files, root.file_id);
    let second = resolve_pipeline(&db, &parsed_files, root.file_id);
    let first_ids = first
        .bodies
        .iter()
        .flat_map(|body| body.locals.iter().map(|local| local.id.raw()))
        .collect::<Vec<_>>();
    let second_ids = second
        .bodies
        .iter()
        .flat_map(|body| body.locals.iter().map(|local| local.id.raw()))
        .collect::<Vec<_>>();
    assert_eq!(first_ids, second_ids);
}

#[test]
fn unresolved_body_names_are_recorded() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn f() { missing; }");
    let parsed_files = vec![root.clone()];
    let resolved = resolve_pipeline(&db, &parsed_files, root.file_id);
    let owner = item_owner(&resolved.item_table, &["f"]);
    let body = &resolved.bodies.bodies_for_owner(&owner)[0];

    assert_eq!(body.unresolved_references.len(), 1);
    assert_eq!(
        body.unresolved_references[0].segments,
        vec!["missing".to_string()]
    );
    let missing_ref = body
        .references
        .iter()
        .find(|reference| reference.segments == vec!["missing".to_string()])
        .expect("missing reference");
    assert_eq!(missing_ref.resolved, ResolvedBodyRef::Unresolved);
}

#[test]
fn body_resolution_uses_declaration_owner_identities() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn top() { top; } struct S { fn m() { m; } } impl S { fn i() { i; } }",
    );
    let parsed_files = vec![root.clone()];
    let resolved = resolve_pipeline(&db, &parsed_files, root.file_id);

    let top_owner = item_owner(&resolved.item_table, &["top"]);
    assert_eq!(resolved.bodies.bodies_for_owner(&top_owner).len(), 1);

    let struct_owner = item_owner(&resolved.item_table, &["S"]);
    assert_eq!(resolved.bodies.bodies_for_owner(&struct_owner).len(), 1);

    let impl_owner = DeclarationOwner::Impl {
        scope_file_id: root.file_id,
        impl_index: 0,
    };
    assert_eq!(resolved.bodies.bodies_for_owner(&impl_owner).len(), 1);
}

#[test]
fn resolved_item_and_import_refs_are_backed_by_global_item_table() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope dep; scope app;");
    let dep = add_and_parse(&mut db, "src/dep.cx", "struct T {}");
    let app = add_and_parse(
        &mut db,
        "src/app.cx",
        "use root::dep::T; fn run() { T; }",
    );
    let parsed_files = vec![root.clone(), dep.clone(), app.clone()];

    let resolved = resolve_pipeline(&db, &parsed_files, root.file_id);
    let owner = item_owner(&resolved.item_table, &["app", "run"]);
    let body = &resolved.bodies.bodies_for_owner(&owner)[0];
    let t_ref = body
        .references
        .iter()
        .find(|reference| reference.segments == vec!["T".to_string()])
        .expect("T reference");
    let t_item = resolved
        .item_table
        .item_id_by_full_path(&["dep".to_string(), "T".to_string()])
        .expect("T item");
    assert_eq!(t_ref.resolved, ResolvedBodyRef::Import(t_item));
    assert!(resolved.item_table.get(t_item).is_some());
}
