//! Desugaring pass for the coreX frontend.
//!
//! This module implements the desugaring phase, which transforms syntactic sugar
//! constructs into more primitive forms. This is a structural transformation pass
//! that operates on the expanded AST.
//!
//! # Goals
//!
//! - Transform high-level syntactic constructs into simpler forms
//! - Preserve provenance information through transformations
//! - Collect diagnostics specific to desugaring
//! - Maintain a clear mapping from source to desugared code
//!
//! # Architecture
//!
//! The desugarer operates in a single pass over the expanded AST, applying
//! local syntax rewrites while preserving source/provenance relationships.

mod grouped;

use crate::frontend::ast::File;
use crate::frontend::diagnostics::DiagnosticsBag;
use crate::frontend::expansion::{ExpandedFile, ProvenanceMap};
use crate::frontend::source::FileId;

/// Desugared file produced by the desugaring pass.
///
/// This extends `ExpandedFile` by applying desugaring transformations to the AST.
/// The structure is similar to `ExpandedFile` to maintain compatibility through
/// the compilation pipeline.
///
/// # Fields
///
/// - `file_id`: The source file identifier
/// - `ast`: The desugared AST (same structure as expanded, but with transformations applied)
/// - `diagnostics`: Diagnostics collected during desugaring
/// - `provenance_map`: Provenance mapping carried forward from expansion (and extended by desugaring)
#[derive(Debug, Clone)]
pub struct DesugaredFile {
    /// The file identifier
    pub file_id: FileId,
    /// The desugared AST
    pub ast: File,
    /// Diagnostics collected during desugaring
    pub diagnostics: DiagnosticsBag,
    /// Provenance mapping for desugared nodes
    pub provenance_map: ProvenanceMap,
}

/// Desugars an expanded file by transforming syntactic sugar constructs.
///
/// # Arguments
///
/// * `expanded` - The expanded file to desugar
///
/// # Returns
///
/// A `DesugaredFile` containing the desugared AST and diagnostics
///
/// # Example
///
/// ```ignore
/// let expanded = expand_file(parsed_file);
/// let desugared = desugar_file(&expanded);
/// ```
pub fn desugar_file(expanded: &ExpandedFile) -> DesugaredFile {
    let desugared_ast = grouped::desugar_file_impl(&expanded.ast);

    DesugaredFile {
        file_id: expanded.file_id,
        ast: desugared_ast,
        // Preserve upstream diagnostics (parse + expansion) until desugaring
        // introduces its own diagnostics on top.
        diagnostics: expanded.diagnostics.clone(),
        provenance_map: expanded.provenance_map.clone(),
    }
}

