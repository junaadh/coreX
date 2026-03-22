use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ResolvedBodyTable, ScopeGraph,
    ScopeResolver, resolve_bodies, resolve_declaration_types,
    resolve_project_imports,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    BuiltinType, NamedTypeKind, Type, build_body_type_environments,
    build_typed_item_table, type_declaration_signatures,
};
use core_x::midend::{
    BodyInferIssueKind, BodyInferenceTable, infer_body_types,
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
    inferred: BodyInferenceTable,
}

fn run_pipeline(
    db: &SourceDb,
    parsed_files: &[core_x::frontend::DesugaredFile],
    root_file_id: FileId,
) -> Pipeline {
    let graph = resolve_library_graph(db, parsed_files, root_file_id);
    let item_table = GlobalItemTable::collect(&graph, parsed_files);
    let hir = core_x::frontend::SemanticHirInput::build(
        &graph,
        parsed_files,
        &item_table,
    );
    let (_, imports) =
        resolve_project_imports(&graph, parsed_files).expect("imports");
    let declarations =
        resolve_declaration_types(&graph, parsed_files, &imports, &item_table);
    let signatures = type_declaration_signatures(&hir, &item_table);
    let typed_items = build_typed_item_table(&item_table, &signatures);
    let bodies = resolve_bodies(
        &graph,
        parsed_files,
        &imports,
        &item_table,
        &declarations,
    );
    let body_envs = build_body_type_environments(&hir, &bodies, &typed_items);
    let inferred = infer_body_types(&hir, &typed_items, &bodies, &body_envs);
    Pipeline {
        item_table,
        bodies,
        inferred,
    }
}

fn owner_for(table: &GlobalItemTable, path: &[&str]) -> DeclarationOwner {
    let full_path = path
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    let item_id = table
        .item_id_by_full_path(&full_path)
        .unwrap_or_else(|| panic!("missing item path {full_path:?}"));
    DeclarationOwner::Item(item_id)
}

#[test]
fn infers_unannotated_local_from_literal_and_tail() {
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
        .expect("x local");

    let root_ty = pipeline.inferred.root_type(&owner, 0);
    if root_ty != Some(&Type::builtin(BuiltinType::I32)) {
        panic!(
            "root type mismatch: {root_ty:?}, issues: {:?}",
            pipeline.inferred.issues
        );
    }
    assert_eq!(
        pipeline
            .inferred
            .local_type_for_resolved_local(&owner, 0, x_local.id),
        Some(&Type::builtin(BuiltinType::I32))
    );
}

#[test]
fn annotation_constrains_literal_inference() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i64 { let x: i64 = 1; x }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);
    let body = &pipeline.bodies.bodies_for_owner(&owner)[0];
    let x_local = body
        .locals
        .iter()
        .find(|local| local.name == "x")
        .expect("x local");

    assert_eq!(
        pipeline.inferred.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I64))
    );
    assert_eq!(
        pipeline
            .inferred
            .local_type_for_resolved_local(&owner, 0, x_local.id),
        Some(&Type::builtin(BuiltinType::I64))
    );
}

#[test]
fn unconstrained_local_requires_annotation() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn f() { let x; }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);
    let body = &pipeline.bodies.bodies_for_owner(&owner)[0];
    let x_local = body
        .locals
        .iter()
        .find(|local| local.name == "x")
        .expect("x local");

    assert!(pipeline.inferred.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            BodyInferIssueKind::RequiresExplicitLocalTypeAnnotation {
                resolved_local_id: Some(local_id),
                ..
            } if local_id == x_local.id
        )
    }));
}

#[test]
fn if_else_propagates_expected_return_type() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { if true { 1 } else { 2 } }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.inferred.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32)),
        "issues: {:?}",
        pipeline.inferred.issues
    );
}

#[test]
fn associated_initializer_call_infers_local_nominal_type() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct Point { x: i32, y: i32, init(_ x: i32, _ y: i32) -> Self { Self { x, y } } } fn f() { let p = Point::init(10, 20); }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);
    let body = &pipeline.bodies.bodies_for_owner(&owner)[0];
    let p_local = body
        .locals
        .iter()
        .find(|local| local.name == "p")
        .expect("p local");
    let point_item_id = pipeline
        .item_table
        .item_id_by_full_path(&["Point".to_string()])
        .expect("Point item id");

    assert_eq!(
        pipeline
            .inferred
            .local_type_for_resolved_local(&owner, 0, p_local.id),
        Some(&Type::named(point_item_id, NamedTypeKind::Struct))
    );
}

