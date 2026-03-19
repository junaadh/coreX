use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ScopeGraph, ScopeResolver,
    resolve_bodies, resolve_declaration_types, resolve_project_imports,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    BodyControlFlowId, ControlFlowIssueKind, build_body_type_environments,
    build_typed_item_table, check_control_flow, type_declaration_signatures,
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
    control_flow: core_x::frontend::ControlFlowTable,
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
    let control_flow = check_control_flow(
        &graph,
        parsed_files,
        &item_table,
        &typed_items,
        &bodies,
        &body_envs,
    );
    Pipeline {
        item_table,
        control_flow,
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
fn correct_return_type() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { return 1; }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    let owner = owner_for(&pipeline.item_table, &["f"]);
    let id = BodyControlFlowId {
        owner,
        body_index: 0,
    };
    let body_result = pipeline
        .control_flow
        .body(&id)
        .expect("control-flow result should exist");
    assert!(body_result.is_compatible);
    assert!(!pipeline.control_flow.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            ControlFlowIssueKind::ReturnTypeMismatch { .. }
                | ControlFlowIssueKind::MissingReturnValue { .. }
                | ControlFlowIssueKind::UnexpectedReturnValue { .. }
        )
    }));
}

#[test]
fn incorrect_return_type() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { return true; }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    assert!(pipeline.control_flow.issues.iter().any(|issue| {
        matches!(issue.kind, ControlFlowIssueKind::ReturnTypeMismatch { .. })
    }));
}

#[test]
fn tail_expression_compatibility() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { true }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    assert!(pipeline.control_flow.issues.iter().any(|issue| {
        matches!(issue.kind, ControlFlowIssueKind::TailTypeMismatch { .. })
    }));
}

#[test]
fn branch_mismatch_in_if_expression() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { if true { 1 } else { false } }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    assert!(pipeline.control_flow.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            ControlFlowIssueKind::IfBranchTypeMismatch { .. }
        )
    }));
}

#[test]
fn void_return_function_behavior() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn ok() { return; } fn bad_return() { return 1; } fn bad_tail() { 1 }",
    );
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);

    let ok_owner = owner_for(&pipeline.item_table, &["ok"]);
    let ok_result = pipeline
        .control_flow
        .body(&BodyControlFlowId {
            owner: ok_owner,
            body_index: 0,
        })
        .expect("control-flow result for ok");
    assert!(ok_result.is_compatible);

    assert!(pipeline.control_flow.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            ControlFlowIssueKind::UnexpectedReturnValue { .. }
        )
    }));
    assert!(pipeline.control_flow.issues.iter().any(|issue| {
        matches!(issue.kind, ControlFlowIssueKind::UnexpectedTailValue { .. })
    }));
}

#[test]
fn return_less_body_mismatch_where_applicable() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "fn f() -> i32 { let x = 1; }");
    let parsed_files = vec![root.clone()];
    let pipeline = run_pipeline(&db, &parsed_files, root.file_id);
    assert!(pipeline.control_flow.issues.iter().any(|issue| {
        matches!(
            issue.kind,
            ControlFlowIssueKind::MissingTailExpression { .. }
        )
    }));
}
