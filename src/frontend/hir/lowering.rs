use super::{
    HirArrayElement, HirAssignOp, HirBinaryOp, HirBody, HirBodyId, HirCallArg,
    HirClosureParam, HirEnum, HirEnumVariant, HirExpr, HirExprId, HirExprKind,
    HirExtern, HirExternFunction, HirFile, HirFunction, HirFunctionParam,
    HirFunctionSignature, HirImpl, HirInitOrigin, HirItem, HirItemId,
    HirItemKind, HirLetStmt, HirLiteral, HirMatchArm, HirModule, HirMutability,
    HirOrigin, HirPat, HirPatId, HirPatKind, HirPath, HirProtocol,
    HirProtocolFunction, HirStmt, HirStmtId, HirStmtKind, HirStruct,
    HirStructExprField, HirStructField, HirStructPatField, HirType, HirTypeId,
    HirTypeKind, HirUnaryOp, HirUse, HirUseTree,
};
use crate::frontend::ast::{
    ArrayElement, AssignOp, BinaryOp, Block, CallArg, Clause, ClauseList,
    EnumCaseParam, EnumDecl, EnumMember, Expr, ExternBlock, ExternFunctionDecl,
    ExternMember, ForStmt, FunctionDecl, GuardStmt, IfStmt, IfStmtElse,
    ImplDecl, ImplMember, InitDecl, InitKind, InitOriginKind, Item, LetStmt,
    ParamDecl, ParamLabel, Pattern, ProtocolDecl, ProtocolFunctionMember,
    ProtocolInitMember, ProtocolMember, Spanned, Stmt, StructDecl,
    StructMember, StructPatternField, Type, TypeExpr, UnaryOp, UseItem,
    UsePath, UseTree, VarStmt, WhileStmt,
};
use crate::frontend::desugar::DesugaredFile;
use crate::frontend::expansion::Provenance;

struct LoweringCtx<'a> {
    desugared: &'a DesugaredFile,
    module: HirModule,
}

/// Lowers one desugared frontend file into HIR containers.
#[must_use]
pub fn lower_to_hir(desugared: &DesugaredFile) -> (HirFile, HirModule) {
    LoweringCtx {
        desugared,
        module: HirModule::new(),
    }
    .lower_file()
}

impl<'a> LoweringCtx<'a> {
    fn lower_file(mut self) -> (HirFile, HirModule) {
        let mut root_items = Vec::new();

        for item in &self.desugared.ast.items {
            if let Some(item_id) = self.lower_top_level_item(item) {
                root_items.push(item_id);
            }
        }

        (
            HirFile {
                file_id: self.desugared.file_id,
                root_items,
            },
            self.module,
        )
    }

    fn lower_top_level_item(
        &mut self,
        item: &Spanned<Item>,
    ) -> Option<HirItemId> {
        let kind = match &item.node {
            Item::Function(function) => HirItemKind::Function(
                self.lower_function_decl(&function.node, function.span),
            ),
            Item::Struct(struct_decl) => HirItemKind::Struct(
                self.lower_struct_decl(&struct_decl.node, struct_decl.span),
            ),
            Item::Enum(enum_decl) => HirItemKind::Enum(
                self.lower_enum_decl(&enum_decl.node, enum_decl.span),
            ),
            Item::Protocol(protocol_decl) => {
                HirItemKind::Protocol(self.lower_protocol_decl(
                    &protocol_decl.node,
                    protocol_decl.span,
                ))
            }
            Item::Impl(impl_decl) => HirItemKind::Impl(
                self.lower_impl_decl(&impl_decl.node, impl_decl.span),
            ),
            Item::ExternBlock(extern_block) => HirItemKind::Extern(
                self.lower_extern_block(&extern_block.node, extern_block.span),
            ),
            Item::Use(use_item) => {
                HirItemKind::Use(self.lower_use_item(&use_item.node))
            }
            // Canonical HIR item set intentionally omits source `scope` and
            // declarative `macro` declarations.
            Item::Scope(_) | Item::Macro(_) => return None,
        };

        Some(self.alloc_item(item.span, kind))
    }

    fn lower_struct_decl(
        &mut self,
        struct_decl: &StructDecl,
        _fallback_span: crate::frontend::ast::Span,
    ) -> HirStruct {
        let mut fields = Vec::new();
        let mut functions = Vec::new();

        for member in &struct_decl.members {
            match &member.node {
                StructMember::Field(field) => fields.push(HirStructField {
                    name: field.node.name.clone(),
                    ty: self.lower_type(&field.node.ty),
                }),
                StructMember::Init(init_decl) => functions.push(
                    self.lower_init_decl(&init_decl.node, init_decl.span),
                ),
                StructMember::Function(function_decl) => {
                    functions.push(self.lower_function_decl(
                        &function_decl.node,
                        function_decl.span,
                    ))
                }
            }
        }

        HirStruct {
            name: struct_decl.name.clone(),
            generic_params: struct_decl
                .generic_params
                .iter()
                .map(|param| param.node.name.clone())
                .collect(),
            fields,
            functions,
        }
    }

    fn lower_enum_decl(
        &mut self,
        enum_decl: &EnumDecl,
        _fallback_span: crate::frontend::ast::Span,
    ) -> HirEnum {
        let mut variants = Vec::new();
        let mut functions = Vec::new();

        for member in &enum_decl.members {
            match &member.node {
                EnumMember::Case(case_decl) => {
                    let payload = case_decl
                        .node
                        .payload
                        .iter()
                        .map(|param| match &param.node {
                            EnumCaseParam::Unnamed(ty) => self.lower_type(ty),
                            EnumCaseParam::Named { ty, .. } => {
                                self.lower_type(ty)
                            }
                        })
                        .collect();
                    variants.push(HirEnumVariant {
                        name: case_decl.node.name.clone(),
                        payload,
                    });
                }
                EnumMember::Init(init_decl) => functions.push(
                    self.lower_init_decl(&init_decl.node, init_decl.span),
                ),
                EnumMember::Function(function_decl) => {
                    functions.push(self.lower_function_decl(
                        &function_decl.node,
                        function_decl.span,
                    ))
                }
            }
        }

        HirEnum {
            name: enum_decl.name.clone(),
            generic_params: enum_decl
                .generic_params
                .iter()
                .map(|param| param.node.name.clone())
                .collect(),
            variants,
            functions,
        }
    }

