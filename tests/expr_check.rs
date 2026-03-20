use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ScopeGraph, ScopeResolver,
    resolve_bodies, resolve_declaration_types, resolve_project_imports,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    BodyExprId, BuiltinType, ExprCheckIssueKind, HirExprId, HirExprKind, Type,
    build_body_type_environments, build_typed_item_table,
    check_expression_types, type_declaration_signatures,
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
    hir: core_x::frontend::SemanticHirInput,
    expr_types: core_x::frontend::ExpressionTypeTable,
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
    let expr_types =
        check_expression_types(&hir, &typed_items, &bodies, &body_envs);
    Pipeline {
        item_table,
        hir,
        expr_types,
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

fn typed_hir_expr_ids_for_body(
    pipeline: &Pipeline,
    owner: &DeclarationOwner,
    body_index: usize,
    predicate: impl Fn(&HirExprKind) -> bool,
) -> Vec<HirExprId> {
    let body_ref = pipeline
        .hir
        .body_ref(owner, body_index)
        .expect("body ref should exist");
    let module = pipeline
        .hir
        .hir_modules
        .get(&body_ref.file_id)
        .expect("HIR module should exist");

    module
        .exprs
        .iter()
        .filter_map(|(expr_id, expr)| {
            (predicate(&expr.kind)
                && pipeline
                    .expr_types
                    .expr_type_for_hir_expr(owner, body_index, *expr_id)
                    .is_some())
            .then_some(*expr_id)
        })
        .collect()
}

#[test]
fn literal_typing() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { 1 }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.expr_types.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32))
    );
    assert!(!pipeline.expr_types.is_empty());
}

#[test]
fn local_ref_typing() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { let x = 1; x }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.expr_types.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32))
    );
}

#[test]
fn binary_operator_typing() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { 1 + 2 }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.expr_types.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32))
    );
    assert!(pipeline.expr_types.issues.is_empty());
}

#[test]
fn call_typing() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn g() -> i32 { 1 } fn f() -> i32 { g() }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.expr_types.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32))
    );
}

#[test]
fn if_expression_branch_compatibility() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { if true { 1 } else { false } }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.expr_types.root_type(&owner, 0),
        Some(&Type::error())
    );
    assert!(pipeline.expr_types.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            ExprCheckIssueKind::IncompatibleIfBranches { .. }
        )
    }));
}

#[test]
fn block_result_typing() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { let x: i32 = 1; x }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.expr_types.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32))
    );
}

#[test]
fn error_propagation_after_mismatch() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { true + 1 }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.expr_types.root_type(&owner, 0),
        Some(&Type::error())
    );
    assert!(pipeline.expr_types.issues.iter().any(|issue| matches!(
        issue.kind,
        ExprCheckIssueKind::InvalidBinaryOp
    )));
}

#[test]
fn expression_result_side_table_behavior() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { let x = 1; let y = 2; x + y }",
    );
    let parsed_files = vec![root.clone()];
    let first = run_pipeline(&db, &parsed_files, root.file_id);
    let second = run_pipeline(&db, &parsed_files, root.file_id);
    assert_eq!(first.expr_types, second.expr_types);

    let owner = owner_for(&first.item_table, &["f"]);
    let expr_ids = first.expr_types.expr_ids_for_body(&owner, 0);
    assert!(!expr_ids.is_empty());
    let first_id = expr_ids[0].clone();
    assert!(first.expr_types.expr_type(&first_id).is_some());

    let synthetic = BodyExprId {
        owner,
        body_index: 0,
        hir_expr_id: HirExprId::new(u32::MAX),
    };
    assert!(first.expr_types.expr_type(&synthetic).is_none());
}

#[test]
fn shadowing_still_works_with_hir_path_resolution() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn takes_bool(_ b: bool) {} fn takes_i32(_ n: i32) -> i32 { n } fn f() -> i32 { let x: i32 = 1; { let x: bool = true; takes_bool(x); }; takes_i32(x) }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    let owner = owner_for(&pipeline.item_table, &["f"]);

    assert_eq!(
        pipeline.expr_types.root_type(&owner, 0),
        Some(&Type::builtin(BuiltinType::I32))
    );
    assert!(!pipeline.expr_types.issues.iter().any(|issue| {
        matches!(issue.kind, ExprCheckIssueKind::CallArgTypeMismatch { .. })
    }));
}

#[test]
fn function_call_validity_works_on_hir() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn g() -> i32 { 1 } fn ok() -> i32 { g() } fn bad() -> i32 { let x = 1; x() }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    let ok_owner = owner_for(&pipeline.item_table, &["ok"]);
    let ok_call_expr_ids =
        typed_hir_expr_ids_for_body(&pipeline, &ok_owner, 0, |kind| {
            matches!(kind, HirExprKind::Call { .. })
        });
    assert_eq!(ok_call_expr_ids.len(), 1);
    assert_eq!(
        pipeline.expr_types.expr_type_for_hir_expr(
            &ok_owner,
            0,
            ok_call_expr_ids[0]
        ),
        Some(&Type::builtin(BuiltinType::I32))
    );

    let bad_owner = owner_for(&pipeline.item_table, &["bad"]);
    let bad_call_expr_ids =
        typed_hir_expr_ids_for_body(&pipeline, &bad_owner, 0, |kind| {
            matches!(kind, HirExprKind::Call { .. })
        });
    assert_eq!(bad_call_expr_ids.len(), 1);
    assert_eq!(
        pipeline.expr_types.expr_type_for_hir_expr(
            &bad_owner,
            0,
            bad_call_expr_ids[0]
        ),
        Some(&Type::error())
    );
    assert!(pipeline.expr_types.issues.iter().any(|issue| {
        issue.owner == bad_owner
            && issue.body_index == 0
            && matches!(issue.kind, ExprCheckIssueKind::InvalidCallCallee)
    }));
}
