use crate::frontend::ast::{
    ArrayElement, Block, CallArg, Clause, ClauseList, EnumCaseParam, EnumDecl,
    EnumMember, Expr, ExternBlock, ExternFunctionDecl, ExternMember, File,
    ForStmt, FunctionDecl, GuardStmt, IfStmt, IfStmtElse, ImplDecl, ImplMember,
    InitKind, InitOriginKind, Item, LetStmt, MacroExprArgs, MatchArm,
    MatchArmBody, ParamDecl, Pattern, ProtocolDecl, ProtocolFunctionMember,
    ProtocolMember, ProtocolPropertyRequirement, ScopeDecl, Span, Spanned,
    Stmt, StructDecl, StructField, StructLiteralField, StructMember,
    StructPatternField, Type, UseItem, UseTree, VarStmt, WhereClause,
    WherePredicate, WhileStmt,
};

pub(super) fn desugar_file_impl(file: &File) -> File {
    File {
        items: file.items.iter().map(desugar_item).collect(),
    }
}

pub(super) fn desugar_item(item: &Spanned<Item>) -> Spanned<Item> {
    let node = match &item.node {
        Item::Use(use_item) => Item::Use(Spanned::new(
            desugar_use_item(&use_item.node),
            use_item.span,
        )),
        Item::Scope(scope_decl) => Item::Scope(Spanned::new(
            desugar_scope_decl(&scope_decl.node),
            scope_decl.span,
        )),
        Item::Struct(struct_decl) => Item::Struct(Spanned::new(
            desugar_struct_decl(&struct_decl.node),
            struct_decl.span,
        )),
        Item::Enum(enum_decl) => Item::Enum(Spanned::new(
            desugar_enum_decl(&enum_decl.node),
            enum_decl.span,
        )),
        Item::Impl(impl_decl) => Item::Impl(Spanned::new(
            desugar_impl_decl(&impl_decl.node),
            impl_decl.span,
        )),
        Item::Protocol(protocol_decl) => Item::Protocol(Spanned::new(
            desugar_protocol_decl(&protocol_decl.node),
            protocol_decl.span,
        )),
        Item::Function(function_decl) => Item::Function(Spanned::new(
            desugar_function_decl(&function_decl.node),
            function_decl.span,
        )),
        Item::ExternBlock(extern_block) => Item::ExternBlock(Spanned::new(
            desugar_extern_block(&extern_block.node),
            extern_block.span,
        )),
        Item::Macro(macro_decl) => Item::Macro(macro_decl.clone()),
    };

    Spanned::new(node, item.span)
}

pub(super) fn desugar_stmt(stmt: &Spanned<Stmt>) -> Spanned<Stmt> {
    let node = match &stmt.node {
        Stmt::If(if_stmt) => {
            Stmt::If(Spanned::new(desugar_if_stmt(&if_stmt.node), if_stmt.span))
        }
        Stmt::Let(let_stmt) => Stmt::Let(Spanned::new(
            desugar_let_stmt(&let_stmt.node),
            let_stmt.span,
        )),
        Stmt::Var(var_stmt) => Stmt::Var(Spanned::new(
            desugar_var_stmt(&var_stmt.node),
            var_stmt.span,
        )),
        Stmt::Expr { expr, has_semi } => Stmt::Expr {
            expr: Box::new(desugar_expr(expr)),
            has_semi: *has_semi,
        },
        Stmt::Guard(guard_stmt) => Stmt::Guard(Spanned::new(
            desugar_guard_stmt(&guard_stmt.node),
            guard_stmt.span,
        )),
        Stmt::While(while_stmt) => Stmt::While(Spanned::new(
            desugar_while_stmt(&while_stmt.node),
            while_stmt.span,
        )),
        Stmt::For(for_stmt) => Stmt::For(Spanned::new(
            desugar_for_stmt(&for_stmt.node),
            for_stmt.span,
        )),
        Stmt::Return(expr) => Stmt::Return(
            expr.as_ref().map(|inner| Box::new(desugar_expr(inner))),
        ),
        Stmt::Break => Stmt::Break,
        Stmt::Continue => Stmt::Continue,
    };

    Spanned::new(node, stmt.span)
}

