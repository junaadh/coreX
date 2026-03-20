//! Tests for HIR scope resolution using internal parameter names only
//!
//! This test verifies that:
//! - Only internal parameter names are bound in local scope
//! - External labels are NOT bound in local scope
//! - Parameters resolve correctly regardless of external label form

use core_x::frontend::hir::{HirFile, HirModule};
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::source::SourceDb;
use core_x::frontend::{
    ExpansionOptions, LocalKind, build_hir_local_binding_table, desugar_files,
    expand_parsed_files, lower_to_hir,
};

fn parse_and_lower(source: &str) -> (HirFile, HirModule) {
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

fn get_first_function_binding(
    hir_file: &HirFile,
    hir_module: &HirModule,
) -> core_x::frontend::HirLocalBindingTable {
    build_hir_local_binding_table(
        &[hir_file.clone()],
        &std::collections::BTreeMap::from([(
            hir_file.file_id,
            hir_module.clone(),
        )]),
    )
    .expect("scope resolution should succeed")
}

#[test]
fn test_internal_name_resolves_with_explicit_label() {
    let source = r#"
        fn foo(bar baz: I32) {
            baz  // Should resolve to parameter
        }
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let bindings = get_first_function_binding(&hir_file, &hir_module);

    // Find the binding for "baz" (internal name)
    let baz_binding = bindings
        .iter_bindings()
        .find(|b| b.name == "baz")
        .expect("should find binding for internal name 'baz'");

    assert_eq!(baz_binding.name, "baz");
    assert!(matches!(baz_binding.kind, LocalKind::Parameter));

    // Verify "bar" (external label) is NOT bound
    let bar_binding = bindings.iter_bindings().find(|b| b.name == "bar");
    assert!(
        bar_binding.is_none(),
        "external label 'bar' should not be bound in scope"
    );
}

#[test]
fn test_from_name_parameter_resolves() {
    let source = r#"
        fn foo(x: I32) {
            x  // Should resolve to parameter (FromName case)
        }
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let bindings = get_first_function_binding(&hir_file, &hir_module);

    // Find the binding for "x"
    let x_binding = bindings
        .iter_bindings()
        .find(|b| b.name == "x")
        .expect("should find binding for 'x'");

    assert_eq!(x_binding.name, "x");
    assert!(matches!(x_binding.kind, LocalKind::Parameter));
}

#[test]
fn test_none_label_parameter_resolves() {
    let source = r#"
        fn foo(_ x: I32) {
            x  // Should resolve to parameter (None case)
        }
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let bindings = get_first_function_binding(&hir_file, &hir_module);

    // Find the binding for "x"
    let x_binding = bindings
        .iter_bindings()
        .find(|b| b.name == "x")
        .expect("should find binding for 'x'");

    assert_eq!(x_binding.name, "x");
    assert!(matches!(x_binding.kind, LocalKind::Parameter));
}

#[test]
fn test_external_label_not_accessible_in_body() {
    let source = r#"
        fn foo(label internalName: I32) {
            internalName  // Should resolve
            // label  // Would NOT resolve if uncommented
        }
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let bindings = get_first_function_binding(&hir_file, &hir_module);

    // Internal name should be bound
    assert!(bindings.iter_bindings().any(|b| b.name == "internalName"));

    // External label should NOT be bound
    assert!(!bindings.iter_bindings().any(|b| b.name == "label"));
}

#[test]
fn test_internal_name_shadowing_works() {
    let source = r#"
        fn foo(x: I32) {
            let x = 42;  // Shadows parameter
            x  // Should resolve to the let binding, not the parameter
        }
    "#;

    let (hir_file, hir_module) = parse_and_lower(source);
    let bindings = get_first_function_binding(&hir_file, &hir_module);

    // Should have two bindings for "x" (parameter and local let)
    let x_bindings: Vec<_> =
        bindings.iter_bindings().filter(|b| b.name == "x").collect();

    assert_eq!(
        x_bindings.len(),
        2,
        "should have two 'x' bindings (parameter and shadowed local)"
    );

    // One should be a parameter, one should be a local binding
    let has_param = x_bindings
        .iter()
        .any(|b| matches!(b.kind, LocalKind::Parameter));
    let has_local = x_bindings
        .iter()
        .any(|b| matches!(b.kind, LocalKind::LocalBinding));

    assert!(has_param, "should have parameter binding");
    assert!(has_local, "should have local binding");
}
