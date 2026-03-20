//! Tests for external parameter label model through parser -> expansion -> desugar
//!
//! This test verifies that the ParamLabel enum correctly represents:
//! - `None` for `_ x: T` (wildcard prefix, no external label)
//! - `Explicit(String)` for `foo x: T` (explicit external label)
//! - `FromName` for `x: T` (external label derived from parameter name)

use core_x::frontend::ast::{File, Item, ParamDecl, ParamLabel, Spanned};
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::source::SourceDb;

fn parse_source(db: &mut SourceDb, source: &str) -> File {
    let file_id = db.add_file("test.cx", source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(
        parsed.diagnostics.is_empty(),
        "strict parse should not emit diagnostics"
    );
    parsed.ast
}

fn get_first_function_params(file: &File) -> Option<&[Spanned<ParamDecl>]> {
    match &file.items.first()?.node {
        Item::Function(func) => Some(&func.node.params),
        _ => None,
    }
}

#[test]
fn test_parser_underscore_param() {
    let source = r#"
        fn foo(_ x: I32) {}
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let params =
        get_first_function_params(&file).expect("should have function");
    assert_eq!(params.len(), 1);

    let param = &params[0].node;
    assert!(matches!(param.label, ParamLabel::None));
    assert_eq!(param.name, "x");
}

#[test]
fn test_parser_from_name_param() {
    let source = r#"
        fn foo(x: I32) {}
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let params =
        get_first_function_params(&file).expect("should have function");
    assert_eq!(params.len(), 1);

    let param = &params[0].node;
    assert!(matches!(param.label, ParamLabel::FromName));
    assert_eq!(param.name, "x");
}

#[test]
fn test_parser_explicit_label_param() {
    let source = r#"
        fn foo(label x: I32) {}
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let params =
        get_first_function_params(&file).expect("should have function");
    assert_eq!(params.len(), 1);

    let param = &params[0].node;
    match &param.label {
        ParamLabel::Explicit(label) => assert_eq!(label, "label"),
        _ => panic!("expected Explicit label"),
    }
    assert_eq!(param.name, "x");
}

#[test]
fn test_parser_multiple_params_mixed_labels() {
    let source = r#"
        fn foo(_ x: I32, y: String, label z: Bool) {}
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let params =
        get_first_function_params(&file).expect("should have function");
    assert_eq!(params.len(), 3);

    // First param: `_ x: I32` -> None
    assert!(matches!(params[0].node.label, ParamLabel::None));
    assert_eq!(params[0].node.name, "x");

    // Second param: `y: String` -> FromName
    assert!(matches!(params[1].node.label, ParamLabel::FromName));
    assert_eq!(params[1].node.name, "y");

    // Third param: `label z: Bool` -> Explicit("label")
    match &params[2].node.label {
        ParamLabel::Explicit(label) => assert_eq!(label, "label"),
        _ => panic!("expected Explicit label for third param"),
    }
    assert_eq!(params[2].node.name, "z");
}

#[test]
fn test_parser_preserves_param_labels_through_struct_init() {
    use core_x::frontend::ast::StructMember;

    let source = r#"
        struct Point {
            init(_ x: I32, y: String, label z: Bool) {}
        }
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let item = &file.items.first().expect("should have item").node;

    match item {
        Item::Struct(struct_decl) => {
            let member = &struct_decl
                .node
                .members
                .first()
                .expect("should have member")
                .node;
            match member {
                StructMember::Init(init) => {
                    assert_eq!(init.node.params.len(), 3);

                    // Verify all labels in init declaration
                    assert!(matches!(
                        init.node.params[0].node.label,
                        ParamLabel::None
                    ));
                    assert!(matches!(
                        init.node.params[1].node.label,
                        ParamLabel::FromName
                    ));
                    match &init.node.params[2].node.label {
                        ParamLabel::Explicit(label) => {
                            assert_eq!(label, "label")
                        }
                        _ => panic!("expected Explicit label"),
                    }
                }
                _ => panic!("expected init member"),
            }
        }
        _ => panic!("expected struct item"),
    }
}

#[test]
fn test_parser_preserves_param_labels_in_protocol_function() {
    use core_x::frontend::ast::ProtocolMember;

    let source = r#"
        protocol Factory {
            fn make(_ x: I32, y: String, label z: Bool);
        }
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let item = &file.items.first().expect("should have item").node;

    match item {
        Item::Protocol(protocol_decl) => {
            let member = &protocol_decl
                .node
                .members
                .first()
                .expect("should have member")
                .node;
            match member {
                ProtocolMember::Function(func) => {
                    assert_eq!(func.node.params.len(), 3);

                    // Verify all labels in protocol function
                    assert!(matches!(
                        func.node.params[0].node.label,
                        ParamLabel::None
                    ));
                    assert!(matches!(
                        func.node.params[1].node.label,
                        ParamLabel::FromName
                    ));
                    match &func.node.params[2].node.label {
                        ParamLabel::Explicit(label) => {
                            assert_eq!(label, "label")
                        }
                        _ => panic!("expected Explicit label"),
                    }
                }
                _ => panic!("expected function member"),
            }
        }
        _ => panic!("expected protocol item"),
    }
}

#[test]
fn test_parser_preserves_param_labels_in_extern_function() {
    use core_x::frontend::ast::ExternMember;

    let source = r#"
        extern libc {
            fn foo(_ x: I32, y: I32, label z: I32);
        }
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let item = &file.items.first().expect("should have item").node;

    match item {
        Item::ExternBlock(extern_block) => {
            let member = &extern_block
                .node
                .members
                .first()
                .expect("should have member")
                .node;
            match member {
                ExternMember::Function(func) => {
                    assert_eq!(func.node.params.len(), 3);

                    // Verify all labels in extern function
                    assert!(matches!(
                        func.node.params[0].node.label,
                        ParamLabel::None
                    ));
                    assert!(matches!(
                        func.node.params[1].node.label,
                        ParamLabel::FromName
                    ));
                    match &func.node.params[2].node.label {
                        ParamLabel::Explicit(label) => {
                            assert_eq!(label, "label")
                        }
                        _ => panic!("expected Explicit label"),
                    }
                }
                _ => panic!("expected function member"),
            }
        }
        _ => panic!("expected extern block"),
    }
}

#[test]
fn test_parser_preserves_param_labels_in_enum_init() {
    use core_x::frontend::ast::EnumMember;

    let source = r#"
        enum Option {
            init(_ x: I32, y: String, label z: Bool) {}
        }
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let item = &file.items.first().expect("should have item").node;

    match item {
        Item::Enum(enum_decl) => {
            let member = &enum_decl
                .node
                .members
                .first()
                .expect("should have member")
                .node;
            match member {
                EnumMember::Init(init) => {
                    assert_eq!(init.node.params.len(), 3);

                    // Verify all labels in enum init declaration
                    assert!(matches!(
                        init.node.params[0].node.label,
                        ParamLabel::None
                    ));
                    assert!(matches!(
                        init.node.params[1].node.label,
                        ParamLabel::FromName
                    ));
                    match &init.node.params[2].node.label {
                        ParamLabel::Explicit(label) => {
                            assert_eq!(label, "label")
                        }
                        _ => panic!("expected Explicit label"),
                    }
                }
                _ => panic!("expected init member"),
            }
        }
        _ => panic!("expected enum item"),
    }
}

#[test]
fn test_parser_preserves_param_labels_in_protocol_init() {
    use core_x::frontend::ast::ProtocolMember;

    let source = r#"
        protocol Factory {
            init(_ x: I32, y: String, label z: Bool);
        }
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let item = &file.items.first().expect("should have item").node;

    match item {
        Item::Protocol(protocol_decl) => {
            let member = &protocol_decl
                .node
                .members
                .first()
                .expect("should have member")
                .node;
            match member {
                ProtocolMember::Initializer(init) => {
                    assert_eq!(init.node.params.len(), 3);

                    // Verify all labels in protocol init declaration
                    assert!(matches!(
                        init.node.params[0].node.label,
                        ParamLabel::None
                    ));
                    assert!(matches!(
                        init.node.params[1].node.label,
                        ParamLabel::FromName
                    ));
                    match &init.node.params[2].node.label {
                        ParamLabel::Explicit(label) => {
                            assert_eq!(label, "label")
                        }
                        _ => panic!("expected Explicit label"),
                    }
                }
                _ => panic!("expected init member"),
            }
        }
        _ => panic!("expected protocol item"),
    }
}

#[test]
fn test_parser_preserves_param_labels_in_impl_init() {
    use core_x::frontend::ast::ImplMember;

    let source = r#"
        impl Point {
            init(_ x: I32, y: String, label z: Bool) {}
        }
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let item = &file.items.first().expect("should have item").node;

    match item {
        Item::Impl(impl_decl) => {
            let member = &impl_decl
                .node
                .members
                .first()
                .expect("should have member")
                .node;
            match member {
                ImplMember::Init(init) => {
                    assert_eq!(init.node.params.len(), 3);

                    // Verify all labels in impl init declaration
                    assert!(matches!(
                        init.node.params[0].node.label,
                        ParamLabel::None
                    ));
                    assert!(matches!(
                        init.node.params[1].node.label,
                        ParamLabel::FromName
                    ));
                    match &init.node.params[2].node.label {
                        ParamLabel::Explicit(label) => {
                            assert_eq!(label, "label")
                        }
                        _ => panic!("expected Explicit label"),
                    }
                }
                _ => panic!("expected init member"),
            }
        }
        _ => panic!("expected impl item"),
    }
}

#[test]
fn test_parser_preserves_param_labels_in_optional_init() {
    use core_x::frontend::ast::{InitKind, StructMember};

    let source = r#"
        struct Point {
            init(_ x: I32, y: String, label z: Bool) -> Option<Self> {}
        }
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let item = &file.items.first().expect("should have item").node;

    match item {
        Item::Struct(struct_decl) => {
            let member = &struct_decl
                .node
                .members
                .first()
                .expect("should have member")
                .node;
            match member {
                StructMember::Init(init) => {
                    assert_eq!(init.node.kind, InitKind::Plain); // Will be inferred during desugaring
                    assert_eq!(init.node.params.len(), 3);

                    // Verify all labels in optional init declaration
                    assert!(matches!(
                        init.node.params[0].node.label,
                        ParamLabel::None
                    ));
                    assert!(matches!(
                        init.node.params[1].node.label,
                        ParamLabel::FromName
                    ));
                    match &init.node.params[2].node.label {
                        ParamLabel::Explicit(label) => {
                            assert_eq!(label, "label")
                        }
                        _ => panic!("expected Explicit label"),
                    }
                }
                _ => panic!("expected init member"),
            }
        }
        _ => panic!("expected struct item"),
    }
}

#[test]
fn test_parser_preserves_param_labels_in_fallible_init() {
    use core_x::frontend::ast::{InitKind, StructMember};

    let source = r#"
        struct Point {
            init(_ x: I32, y: String, label z: Bool) -> Result<Self, Error> {}
        }
    "#;

    let mut db = SourceDb::new();
    let file = parse_source(&mut db, source);
    let item = &file.items.first().expect("should have item").node;

    match item {
        Item::Struct(struct_decl) => {
            let member = &struct_decl
                .node
                .members
                .first()
                .expect("should have member")
                .node;
            match member {
                StructMember::Init(init) => {
                    assert_eq!(init.node.kind, InitKind::Plain); // Will be inferred during desugaring
                    assert_eq!(init.node.params.len(), 3);

                    // Verify all labels in fallible init declaration
                    assert!(matches!(
                        init.node.params[0].node.label,
                        ParamLabel::None
                    ));
                    assert!(matches!(
                        init.node.params[1].node.label,
                        ParamLabel::FromName
                    ));
                    match &init.node.params[2].node.label {
                        ParamLabel::Explicit(label) => {
                            assert_eq!(label, "label")
                        }
                        _ => panic!("expected Explicit label"),
                    }
                }
                _ => panic!("expected init member"),
            }
        }
        _ => panic!("expected struct item"),
    }
}
