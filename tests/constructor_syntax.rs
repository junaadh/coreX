//! Tests for constructor syntax `TypeName(...)` -> `TypeName::init(...)`
//!
//! Verifies that constructor calls are properly parsed and desugared.

use core_x::frontend::ast::{Expr, File, Item, Spanned};
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::source::SourceDb;
use core_x::frontend::{DesugaredFile, ExpansionOptions, desugar_files, expand_parsed_files};

fn parse_single_file(
    db: &mut SourceDb,
    path: &str,
    source: &str,
) -> Vec<core_x::frontend::ParsedFile> {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed = parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(parsed.diagnostics.is_empty(), "parse should not emit diagnostics");
    vec![parsed]
}

fn expand_and_desugar(
    db: &SourceDb,
    parsed_files: &[core_x::frontend::ParsedFile],
) -> Vec<DesugaredFile> {
    let expanded = expand_parsed_files(db, parsed_files, ExpansionOptions::default());
    desugar_files(&expanded)
}

fn find_expression_in_test_fn(file: &File) -> Option<&Spanned<Expr>> {
    for item in &file.items {
        if let Item::Function(func) = &item.node {
            if func.node.name == "test" {
                if let Some(tail) = &func.node.body.tail_expr {
                    return Some(tail);
                }
            }
        }
    }
    None
}

#[test]
fn test_basic_constructor_syntax() {
    let source = r#"
        struct Point {
            x: I32,
            y: I32,
            init(_ x: I32, _ y: I32) -> Self {
                Self { x, y }
            }
        }

        fn test() {
            Point(1, 2)
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    let expr = find_expression_in_test_fn(&desugared[0].ast)
        .expect("should find test expression");

    // Should be desugared to NamespaceAccess + Call
    if let Expr::Call { callee, args, .. } = &expr.node {
        assert_eq!(args.len(), 2, "should have two arguments");
        if let Expr::NamespaceAccess { member, .. } = &callee.node {
            assert_eq!(member, "init", "should call init method");
        } else {
            panic!("expected NamespaceAccess to init");
        }
    } else {
        panic!("expected Call expression");
    }
}

#[test]
fn test_constructor_with_expressions() {
    let source = r#"
        struct Point {
            x: I32,
            y: I32,
            init(_ x: I32, _ y: I32) -> Self {
                Self { x, y }
            }
        }

        fn test() {
            let a = 1;
            let b = 2;
            Point(a + 1, b * 2)
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    let expr = find_expression_in_test_fn(&desugared[0].ast)
        .expect("should find test expression");

    assert!(matches!(&expr.node, Expr::Call { callee, .. } if matches!(&callee.node, Expr::NamespaceAccess { member, .. } if member == "init")));
}

#[test]
fn test_nested_constructor_calls() {
    let source = r#"
        struct Point {
            x: I32,
            y: I32,
            init(_ x: I32, _ y: I32) -> Self {
                Self { x, y }
            }
        }

        struct Rect {
            top_left: Point,
            bottom_right: Point,
            init(_ top_left: Point, _ bottom_right: Point) -> Self {
                Self { top_left, bottom_right }
            }
        }

        fn test() {
            Rect(Point(0, 0), Point(10, 10))
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    let expr = find_expression_in_test_fn(&desugared[0].ast)
        .expect("should find test expression");

    // Outer constructor call
    if let Expr::Call { callee, args, .. } = &expr.node {
        assert_eq!(args.len(), 2, "should have two Point arguments");
        if let Expr::NamespaceAccess { member, .. } = &callee.node {
            assert_eq!(member, "init", "should call Rect::init");
        }

        // Each argument should be a constructor call too
        for arg in args {
            if let Expr::Call { callee, .. } = &arg.value.node {
                if let Expr::NamespaceAccess { member, .. } = &callee.node {
                    assert_eq!(member, "init", "should call Point::init");
                }
            }
        }
    }
}

#[test]
fn test_struct_literal_unchanged() {
    let source = r#"
        struct Point {
            x: I32,
            y: I32,
        }

        fn test() {
            Point { x: 1, y: 2 }
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    let expr = find_expression_in_test_fn(&desugared[0].ast)
        .expect("should find test expression");

    // Should remain as StructLiteral, not be converted to constructor call
    assert!(matches!(&expr.node, Expr::StructLiteral { .. }));
}