pub(super) fn desugar_expr(expr: &Spanned<Expr>) -> Spanned<Expr> {
    match &expr.node {
        Expr::Grouped(inner) => desugar_expr(inner),
        Expr::StringLiteral(literal) => {
            let mut literal = literal.clone();
            for part in &mut literal.parts {
                if let crate::frontend::ast::StringPart::Interpolation(inner) =
                    part
                {
                    *inner = Box::new(desugar_expr(inner));
                }
            }
            Spanned::new(Expr::StringLiteral(literal), expr.span)
        }
        Expr::ArrayLiteral(elements) => Spanned::new(
            Expr::ArrayLiteral(
                elements
                    .iter()
                    .map(|element| match element {
                        ArrayElement::Expr(inner) => {
                            ArrayElement::Expr(Box::new(desugar_expr(inner)))
                        }
                        ArrayElement::Spread(inner) => {
                            ArrayElement::Spread(Box::new(desugar_expr(inner)))
                        }
                    })
                    .collect(),
            ),
            expr.span,
        ),
        Expr::StructLiteral { ty, fields } => Spanned::new(
            Expr::StructLiteral {
                ty: ty.clone(),
                fields: fields
                    .iter()
                    .map(|field| match field {
                        StructLiteralField::Shorthand { name } => {
                            StructLiteralField::Shorthand { name: name.clone() }
                        }
                        StructLiteralField::Named { name, value } => {
                            StructLiteralField::Named {
                                name: name.clone(),
                                value: Box::new(desugar_expr(value)),
                            }
                        }
                        StructLiteralField::Spread { value } => {
                            StructLiteralField::Spread {
                                value: Box::new(desugar_expr(value)),
                            }
                        }
                    })
                    .collect(),
            },
            expr.span,
        ),
        Expr::Block(block) => {
            Spanned::new(Expr::Block(desugar_block(block)), expr.span)
        }
        Expr::UnsafeBlock(block) => {
            Spanned::new(Expr::UnsafeBlock(desugar_block(block)), expr.span)
        }
        Expr::If {
            clauses,
            then_branch,
            else_branch,
        } => Spanned::new(
            Expr::If {
                clauses: desugar_clause_list(clauses),
                then_branch: desugar_block(then_branch),
                else_branch: Some(Box::new(normalize_if_expr_else_branch(
                    else_branch.as_deref().map(desugar_expr),
                    expr.span,
                ))),
            },
            expr.span,
        ),
        Expr::Match { subject, arms } => Spanned::new(
            Expr::Match {
                subject: Box::new(desugar_expr(subject)),
                arms: arms
                    .iter()
                    .map(|arm| {
                        let body = match &arm.node.body {
                            MatchArmBody::Expr(inner) => MatchArmBody::Expr(
                                Box::new(desugar_expr(inner)),
                            ),
                            MatchArmBody::Block(block) => {
                                MatchArmBody::Block(desugar_block(block))
                            }
                        };
                        Spanned::new(
                            MatchArm {
                                pattern: desugar_pattern(&arm.node.pattern),
                                body,
                            },
                            arm.span,
                        )
                    })
                    .collect(),
            },
            expr.span,
        ),
        Expr::Closure {
            params,
            body,
            uses_shorthand_params,
            is_unsafe,
        } => Spanned::new(
            Expr::Closure {
                params: params
                    .iter()
                    .map(|param| crate::frontend::ast::ClosureParam {
                        name: param.name.clone(),
                        ty: param.ty.as_ref().map(desugar_ty),
                    })
                    .collect(),
                body: desugar_block(body),
                uses_shorthand_params: *uses_shorthand_params,
                is_unsafe: *is_unsafe,
            },
            expr.span,
        ),
        Expr::Macro { name, args } => {
            let args = match args {
                MacroExprArgs::Paren(args) => MacroExprArgs::Paren(
                    args.iter()
                        .map(|arg| CallArg {
                            label: arg.label.clone(),
                            value: Box::new(desugar_expr(&arg.value)),
                        })
                        .collect(),
                ),
                MacroExprArgs::Braced(block) => {
                    MacroExprArgs::Braced(block.clone())
                }
            };
            Spanned::new(
                Expr::Macro {
                    name: name.clone(),
                    args,
                },
                expr.span,
            )
        }
        Expr::Unary { op, expr: inner } => Spanned::new(
            Expr::Unary {
                op: op.clone(),
                expr: Box::new(desugar_expr(inner)),
            },
            expr.span,
        ),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => Spanned::new(
            Expr::Ternary {
                condition: Box::new(desugar_expr(condition)),
                then_expr: Box::new(desugar_expr(then_expr)),
                else_expr: Box::new(desugar_expr(else_expr)),
            },
            expr.span,
        ),
        Expr::Binary { op, lhs, rhs } => Spanned::new(
            Expr::Binary {
                op: *op,
                lhs: Box::new(desugar_expr(lhs)),
                rhs: Box::new(desugar_expr(rhs)),
            },
            expr.span,
        ),
        Expr::Assignment { op, target, value } => Spanned::new(
            Expr::Assignment {
                op: *op,
                target: Box::new(desugar_expr(target)),
                value: Box::new(desugar_expr(value)),
            },
            expr.span,
        ),
        Expr::MemberAccess { base, member } => Spanned::new(
            Expr::MemberAccess {
                base: Box::new(desugar_expr(base)),
                member: member.clone(),
            },
            expr.span,
        ),
        Expr::OptionalMemberAccess { base, member } => Spanned::new(
            Expr::OptionalMemberAccess {
                base: Box::new(desugar_expr(base)),
                member: member.clone(),
            },
            expr.span,
        ),
        Expr::NamespaceAccess {
            base,
            member,
            turbofish,
        } => Spanned::new(
            Expr::NamespaceAccess {
                base: Box::new(desugar_expr(base)),
                member: member.clone(),
                turbofish: turbofish.iter().map(desugar_ty).collect(),
            },
            expr.span,
        ),
        Expr::Call {
            callee,
            args,
            trailing_closure,
        } => {
            // Check if this is a method call (callee is MemberAccess)
            // and transform to MethodCall node for later processing
            if matches!(&callee.node, Expr::MemberAccess { .. }) {
                // Transform to MethodCall node
                let (receiver, method_name) = match &callee.node {
                    Expr::MemberAccess { base, member } => {
                        (base, member.clone())
                    }
                    _ => unreachable!(),
                };

                Spanned::new(
                    Expr::MethodCall {
                        receiver: Box::new(desugar_expr(receiver)),
                        method_name,
                        args: args
                            .iter()
                            .map(|arg| CallArg {
                                label: arg.label.clone(),
                                value: Box::new(desugar_expr(&arg.value)),
                            })
                            .collect(),
                        trailing_closure: trailing_closure
                            .as_ref()
                            .map(|c| Box::new(desugar_expr(c))),
                    },
                    expr.span,
                )
            } else {
                // Regular function call
                Spanned::new(
                    Expr::Call {
                        callee: Box::new(desugar_expr(callee)),
                        args: args
                            .iter()
                            .map(|arg| CallArg {
                                label: arg.label.clone(),
                                value: Box::new(desugar_expr(&arg.value)),
                            })
                            .collect(),
                        trailing_closure: trailing_closure
                            .as_ref()
                            .map(|c| Box::new(desugar_expr(c))),
                    },
                    expr.span,
                )
            }
        }
        Expr::MethodCall {
            receiver,
            method_name,
            args,
            trailing_closure,
        } => Spanned::new(
            Expr::MethodCall {
                receiver: Box::new(desugar_expr(receiver)),
                method_name: method_name.clone(),
                args: args
                    .iter()
                    .map(|arg| CallArg {
                        label: arg.label.clone(),
                        value: Box::new(desugar_expr(&arg.value)),
                    })
                    .collect(),
                trailing_closure: trailing_closure
                    .as_ref()
                    .map(|c| Box::new(desugar_expr(c))),
            },
            expr.span,
        ),
        Expr::ConstructorCall { type_name, args } => {
            // Desugar ConstructorCall to explicit NamespaceAccess + Call
            // Point(x, y) -> Point::init(x, y)
            let type_expr =
                Spanned::new(Expr::Identifier(type_name.clone()), expr.span);

            let init_access = Spanned::new(
                Expr::NamespaceAccess {
                    base: Box::new(type_expr),
                    member: "init".to_string(),
                    turbofish: vec![],
                },
                expr.span,
            );

            Spanned::new(
                Expr::Call {
                    callee: Box::new(init_access),
                    args: args
                        .iter()
                        .map(|arg| CallArg {
                            label: arg.label.clone(),
                            value: Box::new(desugar_expr(&arg.value)),
                        })
                        .collect(),
                    trailing_closure: None,
                },
                expr.span,
            )
        }
        Expr::Index { base, index } => Spanned::new(
            Expr::Index {
                base: Box::new(desugar_expr(base)),
                index: Box::new(desugar_expr(index)),
            },
            expr.span,
        ),
        Expr::OptionalIndex { base, index } => Spanned::new(
            Expr::OptionalIndex {
                base: Box::new(desugar_expr(base)),
                index: Box::new(desugar_expr(index)),
            },
            expr.span,
        ),
        Expr::ForceUnwrap { expr: inner } => Spanned::new(
            Expr::ForceUnwrap {
                expr: Box::new(desugar_expr(inner)),
            },
            expr.span,
        ),
        Expr::Cast {
            expr: inner,
            ty,
            is_optional,
        } => Spanned::new(
            Expr::Cast {
                expr: Box::new(desugar_expr(inner)),
                ty: desugar_ty(ty),
                is_optional: *is_optional,
            },
            expr.span,
        ),
        Expr::Range {
            start,
            end,
            inclusive,
        } => Spanned::new(
            Expr::Range {
                start: start
                    .as_ref()
                    .map(|inner| Box::new(desugar_expr(inner))),
                end: end.as_ref().map(|inner| Box::new(desugar_expr(inner))),
                inclusive: *inclusive,
            },
            expr.span,
        ),
        Expr::Spread { expr: inner } => Spanned::new(
            Expr::Spread {
                expr: Box::new(desugar_expr(inner)),
            },
            expr.span,
        ),
        Expr::Try { expr: inner } => Spanned::new(
            Expr::Try {
                expr: Box::new(desugar_expr(inner)),
            },
            expr.span,
        ),
        Expr::QualifiedMember { qualifier, member } => Spanned::new(
            Expr::QualifiedMember {
                qualifier: Box::new(desugar_expr(qualifier)),
                member: member.clone(),
            },
            expr.span,
        ),
        Expr::IntegerLiteral(value) => {
            Spanned::new(Expr::IntegerLiteral(value.clone()), expr.span)
        }
        Expr::FloatLiteral(value) => {
            Spanned::new(Expr::FloatLiteral(value.clone()), expr.span)
        }
        Expr::CharLiteral(value) => {
            Spanned::new(Expr::CharLiteral(value.clone()), expr.span)
        }
        Expr::BooleanLiteral(value) => {
            Spanned::new(Expr::BooleanLiteral(*value), expr.span)
        }
        Expr::Identifier(name) => {
            Spanned::new(Expr::Identifier(name.clone()), expr.span)
        }
        Expr::SelfValue => Spanned::new(Expr::SelfValue, expr.span),
        Expr::SelfType => Spanned::new(Expr::SelfType, expr.span),
        Expr::ShorthandMember { name } => Spanned::new(
            Expr::ShorthandMember { name: name.clone() },
            expr.span,
        ),
        Expr::Tuple(elems) => {
            let desugared: Vec<_> = elems.iter().map(desugar_expr).collect();
            if desugared.len() == 1 {
                Spanned::new(desugared[0].node.clone(), desugared[0].span)
            } else {
                Spanned::new(Expr::Tuple(desugared), expr.span)
            }
        }
    }
}

