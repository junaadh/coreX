use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ResolvedBodyTable, ScopeGraph,
    ScopeResolver, resolve_bodies, resolve_declaration_types,
    resolve_project_imports,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    BuiltinType, StatementKind, StmtCheckIssueKind, Type,
    build_body_type_environments, build_typed_item_table, check_statements,
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
    stmt_types: core_x::frontend::StatementTypeTable,
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
    let stmt_types = check_statements(&hir, &typed_items, &bodies, &body_envs);
    Pipeline {
        item_table,
        bodies,
        stmt_types,
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
fn inferred_local_binding_types() {
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
    assert_eq!(
        pipeline.stmt_types.local_type(&owner, 0, x_local.id),
        Some(&Type::builtin(BuiltinType::I32))
    );
}

#[test]
fn annotated_local_mismatch() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { let x: bool = 1; 0 }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    assert!(pipeline.stmt_types.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            StmtCheckIssueKind::AnnotatedLocalTypeMismatch { .. }
        )
    }));
}

#[test]
fn assignment_compatibility() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { var x: i32 = 1; x = true; 0 }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    assert!(pipeline.stmt_types.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            StmtCheckIssueKind::AssignmentTypeMismatch { .. }
        )
    }));
}

#[test]
fn expression_statements() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { 1 + 2; 0 }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    let stmt_ids = pipeline.stmt_types.stmt_ids_for_body(&owner, 0);
    assert!(!stmt_ids.is_empty());
    let first_stmt = pipeline
        .stmt_types
        .stmt_entry(&stmt_ids[0])
        .expect("statement entry");
    assert_eq!(first_stmt.kind, StatementKind::Expr);
    assert_eq!(first_stmt.ty, Type::builtin(BuiltinType::I32));
}

#[test]
fn return_statement_typing() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { return 1; }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    let stmt_ids = pipeline.stmt_types.stmt_ids_for_body(&owner, 0);
    assert!(!stmt_ids.is_empty());
    let return_stmt = pipeline
        .stmt_types
        .stmt_entry(&stmt_ids[0])
        .expect("statement entry");
    assert_eq!(return_stmt.kind, StatementKind::Return);
    assert_eq!(return_stmt.ty, Type::builtin(BuiltinType::I32));
    assert!(!pipeline.stmt_types.issues.iter().any(|issue| {
        matches!(issue.kind, StmtCheckIssueKind::ReturnTypeMismatch { .. })
    }));
}

#[test]
fn invalid_condition_type() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { while 1 { return 0; } 0 }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    assert!(pipeline.stmt_types.issues.iter().any(|issue| {
        matches!(issue.kind, StmtCheckIssueKind::InvalidConditionType { .. })
    }));
}

#[test]
fn semantic_side_table_updates_for_locals_statements() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { let a = 1; let b = a; b }",
    );
    let parsed_files = vec![root.clone()];
    let first = run_pipeline(&db, &parsed_files, root.file_id);
    let second = run_pipeline(&db, &parsed_files, root.file_id);
    assert_eq!(first.stmt_types, second.stmt_types);

    let owner = owner_for(&first.item_table, &["f"]);
    let body = &first.bodies.bodies_for_owner(&owner)[0];
    let a_local = body
        .locals
        .iter()
        .find(|local| local.name == "a")
        .expect("a local");
    let b_local = body
        .locals
        .iter()
        .find(|local| local.name == "b")
        .expect("b local");
    assert_eq!(
        first.stmt_types.local_type(&owner, 0, a_local.id),
        Some(&Type::builtin(BuiltinType::I32))
    );
    assert_eq!(
        first.stmt_types.local_type(&owner, 0, b_local.id),
        Some(&Type::builtin(BuiltinType::I32))
    );
    assert!(first.stmt_types.len() >= 2);
}
