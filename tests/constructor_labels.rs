//! Test that constructor syntax preserves argument labels

use core_x::frontend::ast::{Expr, Item};
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::source::SourceDb;
use core_x::frontend::{
    DesugaredFile, ExpansionOptions, desugar_files, expand_parsed_files,
};

fn parse_single_file(
    db: &mut SourceDb,
    path: &str,
    source: &str,
) -> Vec<core_x::frontend::ParsedFile> {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(
        parsed.diagnostics.is_empty(),
        "parse should not emit diagnostics"
    );
    vec![parsed]
}

fn expand_and_desugar(
    db: &SourceDb,
    parsed_files: &[core_x::frontend::ParsedFile],
) -> Vec<DesugaredFile> {
    let expanded =
        expand_parsed_files(db, parsed_files, ExpansionOptions::default());
    desugar_files(&expanded)
}

#[test]
fn test_constructor_with_labeled_arguments() {
    let source = r#"
        struct Point {
          x: i32,
          y: i32,
          init(x: i32, y y_inner: i32) {
            Self { x, y: y_inner }
          }
        }

        fn test() {
          Point(x: 1, y: 2)
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    // Find the test function and check the expression
    for item in &desugared[0].ast.items {
        if let Item::Function(func) = &item.node {
            if func.node.name == "test" {
                if let Some(tail) = &func.node.body.tail_expr {
                    // Should be a Call expression (after desugaring)
                    if let Expr::Call { callee, args, .. } = &tail.node {
                        // Should be NamespaceAccess to init
                        if let Expr::NamespaceAccess { member, .. } =
                            &callee.node
                        {
                            assert_eq!(member, "init");
                            // Should have two arguments with labels
                            assert_eq!(args.len(), 2);
                            assert_eq!(args[0].label, Some("x".to_string()));
                            assert_eq!(args[1].label, Some("y".to_string()));
                            return; // Test passed!
                        }
                    }
                }
            }
        }
    }
    panic!("Could not find expected constructor call");
}

#[test]
fn test_constructor_with_positional_arguments() {
    let source = r#"
        struct Point {
          x: i32,
          y: i32,
          init(x: i32, y y_inner: i32) {
            Self { x, y: y_inner }
          }
        }

        fn test() {
          Point(1, 2)
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    for item in &desugared[0].ast.items {
        if let Item::Function(func) = &item.node {
            if func.node.name == "test" {
                if let Some(tail) = &func.node.body.tail_expr {
                    if let Expr::Call { callee, args, .. } = &tail.node {
                        if let Expr::NamespaceAccess { member, .. } =
                            &callee.node
                        {
                            assert_eq!(member, "init");
                            // Positional arguments should have no labels
                            assert_eq!(args.len(), 2);
                            assert_eq!(args[0].label, None);
                            assert_eq!(args[1].label, None);
                            return; // Test passed!
                        }
                    }
                }
            }
        }
    }
    panic!("Could not find expected constructor call");
}