/// Desugars multiple expanded files.
///
/// This is a convenience function that desugars a slice of expanded files,
/// returning a vector of desugared files in the same order.
///
/// # Arguments
///
/// * `expanded_files` - Slice of expanded files to desugar
///
/// # Returns
///
/// A vector of `DesugaredFile` in the same order as the input
///
/// # Example
///
/// ```ignore
/// let expanded_files = expand_parsed_files(&db, &parsed, options);
/// let desugared_files = desugar_files(&expanded_files);
/// ```
#[must_use]
pub fn desugar_files(expanded_files: &[ExpandedFile]) -> Vec<DesugaredFile> {
    expanded_files.iter().map(desugar_file).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::ast::Span;
    use crate::frontend::expansion::ProvenanceMap;

    #[test]
    fn test_desugar_file_preserves_structure() {
        // Create a minimal expanded file
        let file_id = FileId::new(0);
        let mut diagnostics = DiagnosticsBag::default();
        diagnostics.push(crate::frontend::diagnostics::Diagnostic::error(
            "expansion diagnostic",
        ));
        let expanded = ExpandedFile {
            file_id,
            ast: File { items: vec![] },
            diagnostics,
            provenance_map: ProvenanceMap::new(file_id),
        };

        // Desugar the file
        let desugared = desugar_file(&expanded);

        // Verify structure is preserved
        assert_eq!(desugared.file_id, file_id);
        assert_eq!(desugared.ast.items.len(), 0);
        assert_eq!(desugared.diagnostics.len(), 1);
        assert_eq!(
            desugared.diagnostics.as_slice()[0].message,
            "expansion diagnostic"
        );
        assert_eq!(desugared.provenance_map.file_id(), file_id);
    }

    #[test]
    fn test_desugar_item_clones() {
        use crate::frontend::ast::Spanned;
        use crate::frontend::ast::{Item, UseItem, UsePath, UseTree};

        let item = Spanned::new(
            Item::Use(Spanned::new(
                UseItem {
                    visibility: None,
                    tree: Spanned::new(
                        UseTree::Path {
                            path: UsePath {
                                segments: vec![
                                    "std".to_string(),
                                    "collections".to_string(),
                                ],
                            },
                        },
                        Span::new(0, 20),
                    ),
                },
                Span::new(0, 20),
            )),
            Span::new(0, 20),
        );

        let desugared = grouped::desugar_item(&item);
        assert_eq!(item, desugared);
    }

    #[test]
    fn test_desugar_expr_clones() {
        use crate::frontend::ast::Expr;
        use crate::frontend::ast::Spanned;

        let expr = Spanned::new(
            Expr::IntegerLiteral("42".to_string()),
            Span::new(0, 2),
        );

        let desugared = grouped::desugar_expr(&expr);
        assert_eq!(expr, desugared);
    }

    #[test]
    fn test_desugar_ty_clones() {
        use crate::frontend::ast::Spanned;
        use crate::frontend::ast::Type;

        let ty = Spanned::new(
            Type::Named {
                segments: vec!["String".to_string()],
            },
            Span::new(0, 6),
        );

        let desugared = grouped::desugar_ty(&ty);
        assert_eq!(ty, desugared);
    }

    #[test]
    fn test_desugar_pattern_clones() {
        use crate::frontend::ast::Pattern;
        use crate::frontend::ast::Spanned;

        let pattern =
            Spanned::new(Pattern::Identifier("x".to_string()), Span::new(0, 1));

        let desugared = grouped::desugar_pattern(&pattern);
        assert_eq!(pattern, desugared);
    }

    #[test]
    fn test_desugar_stmt_clones() {
        use crate::frontend::ast::Spanned;
        use crate::frontend::ast::{Expr, Stmt};

        let stmt = Spanned::new(
            Stmt::Expr {
                expr: Box::new(Spanned::new(
                    Expr::IntegerLiteral("42".to_string()),
                    Span::new(0, 2),
                )),
                has_semi: true,
            },
            Span::new(0, 3),
        );

        let desugared = grouped::desugar_stmt(&stmt);
        assert_eq!(stmt, desugared);
    }

    #[test]
    fn test_desugar_expr_removes_grouped_wrappers() {
        use crate::frontend::ast::{Expr, Spanned};

        let expr = Spanned::new(
            Expr::Grouped(Box::new(Spanned::new(
                Expr::Grouped(Box::new(Spanned::new(
                    Expr::IntegerLiteral("7".to_string()),
                    Span::new(2, 3),
                ))),
                Span::new(1, 4),
            ))),
            Span::new(0, 5),
        );

        let desugared = grouped::desugar_expr(&expr);
        assert_eq!(
            desugared,
            Spanned::new(
                Expr::IntegerLiteral("7".to_string()),
                Span::new(2, 3)
            )
        );
    }

    #[test]
    fn test_desugar_ty_removes_grouped_wrappers() {
        use crate::frontend::ast::{Spanned, Type};

        let ty = Spanned::new(
            Type::Grouped(Box::new(Spanned::new(
                Type::Grouped(Box::new(Spanned::new(
                    Type::Named {
                        segments: vec!["String".to_string()],
                    },
                    Span::new(2, 8),
                ))),
                Span::new(1, 9),
            ))),
            Span::new(0, 10),
        );

        let desugared = grouped::desugar_ty(&ty);
        assert_eq!(
            desugared,
            Spanned::new(
                Type::Named {
                    segments: vec!["String".to_string()],
                },
                Span::new(2, 8),
            )
        );
    }

    #[test]
    fn test_desugar_stmt_removes_nested_grouped_wrappers() {
        use crate::frontend::ast::{
            Expr, LetStmt, Pattern, Spanned, Stmt, Type,
        };

        let stmt = Spanned::new(
            Stmt::Let(Spanned::new(
                LetStmt {
                    pattern: Spanned::new(
                        Pattern::Identifier("x".to_string()),
                        Span::new(4, 5),
                    ),
                    ty: Some(Spanned::new(
                        Type::Grouped(Box::new(Spanned::new(
                            Type::Named {
                                segments: vec!["I32".to_string()],
                            },
                            Span::new(7, 10),
                        ))),
                        Span::new(6, 11),
                    )),
                    value: Some(Box::new(Spanned::new(
                        Expr::Grouped(Box::new(Spanned::new(
                            Expr::Identifier("y".to_string()),
                            Span::new(14, 15),
                        ))),
                        Span::new(13, 16),
                    ))),
                },
                Span::new(0, 16),
            )),
            Span::new(0, 16),
        );

        let desugared = grouped::desugar_stmt(&stmt);
        let Stmt::Let(let_stmt) = desugared.node else {
            panic!("expected let statement");
        };
        assert_eq!(
            let_stmt.node.ty,
            Some(Spanned::new(
                Type::Named {
                    segments: vec!["I32".to_string()],
                },
                Span::new(7, 10),
            ))
        );
        assert_eq!(
            let_stmt.node.value,
            Some(Box::new(Spanned::new(
                Expr::Identifier("y".to_string()),
                Span::new(14, 15),
            )))
        );
    }

    #[test]
    fn test_desugar_expr_if_wraps_non_block_else_into_block() {
        use crate::frontend::ast::{Clause, ClauseList, Expr, Spanned};

        let expr = Spanned::new(
            Expr::If {
                clauses: ClauseList {
                    clauses: vec![Spanned::new(
                        Clause::Expr(Box::new(Spanned::new(
                            Expr::BooleanLiteral(true),
                            Span::new(3, 7),
                        ))),
                        Span::new(3, 7),
                    )],
                },
                then_branch: crate::frontend::ast::Block {
                    statements: vec![],
                    tail_expr: Some(Box::new(Spanned::new(
                        Expr::IntegerLiteral("1".to_string()),
                        Span::new(10, 11),
                    ))),
                },
                else_branch: Some(Box::new(Spanned::new(
                    Expr::IntegerLiteral("2".to_string()),
                    Span::new(18, 19),
                ))),
            },
            Span::new(0, 19),
        );

        let desugared = grouped::desugar_expr(&expr);
        let Expr::If {
            else_branch: Some(else_branch),
            ..
        } = desugared.node
        else {
            panic!("expected expression if with explicit else");
        };

        let Expr::Block(else_block) = else_branch.node else {
            panic!("expected else branch to be normalized into block");
        };
        assert_eq!(else_block.statements.len(), 0);
        assert_eq!(
            else_block.tail_expr,
            Some(Box::new(Spanned::new(
                Expr::IntegerLiteral("2".to_string()),
                Span::new(18, 19),
            )))
        );
    }

    #[test]
    fn test_desugar_expr_if_makes_missing_else_explicit() {
        use crate::frontend::ast::{Clause, ClauseList, Expr, Spanned};

        let expr = Spanned::new(
            Expr::If {
                clauses: ClauseList {
                    clauses: vec![Spanned::new(
                        Clause::Expr(Box::new(Spanned::new(
                            Expr::BooleanLiteral(true),
                            Span::new(3, 7),
                        ))),
                        Span::new(3, 7),
                    )],
                },
                then_branch: crate::frontend::ast::Block {
                    statements: vec![],
                    tail_expr: None,
                },
                else_branch: None,
            },
            Span::new(0, 12),
        );

        let desugared = grouped::desugar_expr(&expr);
        let Expr::If {
            else_branch: Some(else_branch),
            ..
        } = desugared.node
        else {
            panic!("expected expression if with explicit else");
        };
        let Expr::Block(else_block) = else_branch.node else {
            panic!("expected else branch block");
        };
        assert!(else_block.statements.is_empty());
        assert!(else_block.tail_expr.is_none());
    }

    #[test]
    fn test_desugar_stmt_if_normalizes_else_if_to_else_block() {
        use crate::frontend::ast::{
            Clause, ClauseList, IfStmt, IfStmtElse, Spanned, Stmt,
        };

        let nested_if = Spanned::new(
            IfStmt {
                clauses: ClauseList {
                    clauses: vec![Spanned::new(
                        Clause::Expr(Box::new(Spanned::new(
                            crate::frontend::ast::Expr::BooleanLiteral(false),
                            Span::new(17, 22),
                        ))),
                        Span::new(17, 22),
                    )],
                },
                then_branch: crate::frontend::ast::Block {
                    statements: vec![],
                    tail_expr: None,
                },
                else_branch: None,
            },
            Span::new(14, 24),
        );

        let stmt = Spanned::new(
            Stmt::If(Spanned::new(
                IfStmt {
                    clauses: ClauseList {
                        clauses: vec![Spanned::new(
                            Clause::Expr(Box::new(Spanned::new(
                                crate::frontend::ast::Expr::BooleanLiteral(
                                    true,
                                ),
                                Span::new(3, 7),
                            ))),
                            Span::new(3, 7),
                        )],
                    },
                    then_branch: crate::frontend::ast::Block {
                        statements: vec![],
                        tail_expr: None,
                    },
                    else_branch: Some(IfStmtElse::If(Box::new(nested_if))),
                },
                Span::new(0, 24),
            )),
            Span::new(0, 24),
        );

        let desugared = grouped::desugar_stmt(&stmt);
        let Stmt::If(if_stmt) = desugared.node else {
            panic!("expected if statement");
        };
        let Some(IfStmtElse::Block(else_block)) = if_stmt.node.else_branch
        else {
            panic!("expected else-if to normalize into else block");
        };
        assert_eq!(else_block.statements.len(), 1);
        assert!(else_block.tail_expr.is_none());
        assert!(matches!(else_block.statements[0].node, Stmt::If(_)));
    }

    #[test]
    fn test_desugar_block_normalizes_tail_representation() {
        use crate::frontend::ast::{Expr, Spanned};

        let expr = Spanned::new(
            Expr::Block(crate::frontend::ast::Block {
                statements: vec![Spanned::new(
                    crate::frontend::ast::Stmt::Expr {
                        expr: Box::new(Spanned::new(
                            Expr::Identifier("x".to_string()),
                            Span::new(2, 3),
                        )),
                        has_semi: false,
                    },
                    Span::new(2, 3),
                )],
                tail_expr: None,
            }),
            Span::new(0, 4),
        );

        let desugared = grouped::desugar_expr(&expr);
        let Expr::Block(block) = desugared.node else {
            panic!("expected block expression");
        };
        assert!(block.statements.is_empty());
        assert_eq!(
            block.tail_expr,
            Some(Box::new(Spanned::new(
                Expr::Identifier("x".to_string()),
                Span::new(2, 3),
            )))
        );
    }

    #[test]
    fn test_desugar_struct_init_lowers_to_function_like_member() {
        use crate::frontend::ast::{
            InitDecl, InitKind, InitOriginKind, Item, ParamDecl, ParamLabel,
            Pattern, Spanned, StructDecl, StructMember, Type,
        };

        let init = Spanned::new(
            InitDecl {
                docs: vec![],
                attributes: vec![],
                modifiers: vec![],
                kind: InitKind::Optional, // Will be inferred from return_type
                receiver: None,
                params: vec![Spanned::new(
                    ParamDecl {
                        label: ParamLabel::None,
                        name: "x".to_string(),
                        ty: Spanned::new(
                            Type::Named {
                                segments: vec!["I32".to_string()],
                            },
                            Span::new(12, 15),
                        ),
                    },
                    Span::new(9, 15),
                )],
                return_type: Some(Spanned::new(
                    Type::Optional(Box::new(Spanned::new(
                        Type::SelfType,
                        Span::new(0, 4),
                    ))),
                    Span::new(0, 4),
                )), // Explicit return type to make it optional
                body: crate::frontend::ast::Block {
                    statements: vec![Spanned::new(
                        crate::frontend::ast::Stmt::Let(Spanned::new(
                            crate::frontend::ast::LetStmt {
                                pattern: Spanned::new(
                                    Pattern::Identifier("y".to_string()),
                                    Span::new(18, 19),
                                ),
                                ty: None,
                                value: None,
                            },
                            Span::new(16, 20),
                        )),
                        Span::new(16, 20),
                    )],
                    tail_expr: None,
                },
            },
            Span::new(0, 20),
        );
        let item = Spanned::new(
            Item::Struct(Spanned::new(
                StructDecl {
                    docs: vec![],
                    attributes: vec![],
                    visibility: None,
                    modifiers: vec![],
                    name: "Point".to_string(),
                    generic_params: vec![],
                    members: vec![Spanned::new(
                        StructMember::Init(init),
                        Span::new(0, 20),
                    )],
                },
                Span::new(0, 20),
            )),
            Span::new(0, 20),
        );

        let desugared = grouped::desugar_item(&item);
        let Item::Struct(struct_decl) = desugared.node else {
            panic!("expected struct item");
        };
        let StructMember::Function(function_decl) =
            &struct_decl.node.members[0].node
        else {
            panic!("expected init to lower into function member");
        };
        assert_eq!(function_decl.node.name, "init");
        assert_eq!(
            function_decl.node.init_origin,
            Some(InitOriginKind::Optional),
        );
        assert!(function_decl.node.attributes.is_empty());
        assert!(matches!(
            function_decl.node.return_type,
            Some(Spanned {
                node: Type::Optional(_),
                ..
            })
        ));
        assert_eq!(function_decl.node.params.len(), 1);
        assert_eq!(function_decl.node.body.statements.len(), 1);
    }

    #[test]
    fn test_desugar_protocol_initializer_lowers_to_function_member() {
        use crate::frontend::ast::{
            InitKind, InitOriginKind, Item, ParamDecl, ParamLabel,
            ProtocolDecl, ProtocolInitMember, ProtocolMember, Spanned, Type,
        };

        let protocol_init_member = Spanned::new(
            ProtocolInitMember {
                docs: vec![],
                attributes: vec![],
                modifiers: vec![],
                kind: InitKind::Plain,
                receiver: None,
                params: vec![Spanned::new(
                    ParamDecl {
                        label: ParamLabel::None,
                        name: "x".to_string(),
                        ty: Spanned::new(
                            Type::Named {
                                segments: vec!["I32".to_string()],
                            },
                            Span::new(8, 11),
                        ),
                    },
                    Span::new(5, 11),
                )],
                return_type: None,
                default_body: Some(crate::frontend::ast::Block {
                    statements: vec![],
                    tail_expr: None,
                }),
            },
            Span::new(0, 12),
        );

        let item = Spanned::new(
            Item::Protocol(Spanned::new(
                ProtocolDecl {
                    docs: vec![],
                    attributes: vec![],
                    visibility: None,
                    modifiers: vec![],
                    name: "Factory".to_string(),
                    generic_params: vec![],
                    inheritance: vec![],
                    members: vec![Spanned::new(
                        ProtocolMember::Initializer(protocol_init_member),
                        Span::new(0, 12),
                    )],
                },
                Span::new(0, 12),
            )),
            Span::new(0, 12),
        );

        let desugared = grouped::desugar_item(&item);
        let Item::Protocol(protocol_decl) = desugared.node else {
            panic!("expected protocol item");
        };
        let ProtocolMember::Function(function_member) =
            &protocol_decl.node.members[0].node
        else {
            panic!(
                "expected protocol initializer to lower into function member"
            );
        };
        assert_eq!(function_member.node.name, "init");
        assert_eq!(
            function_member.node.init_origin,
            Some(InitOriginKind::Plain),
        );
        assert!(function_member.node.attributes.is_empty());
        assert!(matches!(
            function_member.node.return_type,
            Some(Spanned {
                node: Type::SelfType,
                ..
            })
        ));
        assert!(function_member.node.default_body.is_some());
    }
}