pub(super) fn desugar_ty(ty: &Spanned<Type>) -> Spanned<Type> {
    match &ty.node {
        Type::Tuple(elems) => {
            let desugared: Vec<_> = elems.iter().map(desugar_ty).collect();
            if desugared.len() == 1 {
                Spanned::new(desugared[0].node.clone(), desugared[0].span)
            } else {
                Spanned::new(Type::Tuple(desugared), ty.span)
            }
        }
        Type::Named { segments } => Spanned::new(
            Type::Named {
                segments: segments.clone(),
            },
            ty.span,
        ),
        Type::Lifetime(lifetime) => {
            Spanned::new(Type::Lifetime(lifetime.clone()), ty.span)
        }
        Type::GenericApplication { base, args } => Spanned::new(
            Type::GenericApplication {
                base: Box::new(desugar_ty(base)),
                args: args.iter().map(desugar_ty).collect(),
            },
            ty.span,
        ),
        Type::SelfType => Spanned::new(Type::SelfType, ty.span),
        Type::Reference { lifetime, inner } => Spanned::new(
            Type::Reference {
                lifetime: lifetime.clone(),
                inner: Box::new(desugar_ty(inner)),
            },
            ty.span,
        ),
        Type::MutableReference { lifetime, inner } => Spanned::new(
            Type::MutableReference {
                lifetime: lifetime.clone(),
                inner: Box::new(desugar_ty(inner)),
            },
            ty.span,
        ),
        Type::ConstPointer(inner) => Spanned::new(
            Type::ConstPointer(Box::new(desugar_ty(inner))),
            ty.span,
        ),
        Type::MutablePointer(inner) => Spanned::new(
            Type::MutablePointer(Box::new(desugar_ty(inner))),
            ty.span,
        ),
        Type::Array(inner) => {
            Spanned::new(Type::Array(Box::new(desugar_ty(inner))), ty.span)
        }
        Type::Optional(inner) => {
            Spanned::new(Type::Optional(Box::new(desugar_ty(inner))), ty.span)
        }
        Type::Result { ok, err } => Spanned::new(
            Type::Result {
                ok: Box::new(desugar_ty(ok)),
                err: Box::new(desugar_ty(err)),
            },
            ty.span,
        ),
    }
}

