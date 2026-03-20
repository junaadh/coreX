//! Tests for method call desugaring
//!
//! Verifies that `obj.method()` calls are properly transformed
//! through desugaring and HIR lowering.

use core_x::frontend::ast::{Expr, File, Item, Spanned, Stmt};
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

fn find_method_call_in_file(file: &File) -> Option<&Spanned<Expr>> {
    for item in &file.items {
        if let Item::Function(func) = &item.node {
            // Check body statements
            for stmt in &func.node.body.statements {
                if let Stmt::Expr { expr, .. } = &stmt.node {
                    if matches!(expr.node, Expr::MethodCall { .. }) {
                        return Some(expr);
                    }
                }
            }
            // Check tail expression
            if let Some(tail) = &func.node.body.tail_expr {
                if matches!(tail.node, Expr::MethodCall { .. }) {
                    return Some(tail);
                }
            }
        }
    }
    None
}

#[test]
fn test_basic_method_call_desugars() {
    let source = r#"
        struct Point {
            x: I32,
            fn get_x(&self) -> I32 {
                self.x
            }
        }

        fn test() {
            let p = Point { x: 42 };
            p.get_x()
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    let method_call = find_method_call_in_file(&desugared[0].ast)
        .expect("should find method call");

    assert!(matches!(&method_call.node, Expr::MethodCall { .. }));
}

#[test]
fn test_method_call_with_arguments() {
    let source = r#"
        struct Counter {
            count: I32,
            fn add(&mut self, amount: I32) -> I32 {
                self.count = self.count + amount;
                self.count
            }
        }

        fn test() {
            let c = Counter { count: 0 };
            c.add(5)
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    let method_call = find_method_call_in_file(&desugared[0].ast)
        .expect("should find method call");

    if let Expr::MethodCall { args, .. } = &method_call.node {
        assert_eq!(args.len(), 1, "should have one argument");
    } else {
        panic!("expected MethodCall");
    }
}

#[test]
fn test_chained_method_calls() {
    let source = r#"
        struct Builder {
            value: I32,
            fn set_value(&mut self, v: I32) -> Self {
                self.value = v;
                self
            }
            fn double(&mut self) -> Self {
                self.value = self.value * 2;
                self
            }
        }

        fn test() {
            let b = Builder { value: 1 };
            b.set_value(5).double()
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    // Find the outer method call
    let outer_call = find_method_call_in_file(&desugared[0].ast)
        .expect("should find method call");

    if let Expr::MethodCall { receiver, .. } = &outer_call.node {
        // Receiver should be another MethodCall (chained)
        assert!(matches!(&receiver.node, Expr::MethodCall { .. }));
    } else {
        panic!("expected MethodCall");
    }
}

#[test]
fn test_regular_function_call_unchanged() {
    let source = r#"
        fn helper(x: I32) -> I32 {
            x + 1
        }

        fn test() {
            helper(42)
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    // Regular function calls should NOT be desugared to MethodCall
    for item in &desugared[0].ast.items {
        if let Item::Function(func) = &item.node {
            if func.node.name == "test" {
                if let Some(tail) = &func.node.body.tail_expr {
                    assert!(matches!(&tail.node, Expr::Call { .. }));
                    assert!(!matches!(&tail.node, Expr::MethodCall { .. }));
                    return; // Test passed
                }
            }
        }
    }
    panic!("Could not find test function or expression");
}

#[test]
fn test_namespace_access_unchanged() {
    let source = r#"
        struct Point {
            x: I32,
            fn origin() -> Self {
                Self { x: 0 }
            }
        }

        fn test() {
            Point::origin()
        }
    "#;

    let mut db = SourceDb::new();
    let parsed = parse_single_file(&mut db, "test.cx", source);
    let desugared = expand_and_desugar(&db, &parsed);

    // Type::method() calls should NOT be desugared to MethodCall
    for item in &desugared[0].ast.items {
        if let Item::Function(func) = &item.node {
            if func.node.name == "test" {
                if let Some(tail) = &func.node.body.tail_expr {
                    if let Expr::Call { callee, .. } = &tail.node {
                        assert!(matches!(&callee.node, Expr::NamespaceAccess { .. }));
                        return; // Test passed
                    }
                }
            }
        }
    }
    panic!("Could not find test function or expression");
}