    fn lower_protocol_decl(
        &mut self,
        protocol_decl: &ProtocolDecl,
        _fallback_span: crate::frontend::ast::Span,
    ) -> HirProtocol {
        let mut functions = Vec::new();

        for member in &protocol_decl.members {
            match &member.node {
                ProtocolMember::Function(function_member) => {
                    functions.push(self.lower_protocol_function_member(
                        &function_member.node,
                        function_member.span,
                    ))
                }
                ProtocolMember::Initializer(init_member) => {
                    functions.push(self.lower_protocol_init_member(
                        &init_member.node,
                        init_member.span,
                    ))
                }
                ProtocolMember::AssociatedType(_)
                | ProtocolMember::Property(_) => {}
            }
        }

        HirProtocol {
            name: protocol_decl.name.clone(),
            generic_params: protocol_decl
                .generic_params
                .iter()
                .map(|param| param.node.name.clone())
                .collect(),
            inherited_types: protocol_decl
                .inheritance
                .iter()
                .map(|ty| self.lower_type(ty))
                .collect(),
            functions,
        }
    }

    fn lower_impl_decl(
        &mut self,
        impl_decl: &ImplDecl,
        _fallback_span: crate::frontend::ast::Span,
    ) -> HirImpl {
        let mut functions = Vec::new();

        for member in &impl_decl.members {
            match &member.node {
                ImplMember::Init(init_decl) => {
                    functions.push(
                        self.lower_init_decl(&init_decl.node, init_decl.span),
                    );
                }
                ImplMember::Function(function_decl) => {
                    functions.push(self.lower_function_decl(
                        &function_decl.node,
                        function_decl.span,
                    ))
                }
            }
        }

        HirImpl {
            target: self.lower_type(&impl_decl.target),
            conformance: impl_decl
                .conformance
                .as_ref()
                .map(|ty| self.lower_type(ty)),
            functions,
        }
    }

    fn lower_extern_block(
        &mut self,
        extern_block: &ExternBlock,
        _fallback_span: crate::frontend::ast::Span,
    ) -> HirExtern {
        let functions = extern_block
            .members
            .iter()
            .map(|member| match &member.node {
                ExternMember::Function(function) => {
                    self.lower_extern_function_decl(&function.node)
                }
            })
            .collect();

        HirExtern {
            library_name: extern_block.library_name.clone(),
            functions,
        }
    }

    fn lower_use_item(&mut self, use_item: &UseItem) -> HirUse {
        HirUse {
            tree: self.lower_use_tree(&use_item.tree),
        }
    }

    fn lower_use_tree(&mut self, tree: &Spanned<UseTree>) -> HirUseTree {
        match &tree.node {
            UseTree::Path { path } => HirUseTree::Path {
                path: self.lower_use_path(path),
            },
            UseTree::Glob { path } => HirUseTree::Glob {
                path: self.lower_use_path(path),
            },
            UseTree::Alias { path, alias } => HirUseTree::Alias {
                path: self.lower_use_path(path),
                alias: alias.clone(),
            },
            UseTree::Group { path, items } => HirUseTree::Group {
                path: path.as_ref().map(|path| self.lower_use_path(path)),
                items: items
                    .iter()
                    .map(|item| self.lower_use_tree(item))
                    .collect(),
            },
            UseTree::SelfImport => HirUseTree::SelfImport,
            UseTree::SelfAlias { alias } => HirUseTree::SelfAlias {
                alias: alias.clone(),
            },
        }
    }

    fn lower_use_path(&self, path: &UsePath) -> HirPath {
        HirPath {
            segments: path.segments.clone(),
        }
    }