pub(super) fn desugar_pattern(pattern: &Spanned<Pattern>) -> Spanned<Pattern> {
    let node = match &pattern.node {
        Pattern::Identifier(name) => Pattern::Identifier(name.clone()),
        Pattern::Wildcard => Pattern::Wildcard,
        Pattern::IntegerLiteral(value) => {
            Pattern::IntegerLiteral(value.clone())
        }
        Pattern::BooleanLiteral(value) => Pattern::BooleanLiteral(*value),
        Pattern::CharLiteral(value) => Pattern::CharLiteral(value.clone()),
        Pattern::StringLiteral(value) => Pattern::StringLiteral(value.clone()),
        Pattern::Tuple(patterns) => {
            Pattern::Tuple(patterns.iter().map(desugar_pattern).collect())
        }
        Pattern::Variant {
            path,
            shorthand,
            args,
            has_rest,
        } => Pattern::Variant {
            path: path.clone(),
            shorthand: *shorthand,
            args: args.iter().map(desugar_pattern).collect(),
            has_rest: *has_rest,
        },
        Pattern::Struct {
            path,
            fields,
            has_rest,
        } => Pattern::Struct {
            path: path.clone(),
            fields: fields
                .iter()
                .map(|field| StructPatternField {
                    name: field.name.clone(),
                    pattern: field.pattern.as_ref().map(desugar_pattern),
                })
                .collect(),
            has_rest: *has_rest,
        },
        Pattern::Array { elements, rest } => Pattern::Array {
            elements: elements.iter().map(desugar_pattern).collect(),
            rest: rest.clone(),
        },
    };

    Spanned::new(node, pattern.span)
}

fn desugar_use_item(use_item: &UseItem) -> UseItem {
    UseItem {
        visibility: use_item.visibility,
        tree: Spanned::new(
            desugar_use_tree(&use_item.tree.node),
            use_item.tree.span,
        ),
    }
}

fn desugar_use_tree(tree: &UseTree) -> UseTree {
    match tree {
        UseTree::Path { path } => UseTree::Path { path: path.clone() },
        UseTree::Glob { path } => UseTree::Glob { path: path.clone() },
        UseTree::Alias { path, alias } => UseTree::Alias {
            path: path.clone(),
            alias: alias.clone(),
        },
        UseTree::Group { path, items } => UseTree::Group {
            path: path.clone(),
            items: items
                .iter()
                .map(|item| {
                    Spanned::new(desugar_use_tree(&item.node), item.span)
                })
                .collect(),
        },
        UseTree::SelfImport => UseTree::SelfImport,
        UseTree::SelfAlias { alias } => UseTree::SelfAlias {
            alias: alias.clone(),
        },
    }
}

fn desugar_scope_decl(scope_decl: &ScopeDecl) -> ScopeDecl {
    scope_decl.clone()
}

