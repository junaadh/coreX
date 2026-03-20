//! Tests for HIR parameter external label lowering
//!
//! This test verifies that the HIR correctly preserves external label information
//! from AST through the lowering process.

use core_x::frontend::hir::{HirFile, HirFunctionParam, HirParamLabel, HirItemKind};
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::source::SourceDb;
use core_x::frontend::{DesugaredFile, lower_to_hir};

fn parse_and_lower(db: &mut SourceDb, source: &str) -> (HirFile, DesugaredFile) {
    let file_id = db.add_file("test.cx", source);
    let file = db.file(file_id).expect("file should exist");
    let parsed = parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(
        parsed.diagnostics.is_empty(),
        "strict parse should not emit diagnostics"
    );

    // Create a desugared file (simulates expansion + desugaring)
    let desugared = DesugaredFile {
        file_id: parsed.file_id,
        ast: parsed.ast,
        diagnostics: parsed.diagnostics,
        provenance_map: core_x::frontend::expansion::ProvenanceMap::new(parsed.file_id),
    };

    let (hir_file, _) = lower_to_hir(&desugared);
    (hir_file, desugared)
}

fn get_first_function_hir_params<'a>(
    hir_file: &'a HirFile,
    hir_module: &'a core_x::frontend::hir::HirModule,
) -> &'a [HirFunctionParam] {
    let first_item_id = &hir_file.root_items[0];
    let hir_item = hir_module.items.get(first_item_id).expect("item should exist");
    let HirItemKind::Function(hir_func) = &hir_item.kind else {
        panic!("expected function item");
    };
    &hir_func.signature.params
}

#[test]
fn test_hir_preserves_none_label() {
    let source = r#"
        fn foo(_ x: I32) {}
    "#;

    let mut db = SourceDb::new();
    let (hir_file, desugared) = parse_and_lower(&mut db, source);
    let (_, hir_module) = lower_to_hir(&desugared);

    let params = get_first_function_hir_params(&hir_file, &hir_module);
    assert_eq!(params.len(), 1);

    let param = &params[0];
    assert!(matches!(param.external_label, HirParamLabel::None));
    assert_eq!(param.name, "x");
}

#[test]
fn test_hir_preserves_from_name_label() {
    let source = r#"
        fn foo(x: I32) {}
    "#;

    let mut db = SourceDb::new();
    let (hir_file, desugared) = parse_and_lower(&mut db, source);
    let (_, hir_module) = lower_to_hir(&desugared);

    let params = get_first_function_hir_params(&hir_file, &hir_module);
    assert_eq!(params.len(), 1);

    let param = &params[0];
    assert!(matches!(param.external_label, HirParamLabel::FromName));
    assert_eq!(param.name, "x");
}

#[test]
fn test_hir_preserves_explicit_label() {
    let source = r#"
        fn foo(label x: I32) {}
    "#;

    let mut db = SourceDb::new();
    let (hir_file, desugared) = parse_and_lower(&mut db, source);
    let (_, hir_module) = lower_to_hir(&desugared);

    let params = get_first_function_hir_params(&hir_file, &hir_module);
    assert_eq!(params.len(), 1);

    let param = &params[0];
    match &param.external_label {
        HirParamLabel::Explicit(label) => assert_eq!(label, "label"),
        _ => panic!("expected Explicit label"),
    }
    assert_eq!(param.name, "x");
}

#[test]
fn test_hir_preserves_mixed_labels() {
    let source = r#"
        fn foo(_ x: I32, y: String, label z: Bool) {}
    "#;

    let mut db = SourceDb::new();
    let (hir_file, desugared) = parse_and_lower(&mut db, source);
    let (_, hir_module) = lower_to_hir(&desugared);

    let params = get_first_function_hir_params(&hir_file, &hir_module);
    assert_eq!(params.len(), 3);

    // First param: `_ x: I32` -> None
    assert!(matches!(params[0].external_label, HirParamLabel::None));
    assert_eq!(params[0].name, "x");

    // Second param: `y: String` -> FromName
    assert!(matches!(params[1].external_label, HirParamLabel::FromName));
    assert_eq!(params[1].name, "y");

    // Third param: `label z: Bool` -> Explicit("label")
    match &params[2].external_label {
        HirParamLabel::Explicit(label) => assert_eq!(label, "label"),
        _ => panic!("expected Explicit label for third param"),
    }
    assert_eq!(params[2].name, "z");
}

