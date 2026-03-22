//! Tests for call signature model supporting overload resolution
//!
//! This test verifies that the CallSignature model correctly distinguishes
//! functions by their external parameter labels, enabling overload resolution.

use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::CallSignature;
use core_x::frontend::source::SourceDb;
use core_x::frontend::{
    build_hir_local_binding_table, desugar_files, expand_parsed_files,
    lower_to_hir, ExpansionOptions,
};

fn parse_and_lower(
    source: &str,
) -> (
    core_x::frontend::hir::HirFile,
    core_x::frontend::hir::HirModule,
) {
    let mut db = SourceDb::new();
    let file_id = db.add_file("test.cx", source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(
        parsed.diagnostics.is_empty(),
        "parse should not emit diagnostics"
    );

    let expanded =
        expand_parsed_files(&db, &[parsed], ExpansionOptions::default());
    let desugared = desugar_files(&expanded);
    lower_to_hir(&desugared[0])
}

#[test]
fn test_foo_x_int_and_foo_y_int_have_same_signature() {
    let source = r#"
        fn foo(x: I32) {}
        fn foo(y: I32) {}
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let _bindings = build_hir_local_binding_table(
        &[hir_file.clone()],
        &std::collections::BTreeMap::from([(
            hir_file.file_id,
            hir_module.clone(),
        )]),
    )
    .expect("scope resolution should succeed");

    // Both functions have FromName parameter, so they have the SAME call signature
    // This means they would be considered duplicates for overload resolution
    // fn foo(x: I32) -> signature: (x: I32)
    // fn foo(y: I32) -> signature: (y: I32)
    // But for CallSignature, internal names don't matter, only external labels
    // Both have FromName, so signatures are equal

    // Get the first function
    let func1 = &hir_file.root_items[0];
    let func1_item = hir_module.items.get(func1).expect("item should exist");
    let sig1 = if let core_x::frontend::hir::HirItemKind::Function(f) =
        &func1_item.kind
    {
        CallSignature::from_hir_function(f)
    } else {
        panic!("expected function")
    };

    // Get the second function
    let func2 = &hir_file.root_items[1];
    let func2_item = hir_module.items.get(func2).expect("item should exist");
    let sig2 = if let core_x::frontend::hir::HirItemKind::Function(f) =
        &func2_item.kind
    {
        CallSignature::from_hir_function(f)
    } else {
        panic!("expected function")
    };

    // Both have FromName, so signatures are equal
    assert_eq!(
        sig1, sig2,
        "FromName(x) and FromName(y) have same signature"
    );
    assert_eq!(sig1.param_count(), 1);
    assert_eq!(sig2.param_count(), 1);
}

#[test]
fn test_foo_underscore_x_int_and_foo_x_int_have_different_signatures() {
    let source = r#"
        fn foo(_ x: I32) {}
        fn foo(x: I32) {}
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let _bindings = build_hir_local_binding_table(
        &[hir_file.clone()],
        &std::collections::BTreeMap::from([(
            hir_file.file_id,
            hir_module.clone(),
        )]),
    )
    .expect("scope resolution should succeed");

    // Get the first function: fn foo(_ x: I32)
    let func1 = &hir_file.root_items[0];
    let func1_item = hir_module.items.get(func1).expect("item should exist");
    let sig1 = if let core_x::frontend::hir::HirItemKind::Function(f) =
        &func1_item.kind
    {
        CallSignature::from_hir_function(f)
    } else {
        panic!("expected function")
    };

    // Get the second function: fn foo(x: I32)
    let func2 = &hir_file.root_items[1];
    let func2_item = hir_module.items.get(func2).expect("item should exist");
    let sig2 = if let core_x::frontend::hir::HirItemKind::Function(f) =
        &func2_item.kind
    {
        CallSignature::from_hir_function(f)
    } else {
        panic!("expected function")
    };

    // None vs FromName - different signatures
    assert_ne!(
        sig1, sig2,
        "None(x) and FromName(x) have different signatures"
    );
    assert_eq!(sig1.param_count(), 1);
    assert_eq!(sig2.param_count(), 1);

    // First has None (no label)
    assert_eq!(
        sig1.params[0].label,
        core_x::frontend::resolver::CallParamLabel::None
    );
    assert!(!sig1.accepts_label_at(0));

    // Second has FromName (label from argument)
    assert_eq!(
        sig2.params[0].label,
        core_x::frontend::resolver::CallParamLabel::FromName
    );
    assert!(!sig2.accepts_label_at(0));
}

#[test]
fn test_foo_label_x_int_has_different_signature() {
    let source = r#"
        fn foo(label x: I32) {}
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let _bindings = build_hir_local_binding_table(
        &[hir_file.clone()],
        &std::collections::BTreeMap::from([(
            hir_file.file_id,
            hir_module.clone(),
        )]),
    )
    .expect("scope resolution should succeed");

    // Get the function
    let func = &hir_file.root_items[0];
    let func_item = hir_module.items.get(func).expect("item should exist");
    let sig = if let core_x::frontend::hir::HirItemKind::Function(f) =
        &func_item.kind
    {
        CallSignature::from_hir_function(f)
    } else {
        panic!("expected function")
    };

    assert_eq!(sig.param_count(), 1);
    assert!(
        sig.accepts_label_at(0),
        "explicit label requires label at call site"
    );
    assert_eq!(sig.external_labels(), vec![Some("label".to_string())]);
}

#[test]
fn test_multiple_params_with_different_labels() {
    let source = r#"
        fn foo(_ x: I32, y: I32, label z: I32) {}
        fn foo(x: I32, _ y: I32, z: I32) {}
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let _bindings = build_hir_local_binding_table(
        &[hir_file.clone()],
        &std::collections::BTreeMap::from([(
            hir_file.file_id,
            hir_module.clone(),
        )]),
    )
    .expect("scope resolution should succeed");

    // Get both functions
    let func1 = &hir_file.root_items[0];
    let func1_item = hir_module.items.get(func1).expect("item should exist");
    let sig1 = if let core_x::frontend::hir::HirItemKind::Function(f) =
        &func1_item.kind
    {
        CallSignature::from_hir_function(f)
    } else {
        panic!("expected function")
    };

    let func2 = &hir_file.root_items[1];
    let func2_item = hir_module.items.get(func2).expect("item should exist");
    let sig2 = if let core_x::frontend::hir::HirItemKind::Function(f) =
        &func2_item.kind
    {
        CallSignature::from_hir_function(f)
    } else {
        panic!("expected function")
    };

    // Different label patterns
    assert_ne!(sig1, sig2);

    // First: (_ x: I32, y: I32, label z: I32)
    // Labels: None, FromName, Explicit("label")
    assert_eq!(sig1.param_count(), 3);
    assert_eq!(
        sig1.external_labels(),
        vec![None, None, Some("label".to_string())]
    );
    assert!(!sig1.accepts_label_at(0)); // None
    assert!(!sig1.accepts_label_at(1)); // FromName
    assert!(sig1.accepts_label_at(2)); // Explicit

    // Second: (x: I32, _ y: I32, z: I32)
    // Labels: FromName, None, FromName
    assert_eq!(sig2.param_count(), 3);
    assert_eq!(sig2.external_labels(), vec![None, None, None]);
    assert!(!sig2.accepts_label_at(0)); // FromName
    assert!(!sig2.accepts_label_at(1)); // None
    assert!(!sig2.accepts_label_at(2)); // FromName
}

#[test]
fn test_init_overload_by_label_signature() {
    let source = r#"
        struct Foo {
          init(label x: I32) {}
          init(x: I32) {}
        }
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    // Get the top-level struct item
    let struct_item = &hir_file.root_items[0];
    let struct_item_ref = hir_module
        .items
        .get(struct_item)
        .expect("struct item should exist");
    let hir_struct = if let core_x::frontend::hir::HirItemKind::Struct(s) =
        &struct_item_ref.kind
    {
        s
    } else {
        panic!("expected struct item");
    };
    let inits = &hir_struct.functions;
    assert!(inits.len() >= 2, "expected at least two init overloads");

    let sig1 =
        core_x::frontend::resolver::CallSignature::from_hir_function(&inits[0]);
    let sig2 =
        core_x::frontend::resolver::CallSignature::from_hir_function(&inits[1]);
    // Overloads should differ by their external labels
    assert_ne!(
        sig1, sig2,
        "Init overloads should have different signatures due to labels"
    );
    assert_eq!(sig1.param_count(), 1);
    assert_eq!(sig2.param_count(), 1);
    assert_ne!(sig1.external_labels(), sig2.external_labels());
}

#[test]
fn test_init_functions_have_signature_with_origin() {
    let source = r#"
        struct Point {
            init(_ x: I32, y: I32) {}
        }
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let _bindings = build_hir_local_binding_table(
        &[hir_file.clone()],
        &std::collections::BTreeMap::from([(
            hir_file.file_id,
            hir_module.clone(),
        )]),
    )
    .expect("scope resolution should succeed");

    // Get the struct
    let struct_item = hir_module
        .items
        .get(&hir_file.root_items[0])
        .expect("item should exist");
    let struct_decl = if let core_x::frontend::hir::HirItemKind::Struct(s) =
        &struct_item.kind
    {
        s
    } else {
        panic!("expected struct")
    };

    // Get the init function
    let init_func = &struct_decl.functions[0];
    let sig = CallSignature::from_hir_function(init_func);

    // Should have init origin
    assert!(sig.init_origin.is_some(), "init should have origin");
    assert_eq!(sig.param_count(), 2);
    assert_eq!(sig.external_labels(), vec![None, None]);
}

#[test]
fn test_signature_display_formats_correctly() {
    let source = r#"
        fn foo(_ x: I32, y: I32, label z: I32) {}
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let _bindings = build_hir_local_binding_table(
        &[hir_file.clone()],
        &std::collections::BTreeMap::from([(
            hir_file.file_id,
            hir_module.clone(),
        )]),
    )
    .expect("scope resolution should succeed");

    // Get the function
    let func = &hir_file.root_items[0];
    let func_item = hir_module.items.get(func).expect("item should exist");
    let sig = if let core_x::frontend::hir::HirItemKind::Function(f) =
        &func_item.kind
    {
        CallSignature::from_hir_function(f)
    } else {
        panic!("expected function")
    };

    assert_eq!(format!("{}", sig), "(_ x, y, label z)");
}