fn desugar_function_decl(function_decl: &FunctionDecl) -> FunctionDecl {
    FunctionDecl {
        docs: function_decl.docs.clone(),
        attributes: function_decl.attributes.clone(),
        visibility: function_decl.visibility,
        modifiers: function_decl.modifiers.clone(),
        name: function_decl.name.clone(),
        generic_params: function_decl.generic_params.clone(),
        receiver: function_decl.receiver.clone(),
        params: function_decl
            .params
            .iter()
            .map(|param| {
                Spanned::new(desugar_param_decl(&param.node), param.span)
            })
            .collect(),
        return_type: function_decl.return_type.as_ref().map(desugar_ty),
        where_clause: function_decl.where_clause.as_ref().map(|where_clause| {
            Spanned::new(
                desugar_where_clause(&where_clause.node),
                where_clause.span,
            )
        }),
        init_origin: function_decl.init_origin,
        body: desugar_block(&function_decl.body),
    }
}

fn desugar_param_decl(param_decl: &ParamDecl) -> ParamDecl {
    ParamDecl {
        label: param_decl.label.clone(),
        name: param_decl.name.clone(),
        ty: desugar_ty(&param_decl.ty),
    }
}

fn desugar_where_clause(where_clause: &WhereClause) -> WhereClause {
    WhereClause {
        predicates: where_clause
            .predicates
            .iter()
            .map(|predicate| {
                Spanned::new(
                    desugar_where_predicate(&predicate.node),
                    predicate.span,
                )
            })
            .collect(),
    }
}

fn desugar_where_predicate(predicate: &WherePredicate) -> WherePredicate {
    WherePredicate {
        ty: desugar_ty(&predicate.ty),
        bounds: predicate.bounds.iter().map(desugar_ty).collect(),
    }
}

fn desugar_struct_decl(struct_decl: &StructDecl) -> StructDecl {
    StructDecl {
        docs: struct_decl.docs.clone(),
        attributes: struct_decl.attributes.clone(),
        visibility: struct_decl.visibility,
        modifiers: struct_decl.modifiers.clone(),
        name: struct_decl.name.clone(),
        generic_params: struct_decl.generic_params.clone(),
        members: struct_decl
            .members
            .iter()
            .map(|member| {
                let node = match &member.node {
                    StructMember::Field(field) => {
                        StructMember::Field(Spanned::new(
                            desugar_struct_field(&field.node),
                            field.span,
                        ))
                    }
                    StructMember::Init(init_decl) => {
                        StructMember::Function(Spanned::new(
                            desugar_init_as_function_decl(
                                &init_decl.node.docs,
                                &init_decl.node.attributes,
                                &init_decl.node.modifiers,
                                init_decl.node.kind,
                                init_decl.node.receiver.as_ref(),
                                &init_decl.node.params,
                                &init_decl.node.return_type,
                                &init_decl.node.body,
                                init_decl.span,
                            ),
                            init_decl.span,
                        ))
                    }
                    StructMember::Function(function_decl) => {
                        StructMember::Function(Spanned::new(
                            desugar_function_decl(&function_decl.node),
                            function_decl.span,
                        ))
                    }
                };
                Spanned::new(node, member.span)
            })
            .collect(),
    }
}

fn desugar_struct_field(field: &StructField) -> StructField {
    StructField {
        docs: field.docs.clone(),
        attributes: field.attributes.clone(),
        name: field.name.clone(),
        ty: desugar_ty(&field.ty),
    }
}

fn desugar_enum_decl(enum_decl: &EnumDecl) -> EnumDecl {
    EnumDecl {
        docs: enum_decl.docs.clone(),
        attributes: enum_decl.attributes.clone(),
        visibility: enum_decl.visibility,
        modifiers: enum_decl.modifiers.clone(),
        name: enum_decl.name.clone(),
        generic_params: enum_decl.generic_params.clone(),
        members: enum_decl
            .members
            .iter()
            .map(|member| {
                let node = match &member.node {
                    EnumMember::Case(enum_case) => {
                        EnumMember::Case(Spanned::new(
                            desugar_enum_case(&enum_case.node),
                            enum_case.span,
                        ))
                    }
                    EnumMember::Init(init_decl) => {
                        EnumMember::Function(Spanned::new(
                            desugar_init_as_function_decl(
                                &init_decl.node.docs,
                                &init_decl.node.attributes,
                                &init_decl.node.modifiers,
                                init_decl.node.kind,
                                init_decl.node.receiver.as_ref(),
                                &init_decl.node.params,
                                &init_decl.node.return_type,
                                &init_decl.node.body,
                                init_decl.span,
                            ),
                            init_decl.span,
                        ))
                    }
                    EnumMember::Function(function_decl) => {
                        EnumMember::Function(Spanned::new(
                            desugar_function_decl(&function_decl.node),
                            function_decl.span,
                        ))
                    }
                };
                Spanned::new(node, member.span)
            })
            .collect(),
    }
}

fn desugar_enum_case(
    enum_case: &crate::frontend::ast::EnumCase,
) -> crate::frontend::ast::EnumCase {
    crate::frontend::ast::EnumCase {
        docs: enum_case.docs.clone(),
        attributes: enum_case.attributes.clone(),
        name: enum_case.name.clone(),
        payload: enum_case
            .payload
            .iter()
            .map(|param| {
                let node = match &param.node {
                    EnumCaseParam::Unnamed(ty) => {
                        EnumCaseParam::Unnamed(desugar_ty(ty))
                    }
                    EnumCaseParam::Named { name, ty } => EnumCaseParam::Named {
                        name: name.clone(),
                        ty: desugar_ty(ty),
                    },
                };
                Spanned::new(node, param.span)
            })
            .collect(),
    }
}