#[test]
fn test_hir_init_lowering_preserves_labels() {
    use core_x::frontend::hir::HirItemKind;

    let source = r#"
        struct Point {
            init(_ x: I32, y: String, label z: Bool) {}
        }
    "#;

    let mut db = SourceDb::new();
    let (hir_file, desugared) = parse_and_lower(&mut db, source);
    let (_, hir_module) = lower_to_hir(&desugared);

    // Get the first HIR item (struct)
    let struct_item_id = &hir_file.root_items[0];
    let struct_item = hir_module.items.get(struct_item_id).expect("item should exist");
    let HirItemKind::Struct(hir_struct) = &struct_item.kind else {
        panic!("expected struct item");
    };

    // Get the first function in the struct (the init lowered to a function)
    let hir_func = &hir_struct.functions[0];

    let params = &hir_func.signature.params;
    assert_eq!(params.len(), 3);

    // Verify all labels are preserved through init lowering
    assert!(matches!(params[0].external_label, HirParamLabel::None));
    assert!(matches!(params[1].external_label, HirParamLabel::FromName));
    match &params[2].external_label {
        HirParamLabel::Explicit(label) => assert_eq!(label, "label"),
        _ => panic!("expected Explicit label"),
    }
}

#[test]
fn test_hir_protocol_function_preserves_labels() {
    use core_x::frontend::hir::HirItemKind;

    let source = r#"
        protocol Factory {
            fn make(_ x: I32, y: String, label z: Bool);
        }
    "#;

    let mut db = SourceDb::new();
    let (hir_file, desugared) = parse_and_lower(&mut db, source);
    let (_, hir_module) = lower_to_hir(&desugared);

    // Get the first HIR item (protocol)
    let protocol_item_id = &hir_file.root_items[0];
    let protocol_item = hir_module.items.get(protocol_item_id).expect("item should exist");
    let HirItemKind::Protocol(hir_protocol) = &protocol_item.kind else {
        panic!("expected protocol item");
    };

    // Get the first function in the protocol
    let hir_func = &hir_protocol.functions[0];

    let params = &hir_func.signature.params;
    assert_eq!(params.len(), 3);

    // Verify all labels are preserved in protocol functions
    assert!(matches!(params[0].external_label, HirParamLabel::None));
    assert!(matches!(params[1].external_label, HirParamLabel::FromName));
    match &params[2].external_label {
        HirParamLabel::Explicit(label) => assert_eq!(label, "label"),
        _ => panic!("expected Explicit label"),
    }
}

#[test]
fn test_hir_extern_function_preserves_labels() {
    use core_x::frontend::hir::HirItemKind;

    let source = r#"
        extern libc {
            fn foo(_ x: I32, y: I32, label z: I32);
        }
    "#;

    let mut db = SourceDb::new();
    let (hir_file, desugared) = parse_and_lower(&mut db, source);
    let (_, hir_module) = lower_to_hir(&desugared);

    // Get the first HIR item (extern)
    let extern_item_id = &hir_file.root_items[0];
    let extern_item = hir_module.items.get(extern_item_id).expect("item should exist");
    let HirItemKind::Extern(hir_extern) = &extern_item.kind else {
        panic!("expected extern item");
    };

    // Get the first function in the extern block
    let hir_func = &hir_extern.functions[0];

    let params = &hir_func.signature.params;
    assert_eq!(params.len(), 3);

    // Verify all labels are preserved in extern functions
    assert!(matches!(params[0].external_label, HirParamLabel::None));
    assert!(matches!(params[1].external_label, HirParamLabel::FromName));
    match &params[2].external_label {
        HirParamLabel::Explicit(label) => assert_eq!(label, "label"),
        _ => panic!("expected Explicit label"),
    }
}
