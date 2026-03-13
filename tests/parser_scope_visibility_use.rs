use core_x::frontend::ast::{Item, UseTree, Visibility};
use core_x::frontend::parser::parse_source_file;

fn parse_single_item(source: &str) -> Item {
    let parsed = parse_source_file(source).expect("parse should succeed");
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(
        parsed.ast.items.len(),
        1,
        "expected exactly one top-level item"
    );
    parsed.ast.items.into_iter().next().expect("one item").node
}

#[test]
fn parse_scope_declaration_plain() {
    let item = parse_single_item("scope foo;");
    match item {
        Item::Scope(scope_decl) => {
            assert_eq!(scope_decl.node.name, "foo");
            assert_eq!(scope_decl.node.visibility, None);
        }
        other => panic!("expected scope item, got {other:?}"),
    }
}

#[test]
fn parse_scope_declaration_pub() {
    let item = parse_single_item("pub scope foo;");
    match item {
        Item::Scope(scope_decl) => {
            assert_eq!(scope_decl.node.name, "foo");
            assert_eq!(scope_decl.node.visibility, Some(Visibility::Public));
        }
        other => panic!("expected scope item, got {other:?}"),
    }
}

#[test]
fn parse_scope_declaration_pub_project() {
    let item = parse_single_item("pub(project) scope foo;");
    match item {
        Item::Scope(scope_decl) => {
            assert_eq!(scope_decl.node.name, "foo");
            assert_eq!(
                scope_decl.node.visibility,
                Some(Visibility::PublicProject)
            );
        }
        other => panic!("expected scope item, got {other:?}"),
    }
}

#[test]
fn parse_scope_declaration_pub_super() {
    let item = parse_single_item("pub(super) scope foo;");
    match item {
        Item::Scope(scope_decl) => {
            assert_eq!(scope_decl.node.name, "foo");
            assert_eq!(
                scope_decl.node.visibility,
                Some(Visibility::PublicSuper)
            );
        }
        other => panic!("expected scope item, got {other:?}"),
    }
}

#[test]
fn parse_function_with_pub_super_visibility() {
    let item = parse_single_item("pub(super) fn foo() {}");
    match item {
        Item::Function(function) => {
            assert_eq!(function.node.name, "foo");
            assert_eq!(function.node.visibility, Some(Visibility::PublicSuper));
        }
        other => panic!("expected function item, got {other:?}"),
    }
}

#[test]
fn parse_struct_with_pub_project_visibility() {
    let item = parse_single_item("pub(project) struct Bar {}");
    match item {
        Item::Struct(struct_decl) => {
            assert_eq!(struct_decl.node.name, "Bar");
            assert_eq!(
                struct_decl.node.visibility,
                Some(Visibility::PublicProject)
            );
        }
        other => panic!("expected struct item, got {other:?}"),
    }
}