fn desugar_impl_decl(impl_decl: &ImplDecl) -> ImplDecl {
    ImplDecl {
        docs: impl_decl.docs.clone(),
        attributes: impl_decl.attributes.clone(),
        modifiers: impl_decl.modifiers.clone(),
        lifetime_params: impl_decl.lifetime_params.clone(),
        target: desugar_ty(&impl_decl.target),
        conformance: impl_decl.conformance.as_ref().map(desugar_ty),
        members: impl_decl
            .members
            .iter()
            .map(|member| {
                let node = match &member.node {
                    ImplMember::Init(init_decl) => {
                        ImplMember::Function(Spanned::new(
                            desugar_init_as_function_decl(
                                &init_decl.node.docs,
                                &init_decl.node.attributes,
                                &init_decl.node.modifiers,
                                init_decl.node.kind,
                                init_decl.node.receiver.as_ref(),
                                &init_decl.node.params,
                                &init_decl.node.return_type,
                                &init_decl.node.body,
                                init_decl.span,
                            ),
                            init_decl.span,
                        ))
                    }
                    ImplMember::Function(function_decl) => {
                        ImplMember::Function(Spanned::new(
                            desugar_function_decl(&function_decl.node),
                            function_decl.span,
                        ))
                    }
                    ImplMember::AssociatedType(assoc) => {
                        ImplMember::AssociatedType(Spanned::new(
                            assoc.node.clone(),
                            assoc.span,
                        ))
                    }
                };
                Spanned::new(node, member.span)
            })
            .collect(),
    }
}

fn desugar_protocol_decl(protocol_decl: &ProtocolDecl) -> ProtocolDecl {
    ProtocolDecl {
        docs: protocol_decl.docs.clone(),
        attributes: protocol_decl.attributes.clone(),
        visibility: protocol_decl.visibility,
        modifiers: protocol_decl.modifiers.clone(),
        name: protocol_decl.name.clone(),
        generic_params: protocol_decl.generic_params.clone(),
        inheritance: protocol_decl.inheritance.iter().map(desugar_ty).collect(),
        members: protocol_decl
            .members
            .iter()
            .map(|member| {
                let node = match &member.node {
                    ProtocolMember::Function(function_member) => {
                        ProtocolMember::Function(Spanned::new(
                            desugar_protocol_function_member(
                                &function_member.node,
                            ),
                            function_member.span,
                        ))
                    }
                    ProtocolMember::Initializer(init_member) => {
                        ProtocolMember::Function(Spanned::new(
                            desugar_init_protocol_member_as_function_member(
                                &init_member.node.docs,
                                &init_member.node.attributes,
                                &init_member.node.modifiers,
                                init_member.node.kind,
                                init_member.node.receiver.as_ref(),
                                &init_member.node.params,
                                &init_member.node.return_type,
                                init_member.node.default_body.as_ref(),
                                init_member.span,
                            ),
                            init_member.span,
                        ))
                    }
                    ProtocolMember::AssociatedType(associated_type) => {
                        let mut associated_type = associated_type.clone();
                        associated_type.node.bounds = associated_type
                            .node
                            .bounds
                            .iter()
                            .map(desugar_ty)
                            .collect();
                        ProtocolMember::AssociatedType(associated_type)
                    }
                    ProtocolMember::Property(property) => {
                        ProtocolMember::Property(Spanned::new(
                            desugar_protocol_property_requirement(
                                &property.node,
                            ),
                            property.span,
                        ))
                    }
                };
                Spanned::new(node, member.span)
            })
            .collect(),
    }
}

fn desugar_protocol_function_member(
    function_member: &ProtocolFunctionMember,
) -> ProtocolFunctionMember {
    ProtocolFunctionMember {
        docs: function_member.docs.clone(),
        attributes: function_member.attributes.clone(),
        modifiers: function_member.modifiers.clone(),
        name: function_member.name.clone(),
        generic_params: function_member.generic_params.clone(),
        receiver: function_member.receiver.clone(),
        params: function_member
            .params
            .iter()
            .map(|param| {
                Spanned::new(desugar_param_decl(&param.node), param.span)
            })
            .collect(),
        return_type: function_member.return_type.as_ref().map(desugar_ty),
        where_clause: function_member.where_clause.as_ref().map(
            |where_clause| {
                Spanned::new(
                    desugar_where_clause(&where_clause.node),
                    where_clause.span,
                )
            },
        ),
        init_origin: function_member.init_origin,
        default_body: function_member.default_body.as_ref().map(desugar_block),
    }
}

fn desugar_protocol_property_requirement(
    property: &ProtocolPropertyRequirement,
) -> ProtocolPropertyRequirement {
    ProtocolPropertyRequirement {
        docs: property.docs.clone(),
        attributes: property.attributes.clone(),
        modifiers: property.modifiers.clone(),
        binding: property.binding,
        name: property.name.clone(),
        ty: desugar_ty(&property.ty),
        accessors: property.accessors.clone(),
    }
}

