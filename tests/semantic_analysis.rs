use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{ScopeResolver, resolve_project_imports};
use core_x::frontend::source::SourceDb;
use core_x::frontend::{NamedTypeKind, Type, analyze_semantics};

fn analyze_sources(
    sources: &[(&str, &str)],
    root_path: &str,
) -> (SourceDb, core_x::frontend::SemanticAnalysis) {
    let mut db = SourceDb::new();
    let mut parsed_files = Vec::with_capacity(sources.len());
    let mut root_file_id = None;

    for &(path, source) in sources {
        let file_id = db.add_file(path, source);
        if path == root_path {
            root_file_id = Some(file_id);
        }
        let file = db.file(file_id).expect("source file should exist");
        let parsed = parse_source_file_from_source_file(file)
            .expect("parse should work");
        assert!(parsed.diagnostics.is_empty(), "strict parse diagnostics");
        parsed_files.push(parsed);
    }

    let root_file_id = root_file_id.expect("root file should be present");
    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let (_, imports) =
        resolve_project_imports(&graph, &parsed_files).expect("imports");
    let semantic = analyze_semantics(&db, &graph, &parsed_files, &imports);
    (db, semantic)
}

fn has_message(
    diagnostics: &core_x::frontend::DiagnosticsBag,
    message: &str,
) -> bool {
    diagnostics
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.message == message)
}

#[test]
fn semantic_analysis_driver_runs_full_pass_chain_successfully() {
    let (_, semantic) = analyze_sources(
        &[(
            "src/root.cx",
            "struct Client {} fn make(_ c: Client) -> Client { let local = c; local }",
        )],
        "src/root.cx",
    );

    assert!(!semantic.global_items.is_empty());
    assert!(!semantic.declarations.is_empty());
    assert!(!semantic.signatures.is_empty());
    assert!(!semantic.typed_items.is_empty());
    assert!(!semantic.resolved_bodies.is_empty());
    assert!(!semantic.body_envs.is_empty());
    assert!(!semantic.expr_types.is_empty());
    assert!(!semantic.stmt_types.is_empty());
    assert!(!semantic.control_flow.is_empty());
    assert!(!semantic.typed_bodies.is_empty());
}

#[test]
fn semantic_analysis_result_contains_expected_sub_results() {
    let (_, semantic) = analyze_sources(
        &[(
            "src/root.cx",
            "struct Client {} fn make(_ c: Client) -> Client { c }",
        )],
        "src/root.cx",
    );

    let make_id = semantic
        .global_items
        .item_id_by_full_path(&["make".to_string()])
        .expect("make item id");
    let client_id = semantic
        .global_items
        .item_id_by_full_path(&["Client".to_string()])
        .expect("Client item id");
    let signature = semantic
        .signatures
        .function(make_id)
        .expect("typed function signature");
    assert_eq!(
        signature.param_types,
        vec![Type::Named {
            item_id: client_id,
            kind: NamedTypeKind::Struct,
        }]
    );
    assert_eq!(
        signature.return_type,
        Some(Type::Named {
            item_id: client_id,
            kind: NamedTypeKind::Struct,
        })
    );
    assert!(semantic.typed_items.function(make_id).is_some());
}

#[test]
fn semantic_analysis_is_deterministic() {
    let source = "struct A {} fn f(_ a: A) -> A { a }";
    let (_, first) = analyze_sources(&[("src/root.cx", source)], "src/root.cx");
    let (_, second) =
        analyze_sources(&[("src/root.cx", source)], "src/root.cx");
    assert_eq!(first, second);
}

#[test]
fn semantic_analysis_aggregates_diagnostics() {
    let (_, semantic) = analyze_sources(
        &[(
            "src/root.cx",
            "fn g(x: i32) -> i32 { x } fn f() -> i32 { let x: bool = 1; g(); while 1 { break; } true }",
        )],
        "src/root.cx",
    );

    assert!(semantic.diagnostics.len() >= 3);
    assert!(has_message(&semantic.diagnostics, "type mismatch"));
    assert!(has_message(&semantic.diagnostics, "invalid call arity"));
    assert!(has_message(&semantic.diagnostics, "invalid condition type"));
}

#[test]
fn semantic_analysis_issues_view_matches_stage_issue_tables() {
    let (_, semantic) = analyze_sources(
        &[("src/root.cx", "fn f(x: Missing) -> Missing { return x; }")],
        "src/root.cx",
    );

    let issues = semantic.issues();
    assert_eq!(issues.signature, semantic.signatures.issues.as_slice());
    assert_eq!(issues.typed_item, semantic.typed_items.issues.as_slice());
    assert_eq!(issues.body_env, semantic.body_envs.issues.as_slice());
    assert_eq!(issues.expr, semantic.expr_types.issues.as_slice());
    assert_eq!(issues.stmt, semantic.stmt_types.issues.as_slice());
    assert_eq!(issues.control_flow, semantic.control_flow.issues.as_slice());
    assert_eq!(issues.typed_body, semantic.typed_bodies.issues.as_slice());
    assert!(!issues.is_empty());
}

#[test]
fn semantic_analysis_uses_real_scope_file_ids_in_semantic_ownership_paths() {
    let (db, semantic) = analyze_sources(
        &[
            ("src/root.cx", "scope net; fn root_fn() -> i32 { 1 }"),
            (
                "src/net.cx",
                "struct Client {} fn make(_ c: Client) -> Client { c }",
            ),
            ("src/unused.cx", "fn ignored() {}"),
        ],
        "src/root.cx",
    );

    for item in semantic.global_items.iter() {
        assert!(
            db.file(item.defining_file_id).is_some(),
            "missing defining file id {}",
            item.defining_file_id.raw()
        );
        assert!(
            db.file(item.containing_scope_file_id).is_some(),
            "missing containing scope file id {}",
            item.containing_scope_file_id.raw()
        );
    }

    for body in semantic.resolved_bodies.iter() {
        assert!(
            db.file(body.containing_scope_file_id).is_some(),
            "missing body containing file id {}",
            body.containing_scope_file_id.raw()
        );
    }

    for env in semantic.body_envs.iter() {
        assert!(
            db.file(env.containing_scope_file_id).is_some(),
            "missing environment file id {}",
            env.containing_scope_file_id.raw()
        );
    }

    for body in semantic.typed_bodies.iter() {
        assert!(
            db.file(body.containing_scope_file_id).is_some(),
            "missing typed body file id {}",
            body.containing_scope_file_id.raw()
        );
    }

    for issue in &semantic.signatures.issues {
        if let Some(file_id) = issue.containing_scope_file_id {
            assert!(
                db.file(file_id).is_some(),
                "missing signature issue file id {}",
                file_id.raw()
            );
        }
    }
}