#[test]
fn parse_use_path() {
    let item = parse_single_item("use root::a::b;");
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Path { path } => {
                assert_eq!(path.segments, vec!["root", "a", "b"]);
            }
            other => panic!("expected use path, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_use_glob() {
    let item = parse_single_item("use root::a::*;");
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Glob { path } => {
                assert_eq!(path.segments, vec!["root", "a"]);
            }
            other => panic!("expected use glob, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_use_group_simple() {
    let item = parse_single_item("use root::a::{b,c};");
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Group {
                path: Some(path),
                items,
            } => {
                assert_eq!(path.segments, vec!["root", "a"]);
                assert_eq!(items.len(), 2);
                match &items[0].node {
                    UseTree::Path { path } => {
                        assert_eq!(path.segments, vec!["b"]);
                    }
                    other => {
                        panic!("expected first group item path, got {other:?}")
                    }
                }
                match &items[1].node {
                    UseTree::Path { path } => {
                        assert_eq!(path.segments, vec!["c"]);
                    }
                    other => {
                        panic!("expected second group item path, got {other:?}")
                    }
                }
            }
            other => panic!("expected use group, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_use_group_with_self() {
    let item = parse_single_item("use root::a::{self,b};");
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Group {
                path: Some(path),
                items,
            } => {
                assert_eq!(path.segments, vec!["root", "a"]);
                assert_eq!(items.len(), 2);
                assert!(matches!(items[0].node, UseTree::SelfImport));
                match &items[1].node {
                    UseTree::Path { path } => {
                        assert_eq!(path.segments, vec!["b"]);
                    }
                    other => {
                        panic!("expected second group item path, got {other:?}")
                    }
                }
            }
            other => panic!("expected use group, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_use_group_with_self_alias() {
    let item = parse_single_item("use root::a::{self as a_root,b};");
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Group {
                path: Some(path),
                items,
            } => {
                assert_eq!(path.segments, vec!["root", "a"]);
                assert_eq!(items.len(), 2);
                match &items[0].node {
                    UseTree::SelfAlias { alias } => {
                        assert_eq!(alias, "a_root");
                    }
                    other => {
                        panic!(
                            "expected first group item self alias, got {other:?}"
                        )
                    }
                }
                match &items[1].node {
                    UseTree::Path { path } => {
                        assert_eq!(path.segments, vec!["b"]);
                    }
                    other => {
                        panic!("expected second group item path, got {other:?}")
                    }
                }
            }
            other => panic!("expected use group, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_empty_use_group_is_rejected() {
    let error = parse_source_file("use root::foo::{};")
        .expect_err("empty grouped import should be rejected");
    match error {
        core_x::frontend::parser::ParseError::UnexpectedToken {
            expected,
            ..
        } => {
            assert_eq!(expected, "use group item");
        }
        other => panic!("expected unexpected token error, got {other:?}"),
    }
}

#[test]
fn parse_top_level_self_use_is_rejected() {
    let error = parse_source_file("use self::Thing;")
        .expect_err("top-level self import should be rejected");
    match error {
        core_x::frontend::parser::ParseError::UnexpectedToken { .. } => {}
        other => panic!("expected unexpected token error, got {other:?}"),
    }
}

#[test]
fn parse_self_as_general_path_segment_is_rejected() {
    let error = parse_source_file("use root::net::self;")
        .expect_err("self path segment should be rejected");
    match error {
        core_x::frontend::parser::ParseError::UnexpectedToken { .. } => {}
        other => panic!("expected unexpected token error, got {other:?}"),
    }
}

#[test]
fn parse_use_alias() {
    let item = parse_single_item("use root::a::b as c;");
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Alias { path, alias } => {
                assert_eq!(path.segments, vec!["root", "a", "b"]);
                assert_eq!(alias, "c");
            }
            other => panic!("expected use alias, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_pub_use_reexport() {
    let item = parse_single_item("pub use root::a::b;");
    match item {
        Item::Use(use_item) => {
            assert_eq!(use_item.node.visibility, Some(Visibility::Public));
            match &use_item.node.tree.node {
                UseTree::Path { path } => {
                    assert_eq!(path.segments, vec!["root", "a", "b"]);
                }
                other => panic!("expected use path, got {other:?}"),
            }
        }
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_nested_use_group() {
    let item = parse_single_item("use root::a::{b::*,c::{self,d}};");
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Group {
                path: Some(path),
                items,
            } => {
                assert_eq!(path.segments, vec!["root", "a"]);
                assert_eq!(items.len(), 2);

                match &items[0].node {
                    UseTree::Glob { path } => {
                        assert_eq!(path.segments, vec!["b"]);
                    }
                    other => {
                        panic!("expected first nested item glob, got {other:?}")
                    }
                }

                match &items[1].node {
                    UseTree::Group {
                        path: Some(path),
                        items,
                    } => {
                        assert_eq!(path.segments, vec!["c"]);
                        assert_eq!(items.len(), 2);
                        assert!(matches!(items[0].node, UseTree::SelfImport));
                        match &items[1].node {
                            UseTree::Path { path } => {
                                assert_eq!(path.segments, vec!["d"]);
                            }
                            other => panic!(
                                "expected nested group second item path, got {other:?}"
                            ),
                        }
                    }
                    other => panic!(
                        "expected second nested item group, got {other:?}"
                    ),
                }
            }
            other => panic!("expected top-level use group, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_visibility_prefixed_recursive_use_group() {
    let item = parse_single_item(
        "pub(project) use root::internal::{helpers::*, model::{self, Id}};",
    );
    match item {
        Item::Use(use_item) => {
            assert_eq!(
                use_item.node.visibility,
                Some(Visibility::PublicProject)
            );
            match &use_item.node.tree.node {
                UseTree::Group {
                    path: Some(path),
                    items,
                } => {
                    assert_eq!(path.segments, vec!["root", "internal"]);
                    assert_eq!(items.len(), 2);

                    match &items[0].node {
                        UseTree::Glob { path } => {
                            assert_eq!(path.segments, vec!["helpers"]);
                        }
                        other => {
                            panic!("expected first item glob, got {other:?}")
                        }
                    }

                    match &items[1].node {
                        UseTree::Group {
                            path: Some(path),
                            items,
                        } => {
                            assert_eq!(path.segments, vec!["model"]);
                            assert_eq!(items.len(), 2);
                            assert!(matches!(
                                items[0].node,
                                UseTree::SelfImport
                            ));
                            match &items[1].node {
                                UseTree::Path { path } => {
                                    assert_eq!(path.segments, vec!["Id"]);
                                }
                                other => {
                                    panic!(
                                        "expected nested path item, got {other:?}"
                                    )
                                }
                            }
                        }
                        other => panic!("expected nested group, got {other:?}"),
                    }
                }
                other => panic!("expected use group, got {other:?}"),
            }
        }
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_recursive_use_group_with_alias_and_glob() {
    let item = parse_single_item("use root::scope::{a::b, c::d as E, f::*};");
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Group {
                path: Some(path),
                items,
            } => {
                assert_eq!(path.segments, vec!["root", "scope"]);
                assert_eq!(items.len(), 3);

                match &items[0].node {
                    UseTree::Path { path } => {
                        assert_eq!(path.segments, vec!["a", "b"]);
                    }
                    other => panic!("expected path item, got {other:?}"),
                }
                match &items[1].node {
                    UseTree::Alias { path, alias } => {
                        assert_eq!(path.segments, vec!["c", "d"]);
                        assert_eq!(alias, "E");
                    }
                    other => panic!("expected alias item, got {other:?}"),
                }
                match &items[2].node {
                    UseTree::Glob { path } => {
                        assert_eq!(path.segments, vec!["f"]);
                    }
                    other => panic!("expected glob item, got {other:?}"),
                }
            }
            other => panic!("expected grouped use, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_deep_recursive_use_group_mix() {
    let item = parse_single_item(
        "use root::scope::{a::{b::{c::*, d as D}, self}, e::f as G};",
    );
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Group {
                path: Some(path),
                items,
            } => {
                assert_eq!(path.segments, vec!["root", "scope"]);
                assert_eq!(items.len(), 2);
                assert!(matches!(items[1].node, UseTree::Alias { .. }));
            }
            other => panic!("expected top-level group, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_use_path_allows_scope_keyword_segment() {
    let item = parse_single_item("use root::scope::scope as S;");
    match item {
        Item::Use(use_item) => match &use_item.node.tree.node {
            UseTree::Alias { path, alias } => {
                assert_eq!(path.segments, vec!["root", "scope", "scope"]);
                assert_eq!(alias, "S");
            }
            other => panic!("expected alias use item, got {other:?}"),
        },
        other => panic!("expected use item, got {other:?}"),
    }
}

#[test]
fn parse_struct_init_rejects_visibility_prefix() {
    let error = parse_source_file("struct S { pub init() {} }")
        .expect_err("visibility on init should be rejected");
    match error {
        core_x::frontend::parser::ParseError::UnexpectedToken {
            expected,
            ..
        } => {
            assert!(
                expected.contains("visibility is not allowed"),
                "unexpected parse expectation: {expected}"
            );
        }
        other => panic!("expected unexpected token error, got {other:?}"),
    }
}