#[test]
fn assignment_constrains_local_type() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { var x = 1; x = 2; x }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);
    let body = &pipeline.bodies.bodies_for_owner(&owner)[0];
    let x_local = body
        .locals
        .iter()
        .find(|local| local.name == "x")
        .expect("x local");

    assert_eq!(
        pipeline
            .inferred
            .local_type_for_resolved_local(&owner, 0, x_local.id),
        Some(&Type::builtin(BuiltinType::I32))
    );
    assert_eq!(
        pipeline.inferred.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32))
    );
}

#[test]
fn integer_literal_defaults_to_i32_without_constraints() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn f() { let x = 1; }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);
    let body = &pipeline.bodies.bodies_for_owner(&owner)[0];
    let x_local = body
        .locals
        .iter()
        .find(|local| local.name == "x")
        .expect("x local");

    assert_eq!(
        pipeline
            .inferred
            .local_type_for_resolved_local(&owner, 0, x_local.id),
        Some(&Type::builtin(BuiltinType::I32))
    );
}

#[test]
fn call_argument_constrains_local_literal_type() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn takes_i64(_ n: i64) {} fn f() { let x = 1; takes_i64(x); }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);
    let body = &pipeline.bodies.bodies_for_owner(&owner)[0];
    let x_local = body
        .locals
        .iter()
        .find(|local| local.name == "x")
        .expect("x local");

    assert_eq!(
        pipeline
            .inferred
            .local_type_for_resolved_local(&owner, 0, x_local.id),
        Some(&Type::builtin(BuiltinType::I64))
    );
}

#[test]
fn method_call_infers_from_receiver_and_args() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct A { init() -> Self { Self {} } fn m(&self, _ n: i32) -> i32 { n } fn m(&self, _ b: bool) -> i32 { 0 } } fn f() -> i32 { let a = A::init(); let b = true; a.m(b) }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.inferred.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32))
    );
    assert!(
        !pipeline.inferred.issues.iter().any(|issue| matches!(
            issue.kind,
            BodyInferIssueKind::AmbiguousCallCandidate { .. }
        )),
        "method call should resolve by type compatibility"
    );
}

#[test]
fn contextual_enum_case_inference_from_expected_type() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "enum MaybeI32 { none, some(i32) } fn f() -> MaybeI32 { .none } fn g() -> MaybeI32 { .some(1) }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let maybe_item_id = pipeline
        .item_table
        .item_id_by_full_path(&["MaybeI32".to_string()])
        .expect("MaybeI32 item id");
    let f_owner = owner_for(&pipeline.item_table, &["f"]);
    let g_owner = owner_for(&pipeline.item_table, &["g"]);

    assert_eq!(
        pipeline.inferred.root_type(&f_owner, 0),
        Some(&Type::named(maybe_item_id, NamedTypeKind::Enum))
    );
    assert_eq!(
        pipeline.inferred.root_type(&g_owner, 0),
        Some(&Type::named(maybe_item_id, NamedTypeKind::Enum))
    );
}

#[test]
fn scoped_enum_case_infers_from_unique_enum_in_scope() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "enum MaybeI32 { none, some(i32) } fn f() { let x = .none; }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);
    let body = &pipeline.bodies.bodies_for_owner(&owner)[0];
    let x_local = body
        .locals
        .iter()
        .find(|local| local.name == "x")
        .expect("x local");
    let maybe_item_id = pipeline
        .item_table
        .item_id_by_full_path(&["MaybeI32".to_string()])
        .expect("MaybeI32 item id");

    // Should successfully infer the type from the unique enum in scope
    assert_eq!(
        pipeline
            .inferred
            .local_type_for_resolved_local(&owner, 0, x_local.id),
        Some(&Type::named(maybe_item_id, NamedTypeKind::Enum))
    );
    // And should have no inference issues
    assert!(pipeline.inferred.issues.is_empty());
}

#[test]
fn ambiguous_method_call_reports_error() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "struct A { init() -> Self { Self {} } fn m(&self, _ n: i32) -> i32 { n } } impl A { fn m(&self, _ n: i32) -> i32 { n } } fn f() -> i32 { let a = A::init(); a.m(1) }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    assert!(pipeline.inferred.issues.iter().any(|issue| matches!(
        issue.kind,
        BodyInferIssueKind::AmbiguousCallCandidate { .. }
    )));
}

#[test]
#[ignore = "TODO: requires generic type inference for Vec<T>"]
fn vec_new_then_push_infers_element_type() {
    // TODO: enable once generic nominal instantiation is represented in semantic types.
    // Example target:
    // let mut v = Vec::new();
    // v.push(1_u8);
    // infer v : Vec<u8>
}
