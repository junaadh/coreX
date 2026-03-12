use core_x::frontend::ast::Item;
use core_x::frontend::parser::{
    parse_source_file_from_source_file_with_recovery,
    parse_source_file_with_recovery,
};
use core_x::frontend::source::SourceDb;

#[test]
fn parse_file_with_recovery_collects_no_diagnostics_for_valid_input() {
    let parsed =
        parse_source_file_with_recovery("fn a() {} struct Foo {}").unwrap();

    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.ast.items.len(), 2);
}

#[test]
fn parse_file_with_recovery_skips_broken_top_level_item_and_continues() {
    let parsed = parse_source_file_with_recovery("fn { struct Foo {}").unwrap();

    assert!(!parsed.diagnostics.is_empty());
    assert!(
        parsed
            .ast
            .items
            .iter()
            .any(|item| matches!(item.node, Item::Struct(_)))
    );
}

#[test]
fn parse_file_with_recovery_collects_multiple_top_level_diagnostics() {
    let parsed =
        parse_source_file_with_recovery("fn { enum { struct Foo {}").unwrap();

    assert!(parsed.diagnostics.len() >= 2);
    assert!(
        parsed
            .ast
            .items
            .iter()
            .any(|item| matches!(item.node, Item::Struct(_)))
    );
}

#[test]
fn parse_file_with_recovery_recovers_after_broken_function_body_statement() {
    let parsed =
        parse_source_file_with_recovery("fn f() { let = ; return; }").unwrap();

    assert!(
        parsed
            .ast
            .items
            .iter()
            .any(|item| matches!(item.node, Item::Function(_)))
    );
    assert!(!parsed.diagnostics.is_empty());
}

#[test]
fn parse_file_with_recovery_recovers_inside_block_and_keeps_later_statements() {
    let parsed =
        parse_source_file_with_recovery("fn f() { let x = ; y; return; }")
            .unwrap();

    let function = parsed
        .ast
        .items
        .iter()
        .find_map(|item| match &item.node {
            Item::Function(function) => Some(function),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected function item"));

    assert!(!parsed.diagnostics.is_empty());
    assert!(
        !function.node.body.statements.is_empty()
            || function.node.body.tail_expr.is_some()
    );
}

#[test]
fn parse_source_file_with_recovery_from_source_file_wrapper_works() {
    let mut db = SourceDb::new();
    let file_id = db.add_file("wrapper.cx", "fn f() { let = ; return; }");
    let file = db.file(file_id).expect("file should exist");

    let report = parse_source_file_from_source_file_with_recovery(file);
    assert!(report.is_ok());
}

#[test]
fn parse_file_with_recovery_handles_unterminated_block_without_panicking() {
    let parsed = parse_source_file_with_recovery("fn f() { let x = y;");

    assert!(parsed.is_ok());
    let parsed = parsed.unwrap();
    assert!(!parsed.diagnostics.is_empty());
}

#[test]
fn parse_file_with_recovery_terminates_on_garbage_input() {
    let parsed = parse_source_file_with_recovery("} } } ??? fn").unwrap();

    assert!(!parsed.diagnostics.is_empty());
}