fn infer_init_kind_from_return_type(
    return_type: &Option<Spanned<Type>>,
) -> InitKind {
    match return_type {
        None => InitKind::Plain, // Default to Self
        Some(ty) => match &ty.node {
            Type::SelfType => InitKind::Plain,
            Type::Optional(inner) if matches!(inner.node, Type::SelfType) => {
                InitKind::Optional
            }
            Type::Result { ok, .. } if matches!(ok.node, Type::SelfType) => {
                InitKind::Fallible
            }
            // Handle Option<Self> as GenericApplication
            Type::GenericApplication { base, args } if args.len() == 1 => {
                match &base.node {
                    Type::Named { segments }
                        if segments.len() == 1 && segments[0] == "Option" =>
                    {
                        if matches!(args[0].node, Type::SelfType) {
                            InitKind::Optional
                        } else {
                            InitKind::Plain
                        }
                    }
                    Type::Named { segments }
                        if segments.len() == 1 && segments[0] == "Result" =>
                    {
                        // Result<Self, E> - we only care about the OK type
                        if matches!(args[0].node, Type::SelfType) {
                            InitKind::Fallible
                        } else {
                            InitKind::Plain
                        }
                    }
                    _ => InitKind::Plain,
                }
            }
            _ => InitKind::Plain, // User specified custom return type, default to Plain
        },
    }
}

fn init_return_type(kind: InitKind, span: Span) -> Spanned<Type> {
    let self_ty = Spanned::new(Type::SelfType, span);
    match kind {
        InitKind::Optional => {
            Spanned::new(Type::Optional(Box::new(self_ty)), span)
        }
        InitKind::Fallible => {
            // For fallible inits, we require explicit return type annotation
            // This will be overridden by the user's annotation if present
            self_ty
        }
        InitKind::Plain => self_ty,
    }
}

fn init_origin_kind(kind: InitKind) -> InitOriginKind {
    match kind {
        InitKind::Plain => InitOriginKind::Plain,
        InitKind::Optional => InitOriginKind::Optional,
        InitKind::Fallible => InitOriginKind::Fallible,
    }
}

fn desugar_init_as_function_decl(
    docs: &[Spanned<crate::frontend::ast::DocComment>],
    attributes: &[Spanned<crate::frontend::ast::Attribute>],
    modifiers: &[crate::frontend::ast::Modifier],
    _kind: InitKind,
    receiver: Option<&Spanned<crate::frontend::ast::ReceiverKind>>,
    params: &[Spanned<ParamDecl>],
    return_type: &Option<Spanned<Type>>,
    body: &Block,
    span: Span,
) -> FunctionDecl {
    // Infer the actual kind from return type annotation
    let inferred_kind = infer_init_kind_from_return_type(return_type);

    FunctionDecl {
        docs: docs.to_vec(),
        attributes: attributes.to_vec(),
        visibility: None,
        modifiers: modifiers.to_vec(),
        name: "init".to_string(),
        generic_params: vec![],
        receiver: receiver.cloned(),
        params: params
            .iter()
            .map(|param| {
                Spanned::new(desugar_param_decl(&param.node), param.span)
            })
            .collect(),
        // Use explicit return type if provided, otherwise generate default from inferred kind
        return_type: return_type
            .as_ref()
            .map(|ty| desugar_ty(ty))
            .or_else(|| Some(init_return_type(inferred_kind, span))),
        where_clause: None,
        init_origin: Some(init_origin_kind(inferred_kind)),
        body: desugar_block(body),
    }
}

fn desugar_init_protocol_member_as_function_member(
    docs: &[Spanned<crate::frontend::ast::DocComment>],
    attributes: &[Spanned<crate::frontend::ast::Attribute>],
    modifiers: &[crate::frontend::ast::Modifier],
    _kind: InitKind,
    receiver: Option<&Spanned<crate::frontend::ast::ReceiverKind>>,
    params: &[Spanned<ParamDecl>],
    return_type: &Option<Spanned<Type>>,
    default_body: Option<&Block>,
    span: Span,
) -> ProtocolFunctionMember {
    // Infer the actual kind from return type annotation
    let inferred_kind = infer_init_kind_from_return_type(return_type);

    ProtocolFunctionMember {
        docs: docs.to_vec(),
        attributes: attributes.to_vec(),
        modifiers: modifiers.to_vec(),
        name: "init".to_string(),
        generic_params: vec![],
        receiver: receiver.cloned(),
        params: params
            .iter()
            .map(|param| {
                Spanned::new(desugar_param_decl(&param.node), param.span)
            })
            .collect(),
        // Use explicit return type if provided, otherwise generate default from inferred kind
        return_type: return_type
            .as_ref()
            .map(|ty| desugar_ty(ty))
            .or_else(|| Some(init_return_type(inferred_kind, span))),
        where_clause: None,
        init_origin: Some(init_origin_kind(inferred_kind)),
        default_body: default_body.map(desugar_block),
    }
}

fn desugar_extern_block(extern_block: &ExternBlock) -> ExternBlock {
    ExternBlock {
        docs: extern_block.docs.clone(),
        attributes: extern_block.attributes.clone(),
        library_name: extern_block.library_name.clone(),
        members: extern_block
            .members
            .iter()
            .map(|member| {
                let node = match &member.node {
                    ExternMember::Function(function_decl) => {
                        ExternMember::Function(Spanned::new(
                            desugar_extern_function_decl(&function_decl.node),
                            function_decl.span,
                        ))
                    }
                };
                Spanned::new(node, member.span)
            })
            .collect(),
    }
}

fn desugar_extern_function_decl(
    function_decl: &ExternFunctionDecl,
) -> ExternFunctionDecl {
    ExternFunctionDecl {
        docs: function_decl.docs.clone(),
        attributes: function_decl.attributes.clone(),
        local_name: function_decl.local_name.clone(),
        native_symbol: function_decl.native_symbol.clone(),
        params: function_decl
            .params
            .iter()
            .map(|param| {
                Spanned::new(desugar_param_decl(&param.node), param.span)
            })
            .collect(),
        return_type: function_decl.return_type.as_ref().map(desugar_ty),
    }
}