    fn lower_function_decl(
        &mut self,
        function: &FunctionDecl,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirFunction {
        HirFunction {
            name: function.name.clone(),
            init_origin: function.init_origin.map(lower_init_origin_kind),
            signature: self.lower_function_signature(
                &function.generic_params,
                &function.params,
                function.return_type.as_ref(),
            ),
            body: self.lower_block(&function.body, fallback_span),
        }
    }

    fn lower_init_decl(
        &mut self,
        init_decl: &InitDecl,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirFunction {
        HirFunction {
            name: "init".to_string(),
            init_origin: Some(lower_init_kind(init_decl.kind)),
            signature: self.lower_function_signature(
                &[],
                &init_decl.params,
                None,
            ),
            body: self.lower_block(&init_decl.body, fallback_span),
        }
    }

    fn lower_protocol_function_member(
        &mut self,
        function_member: &ProtocolFunctionMember,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirProtocolFunction {
        HirProtocolFunction {
            name: function_member.name.clone(),
            init_origin: function_member
                .init_origin
                .map(lower_init_origin_kind),
            signature: self.lower_function_signature(
                &function_member.generic_params,
                &function_member.params,
                function_member.return_type.as_ref(),
            ),
            default_body: function_member
                .default_body
                .as_ref()
                .map(|body| self.lower_block(body, fallback_span)),
        }
    }

    fn lower_protocol_init_member(
        &mut self,
        init_member: &ProtocolInitMember,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirProtocolFunction {
        HirProtocolFunction {
            name: "init".to_string(),
            init_origin: Some(lower_init_kind(init_member.kind)),
            signature: self.lower_function_signature(
                &[],
                &init_member.params,
                None,
            ),
            default_body: init_member
                .default_body
                .as_ref()
                .map(|body| self.lower_block(body, fallback_span)),
        }
    }

    fn lower_extern_function_decl(
        &mut self,
        function: &ExternFunctionDecl,
    ) -> HirExternFunction {
        HirExternFunction {
            local_name: function.local_name.clone(),
            native_symbol: function.native_symbol.clone(),
            signature: self.lower_function_signature(
                &[],
                &function.params,
                function.return_type.as_ref(),
            ),
        }
    }

    fn lower_function_signature(
        &mut self,
        generic_params: &[Spanned<crate::frontend::ast::GenericParam>],
        params: &[Spanned<ParamDecl>],
        return_type: Option<&Spanned<Type>>,
    ) -> HirFunctionSignature {
        HirFunctionSignature {
            generic_params: generic_params
                .iter()
                .map(|param| param.node.name.clone())
                .collect(),
            params: params
                .iter()
                .map(|param| HirFunctionParam {
                    external_label: lower_param_label(&param.node.label),
                    name: param.node.name.clone(),
                    ty: self.lower_type(&param.node.ty),
                })
                .collect(),
            return_type: return_type.map(|ty| self.lower_type(ty)),
        }
    }

    fn lower_block(
        &mut self,
        block: &Block,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirBodyId {
        let span = block_span(block, fallback_span);
        let stmts = block
            .statements
            .iter()
            .map(|stmt| self.lower_stmt(stmt))
            .collect();
        let tail_expr =
            block.tail_expr.as_ref().map(|expr| self.lower_expr(expr));

        self.module.alloc_body(HirBody::new(
            self.origin(span),
            stmts,
            tail_expr,
        ))
    }

    fn lower_stmt(&mut self, stmt: &Spanned<Stmt>) -> HirStmtId {
        let kind = match &stmt.node {
            Stmt::Let(let_stmt) => HirStmtKind::Let(
                self.lower_let_stmt(&let_stmt.node, HirMutability::Immutable),
            ),
            Stmt::Var(var_stmt) => HirStmtKind::Let(
                self.lower_var_stmt(&var_stmt.node, HirMutability::Mutable),
            ),
            Stmt::Expr { expr, has_semi } => {
                let expr_id = self.lower_expr(expr);
                if *has_semi {
                    HirStmtKind::Semi { expr: expr_id }
                } else {
                    HirStmtKind::Expr { expr: expr_id }
                }
            }
            Stmt::If(if_stmt) => HirStmtKind::Expr {
                expr: self.lower_if_stmt_expr(&if_stmt.node, if_stmt.span),
            },
            Stmt::Guard(guard_stmt) => HirStmtKind::Expr {
                expr: self
                    .lower_guard_stmt_expr(&guard_stmt.node, guard_stmt.span),
            },
            Stmt::While(while_stmt) => HirStmtKind::Expr {
                expr: self
                    .lower_while_stmt_expr(&while_stmt.node, while_stmt.span),
            },
            Stmt::For(for_stmt) => HirStmtKind::Expr {
                expr: self.lower_for_stmt_expr(&for_stmt.node, for_stmt.span),
            },
            Stmt::Return(value) => HirStmtKind::Semi {
                expr: {
                    let return_value =
                        value.as_ref().map(|expr| self.lower_expr(expr));
                    self.alloc_expr(
                        stmt.span,
                        HirExprKind::Return {
                            value: return_value,
                        },
                    )
                },
            },
            Stmt::Break => HirStmtKind::Semi {
                expr: self.alloc_expr(stmt.span, HirExprKind::Break),
            },
            Stmt::Continue => HirStmtKind::Semi {
                expr: self.alloc_expr(stmt.span, HirExprKind::Continue),
            },
        };

        self.module
            .alloc_stmt(HirStmt::new(self.origin(stmt.span), kind))
    }

    fn lower_let_stmt(
        &mut self,
        let_stmt: &LetStmt,
        mutability: HirMutability,
    ) -> HirLetStmt {
        HirLetStmt {
            pat: self.lower_pat(&let_stmt.pattern),
            ty: let_stmt.ty.as_ref().map(|ty| self.lower_type(ty)),
            value: let_stmt.value.as_ref().map(|value| self.lower_expr(value)),
            mutability,
        }
    }

    fn lower_var_stmt(
        &mut self,
        var_stmt: &VarStmt,
        mutability: HirMutability,
    ) -> HirLetStmt {
        HirLetStmt {
            pat: self.lower_pat(&var_stmt.pattern),
            ty: var_stmt.ty.as_ref().map(|ty| self.lower_type(ty)),
            value: var_stmt.value.as_ref().map(|value| self.lower_expr(value)),
            mutability,
        }
    }

    fn lower_if_stmt_expr(
        &mut self,
        if_stmt: &IfStmt,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirExprId {
        let condition =
            self.lower_clause_list_condition(&if_stmt.clauses, fallback_span);
        let then_body = self.lower_block(&if_stmt.then_branch, fallback_span);
        let else_expr =
            if_stmt
                .else_branch
                .as_ref()
                .map(|else_branch| match else_branch {
                    IfStmtElse::If(else_if) => {
                        self.lower_if_stmt_expr(&else_if.node, else_if.span)
                    }
                    IfStmtElse::Block(block) => {
                        let body = self.lower_block(block, fallback_span);
                        self.alloc_expr(
                            fallback_span,
                            HirExprKind::Block { body },
                        )
                    }
                });

        self.alloc_expr(
            fallback_span,
            HirExprKind::If {
                condition,
                then_body,
                else_expr,
            },
        )
    }

    fn lower_guard_stmt_expr(
        &mut self,
        guard_stmt: &GuardStmt,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirExprId {
        let condition = self
            .lower_clause_list_condition(&guard_stmt.clauses, fallback_span);
        let then_body = self.module.alloc_body(HirBody::new(
            self.origin(fallback_span),
            Vec::new(),
            None,
        ));
        let else_body = self.lower_block(&guard_stmt.else_block, fallback_span);
        let else_expr = self
            .alloc_expr(fallback_span, HirExprKind::Block { body: else_body });

        self.alloc_expr(
            fallback_span,
            HirExprKind::If {
                condition,
                then_body,
                else_expr: Some(else_expr),
            },
        )
    }

    fn lower_while_stmt_expr(
        &mut self,
        while_stmt: &WhileStmt,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirExprId {
        let condition = self
            .lower_clause_list_condition(&while_stmt.clauses, fallback_span);
        let body = self.lower_block(&while_stmt.body, fallback_span);
        self.alloc_expr(fallback_span, HirExprKind::While { condition, body })
    }

    fn lower_for_stmt_expr(
        &mut self,
        for_stmt: &ForStmt,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirExprId {
        let pat = self.lower_pat(&for_stmt.pattern);
        let iterator = self.lower_expr(&for_stmt.iterator);
        let body = self.lower_block(&for_stmt.body, fallback_span);
        self.alloc_expr(
            fallback_span,
            HirExprKind::For {
                pat,
                iterator,
                body,
            },
        )
    }

    fn lower_clause_list_condition(
        &mut self,
        clauses: &ClauseList,
        fallback_span: crate::frontend::ast::Span,
    ) -> HirExprId {
        let mut clause_exprs = clauses
            .clauses
            .iter()
            .map(|clause| match &clause.node {
                Clause::Expr(expr) => self.lower_expr(expr),
                Clause::LetBinding(binding) | Clause::VarBinding(binding) => {
                    self.lower_expr(&binding.value)
                }
            })
            .collect::<Vec<_>>();

        if clause_exprs.is_empty() {
            return self.alloc_expr(
                fallback_span,
                HirExprKind::Literal(HirLiteral::Boolean(true)),
            );
        }

        let mut current = clause_exprs.remove(0);
        for rhs in clause_exprs {
            current = self.alloc_expr(
                fallback_span,
                HirExprKind::Binary {
                    op: HirBinaryOp::LogicalAnd,
                    lhs: current,
                    rhs,
                },
            );
        }
        current
    }

    fn lower_expr(&mut self, expr: &Spanned<Expr>) -> HirExprId {
        let kind = match &expr.node {
            Expr::IntegerLiteral(value) => {
                HirExprKind::Literal(HirLiteral::Integer(value.clone()))
            }
            Expr::FloatLiteral(value) => {
                HirExprKind::Literal(HirLiteral::Float(value.clone()))
            }
            Expr::CharLiteral(value) => {
                HirExprKind::Literal(HirLiteral::Char(value.clone()))
            }
            Expr::BooleanLiteral(value) => {
                HirExprKind::Literal(HirLiteral::Boolean(*value))
            }
            Expr::StringLiteral(value) => {
                let text = value
                    .parts
                    .iter()
                    .filter_map(|part| match part {
                        crate::frontend::ast::StringPart::Text(text) => {
                            Some(text.as_str())
                        }
                        crate::frontend::ast::StringPart::Interpolation(_) => {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                HirExprKind::Literal(HirLiteral::String(text))
            }
            Expr::Identifier(name) => HirExprKind::Path(HirPath {
                segments: vec![name.clone()],
            }),
            Expr::SelfValue => HirExprKind::Path(HirPath {
                segments: vec!["self".to_string()],
            }),
            Expr::SelfType => HirExprKind::Path(HirPath {
                segments: vec!["Self".to_string()],
            }),
            Expr::ShorthandMember { name } => HirExprKind::Path(HirPath {
                segments: vec![name.clone()],
            }),
            Expr::QualifiedMember { qualifier, member } => HirExprKind::Field {
                base: self.lower_expr(qualifier),
                name: member.clone(),
            },
            Expr::Grouped(inner) => return self.lower_expr(inner),
            Expr::ArrayLiteral(elements) => HirExprKind::Array {
                elements: elements
                    .iter()
                    .map(|element| match element {
                        ArrayElement::Expr(expr) => {
                            HirArrayElement::Expr(self.lower_expr(expr))
                        }
                        ArrayElement::Spread(expr) => {
                            HirArrayElement::Spread(self.lower_expr(expr))
                        }
                    })
                    .collect(),
            },
            Expr::StructLiteral { ty, fields } => {
                let ty_id = self.lower_type_expr(ty, expr.span);
                let fields = fields
                    .iter()
                    .filter_map(|field| {
                        match field {
                        crate::frontend::ast::StructLiteralField::Named {
                            name,
                            value,
                        } => Some(HirStructExprField::Named {
                            name: name.clone(),
                            value: self.lower_expr(value),
                        }),
                        crate::frontend::ast::StructLiteralField::Shorthand {
                            name,
                        } => {
                            let value = self.alloc_expr(
                                expr.span,
                                HirExprKind::Path(HirPath {
                                    segments: vec![name.clone()],
                                }),
                            );
                            Some(HirStructExprField::Named {
                                name: name.clone(),
                                value,
                            })
                        }
                        crate::frontend::ast::StructLiteralField::Spread {
                            value,
                        } => Some(HirStructExprField::Spread {
                            value: self.lower_expr(value),
                        }),
                    }
                    })
                    .collect();
                HirExprKind::Struct { ty: ty_id, fields }
            }
            Expr::Block(block) | Expr::UnsafeBlock(block) => {
                HirExprKind::Block {
                    body: self.lower_block(block, expr.span),
                }
            }
            Expr::If {
                clauses,
                then_branch,
                else_branch,
            } => HirExprKind::If {
                condition: self.lower_clause_list_condition(clauses, expr.span),
                then_body: self.lower_block(then_branch, expr.span),
                else_expr: else_branch
                    .as_ref()
                    .map(|else_expr| self.lower_expr(else_expr)),
            },
            Expr::Match { subject, arms } => HirExprKind::Match {
                subject: self.lower_expr(subject),
                arms: arms
                    .iter()
                    .map(|arm| HirMatchArm {
                        pat: self.lower_pat(&arm.node.pattern),
                        expr: match &arm.node.body {
                            crate::frontend::ast::MatchArmBody::Expr(expr) => {
                                self.lower_expr(expr)
                            }
                            crate::frontend::ast::MatchArmBody::Block(
                                block,
                            ) => {
                                let body = self.lower_block(block, arm.span);
                                self.alloc_expr(
                                    arm.span,
                                    HirExprKind::Block { body },
                                )
                            }
                        },
                    })
                    .collect(),
            },
            Expr::Closure {
                params,
                body,
                uses_shorthand_params,
                is_unsafe,
            } => HirExprKind::Closure {
                params: params
                    .iter()
                    .map(|param| HirClosureParam {
                        name: param.name.clone(),
                        ty: param.ty.as_ref().map(|ty| self.lower_type(ty)),
                    })
                    .collect(),
                body: self.lower_block(body, expr.span),
                uses_shorthand_params: *uses_shorthand_params,
                is_unsafe: *is_unsafe,
            },
            Expr::Macro { name, args } => {
                let callee = self.alloc_expr(
                    expr.span,
                    HirExprKind::Path(HirPath {
                        segments: vec![name.clone()],
                    }),
                );
                let args = match args {
                    crate::frontend::ast::MacroExprArgs::Paren(args) => args
                        .iter()
                        .map(|arg| self.lower_call_arg(arg))
                        .collect(),
                    crate::frontend::ast::MacroExprArgs::Braced(block) => {
                        let body = self.module.alloc_body(HirBody::new(
                            self.origin(block.span),
                            Vec::new(),
                            None,
                        ));
                        vec![HirCallArg {
                            label: None,
                            value: self.alloc_expr(
                                block.span,
                                HirExprKind::Block { body },
                            ),
                        }]
                    }
                };
                HirExprKind::Call { callee, args }
            }
            Expr::Unary { op, expr: inner } => HirExprKind::Unary {
                op: match op {
                    UnaryOp::Negate => HirUnaryOp::Negate,
                    UnaryOp::Not => HirUnaryOp::Not,
                },
                expr: self.lower_expr(inner),
            },
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                let then_expr_id = self.lower_expr(then_expr);
                let then_body = self.module.alloc_body(HirBody::new(
                    self.origin(then_expr.span),
                    Vec::new(),
                    Some(then_expr_id),
                ));
                HirExprKind::If {
                    condition: self.lower_expr(condition),
                    then_body,
                    else_expr: Some(self.lower_expr(else_expr)),
                }
            }
            Expr::Binary { op, lhs, rhs } => HirExprKind::Binary {
                op: lower_binary_op(*op),
                lhs: self.lower_expr(lhs),
                rhs: self.lower_expr(rhs),
            },
            Expr::Assignment { op, target, value } => HirExprKind::Assign {
                op: lower_assign_op(*op),
                target: self.lower_expr(target),
                value: self.lower_expr(value),
            },
            Expr::MemberAccess { base, member } => HirExprKind::Field {
                base: self.lower_expr(base),
                name: member.clone(),
            },
            Expr::OptionalMemberAccess { base, member } => {
                HirExprKind::OptionalField {
                    base: self.lower_expr(base),
                    name: member.clone(),
                }
            }
            Expr::NamespaceAccess {
                base,
                member,
                turbofish,
            } => HirExprKind::NamespaceField {
                base: self.lower_expr(base),
                name: member.clone(),
                turbofish: turbofish
                    .iter()
                    .map(|ty| self.lower_type(ty))
                    .collect(),
            },
            Expr::Call {
                callee,
                args,
                trailing_closure,
            } => {
                let mut lowered_args: Vec<HirCallArg> =
                    args.iter().map(|arg| self.lower_call_arg(arg)).collect();
                if let Some(closure) = trailing_closure {
                    lowered_args.push(HirCallArg {
                        label: None,
                        value: self.lower_expr(closure),
                    });
                }
                HirExprKind::Call {
                    callee: self.lower_expr(callee),
                    args: lowered_args,
                }
            }
            // Method calls are preserved as HIR MethodCall for type checking
            // to transform based on receiver kind (self, &self, &mut self)
            Expr::MethodCall {
                receiver,
                method_name,
                args,
                trailing_closure,
            } => {
                let receiver_id = self.lower_expr(receiver);
                let mut lowered_args: Vec<HirCallArg> =
                    args.iter().map(|arg| self.lower_call_arg(arg)).collect();

                // Handle trailing closure
                if let Some(closure) = trailing_closure {
                    lowered_args.push(HirCallArg {
                        label: None,
                        value: self.lower_expr(closure),
                    });
                }

                HirExprKind::MethodCall {
                    receiver: receiver_id,
                    method_name: method_name.clone(),
                    args: lowered_args,
                }
            }
            Expr::Index { base, index } => HirExprKind::Index {
                base: self.lower_expr(base),
                index: self.lower_expr(index),
            },
            Expr::OptionalIndex { base, index } => HirExprKind::OptionalIndex {
                base: self.lower_expr(base),
                index: self.lower_expr(index),
            },
            Expr::ForceUnwrap { expr: inner } => HirExprKind::ForceUnwrap {
                expr: self.lower_expr(inner),
            },
            Expr::Cast {
                expr: inner,
                ty,
                is_optional,
            } => HirExprKind::Cast {
                expr: self.lower_expr(inner),
                ty: self.lower_type(ty),
                is_optional: *is_optional,
            },
            Expr::Range {
                start,
                end,
                inclusive,
            } => HirExprKind::Range {
                start: start.as_ref().map(|start| self.lower_expr(start)),
                end: end.as_ref().map(|end| self.lower_expr(end)),
                inclusive: *inclusive,
            },
            Expr::Spread { expr: inner } => HirExprKind::Spread {
                expr: self.lower_expr(inner),
            },
            Expr::Try { expr: inner } => HirExprKind::Try {
                expr: self.lower_expr(inner),
            },
            Expr::ConstructorCall { .. } => {
                // ConstructorCall should be desugared to NamespaceAccess + Call
                // before reaching HIR lowering. This is a bug if we reach here.
                panic!("ConstructorCall should not reach HIR lowering - it should be desugared to explicit NamespaceAccess + Call")
            }
        };

        self.alloc_expr(expr.span, kind)
    }

    fn lower_call_arg(&mut self, arg: &CallArg) -> HirCallArg {
        HirCallArg {
            label: arg.label.clone(),
            value: self.lower_expr(&arg.value),
        }
    }

    fn lower_type_expr(
        &mut self,
        type_expr: &TypeExpr,
        span: crate::frontend::ast::Span,
    ) -> HirTypeId {
        let kind = match type_expr {
            TypeExpr::Path(segments) => HirTypeKind::Path(HirPath {
                segments: segments.clone(),
            }),
            TypeExpr::SelfType => HirTypeKind::SelfType,
        };
        self.module
            .alloc_type(HirType::new(self.origin(span), kind))
    }

    fn lower_type(&mut self, ty: &Spanned<Type>) -> HirTypeId {
        let kind = match &ty.node {
            Type::Named { segments } => HirTypeKind::Path(HirPath {
                segments: segments.clone(),
            }),
            Type::GenericApplication { base, args } => {
                HirTypeKind::GenericApplication {
                    base: self.lower_type(base),
                    args: args.iter().map(|arg| self.lower_type(arg)).collect(),
                }
            }
            Type::SelfType => HirTypeKind::SelfType,
            Type::Reference(inner) => HirTypeKind::Reference {
                mutable: false,
                inner: self.lower_type(inner),
            },
            Type::MutableReference(inner) => HirTypeKind::Reference {
                mutable: true,
                inner: self.lower_type(inner),
            },
            Type::ConstPointer(inner) => HirTypeKind::Pointer {
                mutable: false,
                inner: self.lower_type(inner),
            },
            Type::MutablePointer(inner) => HirTypeKind::Pointer {
                mutable: true,
                inner: self.lower_type(inner),
            },
            Type::Array(inner) => {
                let base = self.module.alloc_type(HirType::new(
                    self.origin(ty.span),
                    HirTypeKind::Path(HirPath {
                        segments: vec!["Array".to_string()],
                    }),
                ));
                HirTypeKind::GenericApplication {
                    base,
                    args: vec![self.lower_type(inner)],
                }
            }
            Type::Optional(inner) => HirTypeKind::Optional {
                inner: self.lower_type(inner),
            },
            Type::Result { ok, err } => HirTypeKind::Result {
                ok: self.lower_type(ok),
                err: self.lower_type(err),
            },
            Type::Grouped(inner) => return self.lower_type(inner),
        };

        self.module
            .alloc_type(HirType::new(self.origin(ty.span), kind))
    }

    fn lower_pat(&mut self, pattern: &Spanned<Pattern>) -> HirPatId {
        let kind = match &pattern.node {
            Pattern::Identifier(name) => {
                HirPatKind::Binding { name: name.clone() }
            }
            Pattern::Wildcard => HirPatKind::Wildcard,
            Pattern::IntegerLiteral(value) => {
                HirPatKind::Literal(HirLiteral::Integer(value.clone()))
            }
            Pattern::BooleanLiteral(value) => {
                HirPatKind::Literal(HirLiteral::Boolean(*value))
            }
            Pattern::CharLiteral(value) => {
                HirPatKind::Literal(HirLiteral::Char(value.clone()))
            }
            Pattern::StringLiteral(value) => {
                HirPatKind::Literal(HirLiteral::String(value.clone()))
            }
            Pattern::Tuple(elements) => HirPatKind::Tuple {
                elements: elements
                    .iter()
                    .map(|element| self.lower_pat(element))
                    .collect(),
            },
            Pattern::Variant {
                path,
                shorthand,
                args,
                has_rest,
            } => HirPatKind::EnumVariant {
                path: HirPath {
                    segments: path.clone(),
                },
                shorthand: *shorthand,
                args: args.iter().map(|arg| self.lower_pat(arg)).collect(),
                has_rest: *has_rest,
            },
            Pattern::Struct {
                path,
                fields,
                has_rest,
            } => HirPatKind::Struct {
                path: HirPath {
                    segments: path.clone(),
                },
                fields: fields
                    .iter()
                    .map(|field| self.lower_struct_pat_field(field))
                    .collect(),
                has_rest: *has_rest,
            },
            // Array patterns are canonicalized to tuple-pattern shape.
            Pattern::Array { elements, rest } => {
                let mut lowered: Vec<HirPatId> = elements
                    .iter()
                    .map(|element| self.lower_pat(element))
                    .collect();
                if let Some(rest) = rest {
                    let rest_pat = match rest {
                        crate::frontend::ast::ArrayPatternRest::Ignore => {
                            HirPatKind::Wildcard
                        }
                        crate::frontend::ast::ArrayPatternRest::Bind(name) => {
                            HirPatKind::Binding { name: name.clone() }
                        }
                    };
                    lowered.push(self.module.alloc_pat(HirPat::new(
                        self.origin(pattern.span),
                        rest_pat,
                    )));
                }
                HirPatKind::Tuple { elements: lowered }
            }
        };

        self.module
            .alloc_pat(HirPat::new(self.origin(pattern.span), kind))
    }

    fn lower_struct_pat_field(
        &mut self,
        field: &StructPatternField,
    ) -> HirStructPatField {
        HirStructPatField {
            name: field.name.clone(),
            pat: field
                .pattern
                .as_ref()
                .map(|pattern| self.lower_pat(pattern)),
        }
    }

    fn alloc_item(
        &mut self,
        span: crate::frontend::ast::Span,
        kind: HirItemKind,
    ) -> HirItemId {
        self.module
            .alloc_item(HirItem::new(self.origin(span), kind))
    }

    fn alloc_expr(
        &mut self,
        span: crate::frontend::ast::Span,
        kind: HirExprKind,
    ) -> HirExprId {
        self.module
            .alloc_expr(HirExpr::new(self.origin(span), kind))
    }

    fn origin(&self, span: crate::frontend::ast::Span) -> HirOrigin {
        let provenance =
            self.desugared.provenance_map.get(span).cloned().unwrap_or(
                Provenance::DirectSource {
                    file_id: self.desugared.file_id,
                    span,
                },
            );
        HirOrigin::new(self.desugared.file_id, span, provenance)
    }
}

fn lower_param_label(label: &ParamLabel) -> super::HirParamLabel {
    match label {
        ParamLabel::None => super::HirParamLabel::None,
        ParamLabel::Explicit(label) => super::HirParamLabel::Explicit(label.clone()),
        ParamLabel::FromName => super::HirParamLabel::FromName,
    }
}

const fn lower_init_origin_kind(kind: InitOriginKind) -> HirInitOrigin {
    match kind {
        InitOriginKind::Plain => HirInitOrigin::Plain,
        InitOriginKind::Optional => HirInitOrigin::Optional,
        InitOriginKind::Fallible => HirInitOrigin::Fallible,
    }
}

const fn lower_init_kind(kind: InitKind) -> HirInitOrigin {
    match kind {
        InitKind::Plain => HirInitOrigin::Plain,
        InitKind::Optional => HirInitOrigin::Optional,
        InitKind::Fallible => HirInitOrigin::Fallible,
    }
}

const fn lower_binary_op(op: BinaryOp) -> HirBinaryOp {
    match op {
        BinaryOp::LogicalOr => HirBinaryOp::LogicalOr,
        BinaryOp::LogicalAnd => HirBinaryOp::LogicalAnd,
        BinaryOp::NullCoalescing => HirBinaryOp::NullCoalescing,
        BinaryOp::BitOr => HirBinaryOp::BitOr,
        BinaryOp::BitXor => HirBinaryOp::BitXor,
        BinaryOp::BitAnd => HirBinaryOp::BitAnd,
        BinaryOp::Equal => HirBinaryOp::Equal,
        BinaryOp::NotEqual => HirBinaryOp::NotEqual,
        BinaryOp::Less => HirBinaryOp::Less,
        BinaryOp::LessEqual => HirBinaryOp::LessEqual,
        BinaryOp::Greater => HirBinaryOp::Greater,
        BinaryOp::GreaterEqual => HirBinaryOp::GreaterEqual,
        BinaryOp::ShiftLeft => HirBinaryOp::ShiftLeft,
        BinaryOp::ShiftRight => HirBinaryOp::ShiftRight,
        BinaryOp::Add => HirBinaryOp::Add,
        BinaryOp::Subtract => HirBinaryOp::Subtract,
        BinaryOp::Multiply => HirBinaryOp::Multiply,
        BinaryOp::Divide => HirBinaryOp::Divide,
        BinaryOp::Remainder => HirBinaryOp::Remainder,
    }
}

const fn lower_assign_op(op: AssignOp) -> HirAssignOp {
    match op {
        AssignOp::Assign => HirAssignOp::Assign,
        AssignOp::AddAssign => HirAssignOp::AddAssign,
        AssignOp::SubAssign => HirAssignOp::SubAssign,
        AssignOp::MulAssign => HirAssignOp::MulAssign,
        AssignOp::DivAssign => HirAssignOp::DivAssign,
        AssignOp::RemAssign => HirAssignOp::RemAssign,
        AssignOp::BitXorAssign => HirAssignOp::BitXorAssign,
        AssignOp::BitOrAssign => HirAssignOp::BitOrAssign,
        AssignOp::BitAndAssign => HirAssignOp::BitAndAssign,
        AssignOp::ShlAssign => HirAssignOp::ShlAssign,
        AssignOp::ShrAssign => HirAssignOp::ShrAssign,
    }
}

fn block_span(
    block: &Block,
    fallback_span: crate::frontend::ast::Span,
) -> crate::frontend::ast::Span {
    let mut start = fallback_span.start;
    let mut end = fallback_span.end;
    let mut found = false;

    for stmt in &block.statements {
        if !found {
            start = stmt.span.start;
            end = stmt.span.end;
            found = true;
        } else {
            if stmt.span.start < start {
                start = stmt.span.start;
            }
            if stmt.span.end > end {
                end = stmt.span.end;
            }
        }
    }

    if let Some(tail_expr) = &block.tail_expr {
        if !found {
            start = tail_expr.span.start;
            end = tail_expr.span.end;
            found = true;
        } else {
            if tail_expr.span.start < start {
                start = tail_expr.span.start;
            }
            if tail_expr.span.end > end {
                end = tail_expr.span.end;
            }
        }
    }

    if found {
        crate::frontend::ast::Span::new(start, end)
    } else {
        fallback_span
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::ast::{
        Expr, File, FunctionDecl, Item, MacroExprArgs, Spanned, Stmt,
    };
    use crate::frontend::diagnostics::DiagnosticsBag;
    use crate::frontend::expansion::{ProvenanceMap, SynthesisPurpose};
    use crate::frontend::source::FileId;

    fn desugared_from_file(
        file_id: FileId,
        file: File,
        provenance_entries: Vec<(crate::frontend::ast::Span, Provenance)>,
    ) -> DesugaredFile {
        let mut provenance_map = ProvenanceMap::new(file_id);
        for (span, provenance) in provenance_entries {
            provenance_map.insert(span, provenance);
        }

        DesugaredFile {
            file_id,
            ast: file,
            diagnostics: DiagnosticsBag::default(),
            provenance_map,
        }
    }

    fn empty_function_decl(body: Block) -> FunctionDecl {
        FunctionDecl {
            docs: Vec::new(),
            attributes: Vec::new(),
            visibility: None,
            modifiers: Vec::new(),
            name: "demo".to_string(),
            generic_params: Vec::new(),
            receiver: None,
            params: Vec::new(),
            return_type: None,
            where_clause: None,
            init_origin: None,
            body,
        }
    }

    #[test]
    fn lowering_eliminates_grouped_and_macro_forms() {
        let file_id = FileId::new(1);
        let fn_span = crate::frontend::ast::Span::new(0, 100);
        let stmt_span = crate::frontend::ast::Span::new(18, 30);
        let grouped_span = crate::frontend::ast::Span::new(19, 29);
        let macro_span = crate::frontend::ast::Span::new(20, 28);
        let lit_span = crate::frontend::ast::Span::new(23, 24);

        let macro_expr = Spanned::new(
            Expr::Macro {
                name: "m".to_string(),
                args: MacroExprArgs::Paren(vec![
                    crate::frontend::ast::CallArg {
                        label: None,
                        value: Box::new(Spanned::new(
                            Expr::IntegerLiteral("1".to_string()),
                            lit_span,
                        )),
                    },
                ]),
            },
            macro_span,
        );
        let grouped_expr =
            Spanned::new(Expr::Grouped(Box::new(macro_expr)), grouped_span);

        let stmt = Spanned::new(
            Stmt::Expr {
                expr: Box::new(grouped_expr),
                has_semi: true,
            },
            stmt_span,
        );

        let function_decl = empty_function_decl(Block {
            statements: vec![stmt],
            tail_expr: None,
        });

        let file = File {
            items: vec![Spanned::new(
                Item::Function(Spanned::new(function_decl, fn_span)),
                fn_span,
            )],
        };
        let desugared = desugared_from_file(file_id, file, Vec::new());

        let (_hir_file, hir_module) = lower_to_hir(&desugared);

        let has_macro_as_call = hir_module.exprs.values().any(|expr| {
            let HirExprKind::Call { callee, .. } = &expr.kind else {
                return false;
            };
            matches!(
                hir_module.exprs.get(callee),
                Some(HirExpr {
                    kind: HirExprKind::Path(HirPath { segments }),
                    ..
                }) if segments == &vec!["m".to_string()]
            )
        });

        assert!(
            has_macro_as_call,
            "macro expression should be canonicalized to call/path form"
        );
        assert!(
            !hir_module
                .exprs
                .values()
                .any(|expr| expr.origin.span == grouped_span),
            "grouped wrapper span should not appear as a dedicated HIR expression"
        );
    }

    #[test]
    fn lowering_is_deterministic() {
        let file_id = FileId::new(2);
        let fn_span = crate::frontend::ast::Span::new(0, 80);
        let stmt_span = crate::frontend::ast::Span::new(10, 30);
        let lit_span = crate::frontend::ast::Span::new(20, 21);

        let stmt = Spanned::new(
            Stmt::Expr {
                expr: Box::new(Spanned::new(
                    Expr::IntegerLiteral("7".to_string()),
                    lit_span,
                )),
                has_semi: false,
            },
            stmt_span,
        );

        let file = File {
            items: vec![Spanned::new(
                Item::Function(Spanned::new(
                    empty_function_decl(Block {
                        statements: vec![stmt],
                        tail_expr: None,
                    }),
                    fn_span,
                )),
                fn_span,
            )],
        };
        let desugared = desugared_from_file(file_id, file, Vec::new());

        let (file_a, module_a) = lower_to_hir(&desugared);
        let (file_b, module_b) = lower_to_hir(&desugared);

        assert_eq!(file_a, file_b);
        assert_eq!(module_a, module_b);
    }

    #[test]
    fn lowering_preserves_origin_for_key_nodes() {
        let file_id = FileId::new(3);
        let fn_span = crate::frontend::ast::Span::new(0, 60);
        let stmt_span = crate::frontend::ast::Span::new(10, 26);
        let lit_span = crate::frontend::ast::Span::new(24, 25);

        let function_provenance = Provenance::ExpandedFrom {
            macro_name: "trace".to_string(),
            call_site_file: file_id,
            call_site_span: crate::frontend::ast::Span::new(0, 5),
            definition_span: Some(crate::frontend::ast::Span::new(100, 110)),
        };
        let body_provenance = Provenance::SyntheticFor {
            purpose: SynthesisPurpose::Desugar,
            related_span: Some((file_id, stmt_span)),
        };
        let literal_provenance = Provenance::ExpandedFrom {
            macro_name: "lit".to_string(),
            call_site_file: file_id,
            call_site_span: crate::frontend::ast::Span::new(20, 25),
            definition_span: None,
        };

        let stmt = Spanned::new(
            Stmt::Expr {
                expr: Box::new(Spanned::new(
                    Expr::IntegerLiteral("9".to_string()),
                    lit_span,
                )),
                has_semi: true,
            },
            stmt_span,
        );

        let file = File {
            items: vec![Spanned::new(
                Item::Function(Spanned::new(
                    empty_function_decl(Block {
                        statements: vec![stmt],
                        tail_expr: None,
                    }),
                    fn_span,
                )),
                fn_span,
            )],
        };

        let desugared = desugared_from_file(
            file_id,
            file,
            vec![
                (fn_span, function_provenance.clone()),
                (stmt_span, body_provenance.clone()),
                (lit_span, literal_provenance.clone()),
            ],
        );

        let (hir_file, hir_module) = lower_to_hir(&desugared);
        let root_item_id = hir_file.root_items[0];
        let root_item = hir_module
            .items
            .get(&root_item_id)
            .expect("root item should exist");
        assert_eq!(root_item.origin.provenance, function_provenance);

        let body_id = match &root_item.kind {
            HirItemKind::Function(function) => function.body,
            _ => panic!("expected function root item"),
        };
        let body = hir_module
            .bodies
            .get(&body_id)
            .expect("function body should exist");
        assert_eq!(body.origin.provenance, body_provenance);

        let literal_expr = hir_module
            .exprs
            .values()
            .find(|expr| expr.origin.span == lit_span)
            .expect("literal expression should exist");
        assert_eq!(literal_expr.origin.provenance, literal_provenance);
    }
}
