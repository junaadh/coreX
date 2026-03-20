use core_x::frontend::resolver::{
    NamedImportRoot, ScopeResolver, resolve_project_imports,
    resolve_project_imports_with_named_roots_and_diagnostics,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    ExpansionOptions, ExprCheckIssueKind, FrontendContext, analyze_semantics,
    analyze_semantics_with_external_lookup, build_external_semantic_lookup,
    resolve_hir_semantic_input,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn parse_sources(
    sources: &[(&str, &str)],
) -> (
    SourceDb,
    Vec<core_x::frontend::DesugaredFile>,
    BTreeMap<String, FileId>,
) {
    let mut frontend = FrontendContext::new();
    let mut file_ids = BTreeMap::new();

    for &(path, source) in sources {
        let file_id = frontend.add_file(path, source);
        file_ids.insert(path.to_string(), file_id);
    }
    let ordered_file_ids = frontend.ordered_file_ids().to_vec();
    let parsed_files = frontend
        .pre_resolution_pipeline(&ordered_file_ids, ExpansionOptions::default())
        .expect("pre-resolution pipeline should succeed");
    let db = frontend.into_db();

    (db, parsed_files, file_ids)
}

#[test]
fn semantic_hir_valid_function_call_resolves_correctly() {
    let (db, parsed_files, file_ids) = parse_sources(&[
        ("src/root.cx", "scope net; scope app;"),
        ("src/net.cx", "fn helper() -> i32 { 1 }"),
        (
            "src/app.cx",
            "use root::net::helper as remote; fn run() -> i32 { remote() }",
        ),
    ]);
    let root_file_id = file_ids["src/root.cx"];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let semantic = analyze_semantics(
        &db,
        resolve_hir_semantic_input(&graph, &parsed_files, &imports),
    );

    assert!(!semantic.expr_types.issues.iter().any(|issue| matches!(
        issue.kind,
        ExprCheckIssueKind::InvalidCallCallee
    )));
}

#[test]
fn semantic_resolved_hir_invalid_call_target_is_reported() {
    let (db, parsed_files, file_ids) = parse_sources(&[(
        "src/root.cx",
        "fn run() -> i32 { let x = 1; x(); 0 }",
    )]);
    let root_file_id = file_ids["src/root.cx"];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let semantic = analyze_semantics(
        &db,
        resolve_hir_semantic_input(&graph, &parsed_files, &imports),
    );

    assert!(!semantic.hir.hir_path_table.is_empty());
    assert!(semantic.expr_types.issues.iter().any(|issue| matches!(
        issue.kind,
        ExprCheckIssueKind::InvalidCallCallee
    )));
}

#[test]
fn semantic_resolved_hir_cross_target_library_to_binary_call_still_works() {
    let (db, parsed_files, file_ids) = parse_sources(&[
        ("src/root.cx", "pub fn shared_logic() -> i32 { 1 }"),
        (
            "src/main.cx",
            "use app::shared_logic; fn main() -> i32 { shared_logic() }",
        ),
    ]);
    let root_file_id = file_ids["src/root.cx"];
    let main_file_id = file_ids["src/main.cx"];

    let resolver = ScopeResolver::new(&db, &parsed_files);
    let library_graph = resolver
        .resolve_library_root(root_file_id)
        .expect("library graph");
    let binary_graph = resolver
        .resolve_binary_root(main_file_id)
        .expect("binary graph");

    let mut named_roots = BTreeMap::new();
    let path_by_file_id = file_ids
        .iter()
        .map(|(path, file_id)| (*file_id, PathBuf::from(path)))
        .collect();
    named_roots.insert(
        "app".to_string(),
        NamedImportRoot::LoadedLibrary {
            graph: library_graph,
            parsed_files: parsed_files.clone(),
            path_by_file_id,
        },
    );

    let (_, imports, _) =
        resolve_project_imports_with_named_roots_and_diagnostics(
            &binary_graph,
            &parsed_files,
            &named_roots,
            &db,
        );
    let external_lookup = build_external_semantic_lookup(
        &db,
        &named_roots,
        &binary_graph,
        &parsed_files,
    );
    let semantic = analyze_semantics_with_external_lookup(
        &db,
        resolve_hir_semantic_input(&binary_graph, &parsed_files, &imports),
        &external_lookup,
    );

    assert!(!semantic.expr_types.issues.iter().any(|issue| matches!(
        issue.kind,
        ExprCheckIssueKind::InvalidCallCallee
    )));
    assert!(semantic.expr_types.issues.iter().all(|issue| !matches!(
        issue.kind,
        ExprCheckIssueKind::MissingResolvedReference { .. }
    )));
}

#[test]
fn semantic_constructor_call_resolves_associated_initializer_member() {
    let (db, parsed_files, file_ids) = parse_sources(&[(
        "src/root.cx",
        "struct Point { x: i32, y: i32, init(_ x: i32, _ y: i32) -> Self { Self { x, y } } } fn run() { Point(10, 20); }",
    )]);
    let root_file_id = file_ids["src/root.cx"];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let semantic = analyze_semantics(
        &db,
        resolve_hir_semantic_input(&graph, &parsed_files, &imports),
    );

    let point_init_path = vec!["Point".to_string(), "init".to_string()];
    assert!(!semantic.expr_types.issues.iter().any(|issue| {
        matches!(
            &issue.kind,
            ExprCheckIssueKind::MissingResolvedReference { segments } if segments == &point_init_path
        )
    }));
    assert!(!semantic.expr_types.issues.iter().any(|issue| matches!(
        issue.kind,
        ExprCheckIssueKind::InvalidCallCallee
    )));
}

#[test]
fn semantic_explicit_initializer_call_resolves_associated_member() {
    let (db, parsed_files, file_ids) = parse_sources(&[(
        "src/root.cx",
        "struct Point { x: i32, y: i32, init(_ x: i32, _ y: i32) -> Self { Self { x, y } } } fn run() { Point::init(10, 20); }",
    )]);
    let root_file_id = file_ids["src/root.cx"];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let semantic = analyze_semantics(
        &db,
        resolve_hir_semantic_input(&graph, &parsed_files, &imports),
    );

    let point_init_path = vec!["Point".to_string(), "init".to_string()];
    assert!(!semantic.expr_types.issues.iter().any(|issue| {
        matches!(
            &issue.kind,
            ExprCheckIssueKind::MissingResolvedReference { segments } if segments == &point_init_path
        )
    }));
    assert!(!semantic.expr_types.issues.iter().any(|issue| matches!(
        issue.kind,
        ExprCheckIssueKind::InvalidCallCallee
    )));
}