fn desugar_block(block: &Block) -> Block {
    let mut statements: Vec<Spanned<Stmt>> =
        block.statements.iter().map(desugar_stmt).collect();
    let mut tail_expr = block
        .tail_expr
        .as_ref()
        .map(|tail_expr| Box::new(desugar_expr(tail_expr)));

    if tail_expr.is_none() {
        let inferred_tail =
            statements.last().and_then(|stmt| match &stmt.node {
                Stmt::Expr {
                    has_semi: false, ..
                } => Some(()),
                _ => None,
            });

        if inferred_tail.is_some() {
            let Some(tail_stmt) = statements.pop() else {
                return Block {
                    statements,
                    tail_expr,
                };
            };
            if let Stmt::Expr { expr, .. } = tail_stmt.node {
                tail_expr = Some(expr);
            }
        }
    }

    for statement in &mut statements {
        if let Stmt::Expr { has_semi, .. } = &mut statement.node {
            *has_semi = true;
        }
    }

    Block {
        statements,
        tail_expr,
    }
}

fn desugar_if_stmt(if_stmt: &IfStmt) -> IfStmt {
    IfStmt {
        clauses: desugar_clause_list(&if_stmt.clauses),
        then_branch: desugar_block(&if_stmt.then_branch),
        else_branch: if_stmt.else_branch.as_ref().map(|else_branch| {
            match else_branch {
                IfStmtElse::If(else_if) => {
                    let nested_if = Spanned::new(
                        desugar_if_stmt(&else_if.node),
                        else_if.span,
                    );
                    let nested_stmt =
                        Spanned::new(Stmt::If(nested_if), else_if.span);
                    IfStmtElse::Block(desugar_block(&Block {
                        statements: vec![nested_stmt],
                        tail_expr: None,
                    }))
                }
                IfStmtElse::Block(block) => {
                    IfStmtElse::Block(desugar_block(block))
                }
            }
        }),
    }
}

fn normalize_if_expr_else_branch(
    else_branch: Option<Spanned<Expr>>,
    if_span: Span,
) -> Spanned<Expr> {
    let else_expr = else_branch.unwrap_or_else(|| {
        Spanned::new(
            Expr::Block(Block {
                statements: vec![],
                tail_expr: None,
            }),
            Span::new(if_span.end, if_span.end),
        )
    });
    ensure_expr_is_block(else_expr)
}

fn ensure_expr_is_block(expr: Spanned<Expr>) -> Spanned<Expr> {
    let span = expr.span;
    match expr.node {
        Expr::Block(block) => Spanned::new(Expr::Block(block), span),
        _ => Spanned::new(
            Expr::Block(Block {
                statements: vec![],
                tail_expr: Some(Box::new(expr)),
            }),
            span,
        ),
    }
}

fn desugar_clause_list(clauses: &ClauseList) -> ClauseList {
    ClauseList {
        clauses: clauses
            .clauses
            .iter()
            .map(|clause| {
                let node = match &clause.node {
                    Clause::Expr(expr) => {
                        Clause::Expr(Box::new(desugar_expr(expr)))
                    }
                    Clause::LetBinding(binding) => Clause::LetBinding(
                        crate::frontend::ast::BindingClause {
                            pattern: desugar_pattern(&binding.pattern),
                            ty: binding.ty.as_ref().map(desugar_ty),
                            value: Box::new(desugar_expr(&binding.value)),
                        },
                    ),
                    Clause::VarBinding(binding) => Clause::VarBinding(
                        crate::frontend::ast::BindingClause {
                            pattern: desugar_pattern(&binding.pattern),
                            ty: binding.ty.as_ref().map(desugar_ty),
                            value: Box::new(desugar_expr(&binding.value)),
                        },
                    ),
                };
                Spanned::new(node, clause.span)
            })
            .collect(),
    }
}

fn desugar_let_stmt(let_stmt: &LetStmt) -> LetStmt {
    LetStmt {
        pattern: desugar_pattern(&let_stmt.pattern),
        ty: let_stmt.ty.as_ref().map(desugar_ty),
        value: let_stmt
            .value
            .as_ref()
            .map(|value| Box::new(desugar_expr(value))),
    }
}

fn desugar_var_stmt(var_stmt: &VarStmt) -> VarStmt {
    VarStmt {
        pattern: desugar_pattern(&var_stmt.pattern),
        ty: var_stmt.ty.as_ref().map(desugar_ty),
        value: var_stmt
            .value
            .as_ref()
            .map(|value| Box::new(desugar_expr(value))),
    }
}

fn desugar_guard_stmt(guard_stmt: &GuardStmt) -> GuardStmt {
    GuardStmt {
        clauses: desugar_clause_list(&guard_stmt.clauses),
        else_block: desugar_block(&guard_stmt.else_block),
    }
}

fn desugar_while_stmt(while_stmt: &WhileStmt) -> WhileStmt {
    WhileStmt {
        clauses: desugar_clause_list(&while_stmt.clauses),
        body: desugar_block(&while_stmt.body),
    }
}

fn desugar_for_stmt(for_stmt: &ForStmt) -> ForStmt {
    ForStmt {
        pattern: desugar_pattern(&for_stmt.pattern),
        iterator: Box::new(desugar_expr(&for_stmt.iterator)),
        body: desugar_block(&for_stmt.body),
    }
}
