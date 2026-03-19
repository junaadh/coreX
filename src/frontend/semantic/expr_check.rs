use super::body_env::BodyTypeEnvironmentTable;
use super::external_lookup::ExternalSemanticLookup;
use super::signatures::TypedFunctionSignature;
use super::{BuiltinType, Type, TypedItemData, TypedItemTable};
use crate::frontend::ExpandedFile;
use crate::frontend::ast::{
    BinaryOp, Block, Clause, Expr, Item, MatchArmBody, Span, Stmt,
    StructMember, UnaryOp,
};
use crate::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ImportBindingKind, ItemId, LocalId,
    LocalMutability, ResolvedBody, ResolvedBodyRef, ResolvedBodyTable,
    ResolvedImports, ScopeGraph,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BodyExprId {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub expr_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprCheckIssueKind {
    MissingBodyAst,
    MissingBodyEnvironment,
    MissingResolvedReference {
        segments: Vec<String>,
    },
    MissingLocalType {
        local_id: LocalId,
    },
    MissingTypedItem {
        item_id: ItemId,
    },
    InvalidUnaryOp,
    InvalidBinaryOp,
    InvalidAssignmentTarget,
    MutabilityViolation {
        local_id: LocalId,
    },
    AssignmentTypeMismatch {
        target: Type,
        value: Type,
    },
    InvalidCallCallee,
    BareExternFunctionCall {
        function: String,
        namespace: String,
    },
    CallArityMismatch {
        expected: usize,
        found: usize,
    },
    CallArgTypeMismatch {
        index: usize,
        expected: Type,
        found: Type,
    },
    MissingElseBranch,
    IncompatibleIfBranches {
        then_type: Type,
        else_type: Type,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprCheckIssue {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub span: Span,
    pub kind: ExprCheckIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionTypeTable {
    types_by_expr_id: BTreeMap<BodyExprId, Type>,
    expr_ids_by_span:
        BTreeMap<(DeclarationOwner, usize, usize, usize), Vec<BodyExprId>>,
    root_types_by_body: BTreeMap<(DeclarationOwner, usize), Type>,
    pub issues: Vec<ExprCheckIssue>,
}

impl ExpressionTypeTable {
    #[must_use]
    pub fn expr_type(&self, expr_id: &BodyExprId) -> Option<&Type> {
        self.types_by_expr_id.get(expr_id)
    }

    #[must_use]
    pub fn expr_type_for_span(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        span: Span,
    ) -> Option<&Type> {
        let key = (owner.clone(), body_index, span.start, span.end);
        let expr_id = self.expr_ids_by_span.get(&key)?.last()?;
        self.types_by_expr_id.get(expr_id)
    }

    #[must_use]
    pub fn root_type(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Option<&Type> {
        self.root_types_by_body.get(&(owner.clone(), body_index))
    }

    #[must_use]
    pub fn expr_ids_for_body(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Vec<BodyExprId> {
        self.types_by_expr_id
            .keys()
            .filter(|id| id.owner == *owner && id.body_index == body_index)
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = (&BodyExprId, &Type)> {
        self.types_by_expr_id.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.types_by_expr_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types_by_expr_id.is_empty()
    }
}

#[must_use]
pub fn check_expression_types(
    graph: &ScopeGraph,
    parsed_files: &[ExpandedFile],
    global_items: &GlobalItemTable,
    typed_items: &TypedItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
) -> ExpressionTypeTable {
    let empty_imports = BTreeMap::new();
    let empty_external_lookup = ExternalSemanticLookup::default();
    check_expression_types_with_external_lookup(
        graph,
        parsed_files,
        global_items,
        typed_items,
        resolved_bodies,
        body_envs,
        &empty_imports,
        &empty_external_lookup,
    )
}

#[must_use]
pub fn check_expression_types_with_external_lookup(
    graph: &ScopeGraph,
    parsed_files: &[ExpandedFile],
    global_items: &GlobalItemTable,
    typed_items: &TypedItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
    imports: &BTreeMap<FileId, ResolvedImports>,
    external_lookup: &ExternalSemanticLookup,
) -> ExpressionTypeTable {
    let parsed_by_id: BTreeMap<FileId, &ExpandedFile> = parsed_files
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect();

    let body_blocks = collect_body_blocks(graph, &parsed_by_id, global_items);

    let mut types_by_expr_id = BTreeMap::new();
    let mut expr_ids_by_span: BTreeMap<
        (DeclarationOwner, usize, usize, usize),
        Vec<BodyExprId>,
    > = BTreeMap::new();
    let mut root_types_by_body = BTreeMap::new();
    let mut issues = Vec::new();

    for body in resolved_bodies.iter() {
        let Some(env) = body_envs
            .envs_for_owner(&body.owner)
            .iter()
            .find(|candidate| candidate.body_index == body.body_index)
        else {
            issues.push(ExprCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: ExprCheckIssueKind::MissingBodyEnvironment,
            });
            continue;
        };

        let Some(block_entry) = body_blocks
            .get(&body.owner)
            .and_then(|entries| entries.get(body.body_index))
        else {
            issues.push(ExprCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: ExprCheckIssueKind::MissingBodyAst,
            });
            continue;
        };

        let mut checker = BodyExprChecker::new(
            body,
            env.local_types.clone(),
            imports,
            external_lookup,
        );
        let root_type = checker.check_block(
            block_entry.block,
            typed_items,
            &mut issues,
            &mut types_by_expr_id,
        );
        for (key, ids) in checker.expr_ids_by_span {
            expr_ids_by_span.entry(key).or_default().extend(ids);
        }
        root_types_by_body
            .insert((body.owner.clone(), body.body_index), root_type);
    }

    ExpressionTypeTable {
        types_by_expr_id,
        expr_ids_by_span,
        root_types_by_body,
        issues,
    }
}

struct BodyBlockEntry<'a> {
    block: &'a Block,
}

fn collect_body_blocks<'a>(
    graph: &'a ScopeGraph,
    parsed_by_id: &'a BTreeMap<FileId, &'a ExpandedFile>,
    global_items: &'a GlobalItemTable,
) -> BTreeMap<DeclarationOwner, Vec<BodyBlockEntry<'a>>> {
    let mut result: BTreeMap<DeclarationOwner, Vec<BodyBlockEntry<'a>>> =
        BTreeMap::new();

    for (scope_file_id, scope) in &graph.scopes {
        let Some(parsed) = parsed_by_id.get(scope_file_id) else {
            continue;
        };

        let mut impl_index = 0usize;
        for item in &parsed.ast.items {
            match &item.node {
                Item::Function(function_decl) => {
                    if let Some(item_id) = item_id_for_top_level(
                        global_items,
                        *scope_file_id,
                        &scope.scope_path,
                        &function_decl.node.name,
                    ) {
                        result
                            .entry(DeclarationOwner::Item(item_id))
                            .or_default()
                            .push(BodyBlockEntry {
                                block: &function_decl.node.body,
                            });
                    }
                }
                Item::Struct(struct_decl) => {
                    if let Some(item_id) = item_id_for_top_level(
                        global_items,
                        *scope_file_id,
                        &scope.scope_path,
                        &struct_decl.node.name,
                    ) {
                        let owner = DeclarationOwner::Item(item_id);
                        for member in &struct_decl.node.members {
                            match &member.node {
                                StructMember::Function(function_decl) => {
                                    result
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(BodyBlockEntry {
                                            block: &function_decl.node.body,
                                        });
                                }
                                StructMember::Init(init_decl) => {
                                    result
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(BodyBlockEntry {
                                            block: &init_decl.node.body,
                                        });
                                }
                                StructMember::Field(_) => {}
                            }
                        }
                    }
                }
                Item::Enum(enum_decl) => {
                    if let Some(item_id) = item_id_for_top_level(
                        global_items,
                        *scope_file_id,
                        &scope.scope_path,
                        &enum_decl.node.name,
                    ) {
                        let owner = DeclarationOwner::Item(item_id);
                        for member in &enum_decl.node.members {
                            match &member.node {
                                crate::frontend::ast::EnumMember::Function(
                                    function_decl,
                                ) => {
                                    result
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(BodyBlockEntry {
                                            block: &function_decl.node.body,
                                        });
                                }
                                crate::frontend::ast::EnumMember::Init(
                                    init_decl,
                                ) => {
                                    result
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(BodyBlockEntry {
                                            block: &init_decl.node.body,
                                        });
                                }
                                crate::frontend::ast::EnumMember::Case(_) => {}
                            }
                        }
                    }
                }
                Item::Protocol(protocol_decl) => {
                    if let Some(item_id) = item_id_for_top_level(
                        global_items,
                        *scope_file_id,
                        &scope.scope_path,
                        &protocol_decl.node.name,
                    ) {
                        let owner = DeclarationOwner::Item(item_id);
                        for member in &protocol_decl.node.members {
                            match &member.node {
                                crate::frontend::ast::ProtocolMember::Function(
                                    function_member,
                                ) => {
                                    if let Some(default_body) =
                                        &function_member.node.default_body
                                    {
                                        result.entry(owner.clone()).or_default().push(
                                            BodyBlockEntry {
                                                block: default_body,
                                            },
                                        );
                                    }
                                }
                                crate::frontend::ast::ProtocolMember::Initializer(
                                    init_member,
                                ) => {
                                    if let Some(default_body) =
                                        &init_member.node.default_body
                                    {
                                        result.entry(owner.clone()).or_default().push(
                                            BodyBlockEntry {
                                                block: default_body,
                                            },
                                        );
                                    }
                                }
                                crate::frontend::ast::ProtocolMember::AssociatedType(_)
                                | crate::frontend::ast::ProtocolMember::Property(_) => {}
                            }
                        }
                    }
                }
                Item::Impl(impl_decl) => {
                    let owner = DeclarationOwner::Impl {
                        scope_file_id: *scope_file_id,
                        impl_index,
                    };
                    impl_index = impl_index.saturating_add(1);
                    for member in &impl_decl.node.members {
                        match &member.node {
                            crate::frontend::ast::ImplMember::Function(
                                function_decl,
                            ) => {
                                result.entry(owner.clone()).or_default().push(
                                    BodyBlockEntry {
                                        block: &function_decl.node.body,
                                    },
                                );
                            }
                            crate::frontend::ast::ImplMember::Init(
                                init_decl,
                            ) => {
                                result.entry(owner.clone()).or_default().push(
                                    BodyBlockEntry {
                                        block: &init_decl.node.body,
                                    },
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    result
}

fn item_id_for_top_level(
    global_items: &GlobalItemTable,
    scope_file_id: FileId,
    scope_path: &[String],
    name: &str,
) -> Option<ItemId> {
    let mut full_path = scope_path.to_vec();
    full_path.push(name.to_string());
    let item_id = global_items.item_id_by_full_path(&full_path)?;
    let item = global_items.get(item_id)?;
    (item.containing_scope_file_id == scope_file_id).then_some(item_id)
}

struct BodyExprChecker<'a> {
    body: &'a ResolvedBody,
    local_types: BTreeMap<LocalId, Type>,
    imports: &'a BTreeMap<FileId, ResolvedImports>,
    external_lookup: &'a ExternalSemanticLookup,
    expr_ids_by_span:
        BTreeMap<(DeclarationOwner, usize, usize, usize), Vec<BodyExprId>>,
    next_expr_index: u32,
}

impl<'a> BodyExprChecker<'a> {
    fn new(
        body: &'a ResolvedBody,
        local_types: BTreeMap<LocalId, Type>,
        imports: &'a BTreeMap<FileId, ResolvedImports>,
        external_lookup: &'a ExternalSemanticLookup,
    ) -> Self {
        Self {
            body,
            local_types,
            imports,
            external_lookup,
            expr_ids_by_span: BTreeMap::new(),
            next_expr_index: 0,
        }
    }

    fn allocate_expr_id(&mut self) -> BodyExprId {
        let expr_index = self.next_expr_index;
        self.next_expr_index = self.next_expr_index.saturating_add(1);
        BodyExprId {
            owner: self.body.owner.clone(),
            body_index: self.body.body_index,
            expr_index,
        }
    }

    fn check_block(
        &mut self,
        block: &Block,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
        types_by_expr_id: &mut BTreeMap<BodyExprId, Type>,
    ) -> Type {
        for statement in &block.statements {
            self.check_stmt(
                &statement.node,
                typed_items,
                issues,
                types_by_expr_id,
            );
        }
        if let Some(tail_expr) = &block.tail_expr {
            self.check_expr(tail_expr, typed_items, issues, types_by_expr_id)
        } else {
            Type::void()
        }
    }

    fn check_stmt(
        &mut self,
        stmt: &Stmt,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
        types_by_expr_id: &mut BTreeMap<BodyExprId, Type>,
    ) {
        match stmt {
            Stmt::Let(let_stmt) => {
                let value_type = let_stmt
                    .node
                    .value
                    .as_ref()
                    .map(|value| {
                        self.check_expr(
                            value,
                            typed_items,
                            issues,
                            types_by_expr_id,
                        )
                    })
                    .unwrap_or_else(Type::void);
                self.infer_local_from_pattern(
                    &let_stmt.node.pattern,
                    value_type,
                    false,
                );
            }
            Stmt::Var(var_stmt) => {
                let value_type = var_stmt
                    .node
                    .value
                    .as_ref()
                    .map(|value| {
                        self.check_expr(
                            value,
                            typed_items,
                            issues,
                            types_by_expr_id,
                        )
                    })
                    .unwrap_or_else(Type::void);
                self.infer_local_from_pattern(
                    &var_stmt.node.pattern,
                    value_type,
                    true,
                );
            }
            Stmt::Expr { expr, .. } => {
                self.check_expr(expr, typed_items, issues, types_by_expr_id);
            }
            Stmt::If(if_stmt) => {
                self.check_clause_list(
                    &if_stmt.node.clauses,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                self.check_block(
                    &if_stmt.node.then_branch,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                if let Some(else_branch) = &if_stmt.node.else_branch {
                    match else_branch {
                        crate::frontend::ast::IfStmtElse::If(nested) => {
                            self.check_stmt(
                                &Stmt::If(*nested.clone()),
                                typed_items,
                                issues,
                                types_by_expr_id,
                            );
                        }
                        crate::frontend::ast::IfStmtElse::Block(block) => {
                            self.check_block(
                                block,
                                typed_items,
                                issues,
                                types_by_expr_id,
                            );
                        }
                    }
                }
            }
            Stmt::Guard(guard_stmt) => {
                self.check_clause_list(
                    &guard_stmt.node.clauses,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                self.check_block(
                    &guard_stmt.node.else_block,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
            }
            Stmt::While(while_stmt) => {
                self.check_clause_list(
                    &while_stmt.node.clauses,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                self.check_block(
                    &while_stmt.node.body,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
            }
            Stmt::For(for_stmt) => {
                self.check_expr(
                    &for_stmt.node.iterator,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                self.check_block(
                    &for_stmt.node.body,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.check_expr(
                        value,
                        typed_items,
                        issues,
                        types_by_expr_id,
                    );
                }
            }
            Stmt::Break | Stmt::Continue => {}
        }
    }

    fn check_expr(
        &mut self,
        expr: &crate::frontend::ast::Spanned<Expr>,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
        types_by_expr_id: &mut BTreeMap<BodyExprId, Type>,
    ) -> Type {
        let expr_id = self.allocate_expr_id();
        let ty = match &expr.node {
            Expr::IntegerLiteral(_) => Type::builtin(BuiltinType::I32),
            Expr::FloatLiteral(_) => Type::builtin(BuiltinType::F64),
            Expr::CharLiteral(_) => Type::builtin(BuiltinType::Char),
            Expr::BooleanLiteral(_) => Type::builtin(BuiltinType::Bool),
            Expr::StringLiteral(_) => Type::builtin(BuiltinType::String),
            Expr::Identifier(name) => self.type_for_path_reference(
                expr.span,
                vec![name.clone()],
                typed_items,
                issues,
            ),
            Expr::SelfValue => self.type_for_path_reference(
                expr.span,
                vec!["self".to_string()],
                typed_items,
                issues,
            ),
            Expr::NamespaceAccess { .. } => {
                let path = match Self::extract_namespace_path(&expr.node) {
                    Some(path) => path,
                    None => {
                        return self.store_and_return(
                            expr_id,
                            expr.span,
                            Type::error(),
                            types_by_expr_id,
                        );
                    }
                };
                self.type_for_path_reference(
                    expr.span,
                    path,
                    typed_items,
                    issues,
                )
            }
            Expr::Grouped(inner) => {
                self.check_expr(inner, typed_items, issues, types_by_expr_id)
            }
            Expr::Unary { op, expr: inner } => {
                let operand = self.check_expr(
                    inner,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                self.type_unary(expr.span, op.clone(), operand, issues)
            }
            Expr::Binary { op, lhs, rhs } => {
                let lhs_ty =
                    self.check_expr(lhs, typed_items, issues, types_by_expr_id);
                let rhs_ty =
                    self.check_expr(rhs, typed_items, issues, types_by_expr_id);
                self.type_binary(expr.span, *op, lhs_ty, rhs_ty, issues)
            }
            Expr::Assignment { target, value, .. } => {
                let target_ty = self.check_expr(
                    target,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                let value_ty = self.check_expr(
                    value,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );

                match self.assignment_target_status(target) {
                    AssignmentTargetStatus::MutableLocalOrNonPath => {}
                    AssignmentTargetStatus::ImmutableLocal(local_id) => {
                        issues.push(ExprCheckIssue {
                            owner: self.body.owner.clone(),
                            body_index: self.body.body_index,
                            span: expr.span,
                            kind: ExprCheckIssueKind::MutabilityViolation {
                                local_id,
                            },
                        });
                        return self.store_and_return(
                            expr_id,
                            expr.span,
                            Type::error(),
                            types_by_expr_id,
                        );
                    }
                    AssignmentTargetStatus::Invalid => {
                        issues.push(ExprCheckIssue {
                            owner: self.body.owner.clone(),
                            body_index: self.body.body_index,
                            span: expr.span,
                            kind: ExprCheckIssueKind::InvalidAssignmentTarget,
                        });
                        return self.store_and_return(
                            expr_id,
                            expr.span,
                            Type::error(),
                            types_by_expr_id,
                        );
                    }
                }

                if !target_ty.is_error()
                    && !value_ty.is_error()
                    && target_ty != value_ty
                {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: expr.span,
                        kind: ExprCheckIssueKind::AssignmentTypeMismatch {
                            target: target_ty.clone(),
                            value: value_ty,
                        },
                    });
                    Type::error()
                } else {
                    target_ty
                }
            }
            Expr::Call {
                callee,
                args,
                trailing_closure,
            } => {
                let callee_sig =
                    self.call_signature_for_callee(callee, typed_items);
                let _ = self.check_expr(
                    callee,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );

                let mut arg_types = Vec::with_capacity(args.len());
                for arg in args {
                    arg_types.push(self.check_expr(
                        &arg.value,
                        typed_items,
                        issues,
                        types_by_expr_id,
                    ));
                }
                if let Some(trailing_closure) = trailing_closure {
                    let _ = self.check_expr(
                        trailing_closure,
                        typed_items,
                        issues,
                        types_by_expr_id,
                    );
                    arg_types.push(Type::error());
                }

                let signature = match callee_sig {
                    CallSignatureResolution::Signature(signature) => signature,
                    CallSignatureResolution::BareExtern {
                        function,
                        namespace,
                    } => {
                        issues.push(ExprCheckIssue {
                            owner: self.body.owner.clone(),
                            body_index: self.body.body_index,
                            span: expr.span,
                            kind: ExprCheckIssueKind::BareExternFunctionCall {
                                function,
                                namespace,
                            },
                        });
                        return self.store_and_return(
                            expr_id,
                            expr.span,
                            Type::error(),
                            types_by_expr_id,
                        );
                    }
                    CallSignatureResolution::Missing => {
                        issues.push(ExprCheckIssue {
                            owner: self.body.owner.clone(),
                            body_index: self.body.body_index,
                            span: expr.span,
                            kind: ExprCheckIssueKind::InvalidCallCallee,
                        });
                        return self.store_and_return(
                            expr_id,
                            expr.span,
                            Type::error(),
                            types_by_expr_id,
                        );
                    }
                };

                if signature.param_types.len() != arg_types.len() {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: expr.span,
                        kind: ExprCheckIssueKind::CallArityMismatch {
                            expected: signature.param_types.len(),
                            found: arg_types.len(),
                        },
                    });
                    Type::error()
                } else {
                    for (index, (expected, actual)) in signature
                        .param_types
                        .iter()
                        .zip(arg_types.iter())
                        .enumerate()
                    {
                        if !expected.is_error()
                            && !actual.is_error()
                            && expected != actual
                        {
                            issues.push(ExprCheckIssue {
                                owner: self.body.owner.clone(),
                                body_index: self.body.body_index,
                                span: expr.span,
                                kind: ExprCheckIssueKind::CallArgTypeMismatch {
                                    index,
                                    expected: expected.clone(),
                                    found: actual.clone(),
                                },
                            });
                            return self.store_and_return(
                                expr_id,
                                expr.span,
                                Type::error(),
                                types_by_expr_id,
                            );
                        }
                    }

                    signature.return_type.clone().unwrap_or_else(Type::void)
                }
            }
            Expr::Block(block) | Expr::UnsafeBlock(block) => {
                self.check_block(block, typed_items, issues, types_by_expr_id)
            }
            Expr::If {
                clauses,
                then_branch,
                else_branch,
            } => {
                self.check_clause_list(
                    clauses,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                let mut then_checker = Self {
                    body: self.body,
                    local_types: self.local_types.clone(),
                    imports: self.imports,
                    external_lookup: self.external_lookup,
                    expr_ids_by_span: BTreeMap::new(),
                    next_expr_index: self.next_expr_index,
                };
                let then_ty = then_checker.check_block(
                    then_branch,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                self.next_expr_index = then_checker.next_expr_index;
                for (key, ids) in then_checker.expr_ids_by_span {
                    self.expr_ids_by_span.entry(key).or_default().extend(ids);
                }

                let Some(else_branch) = else_branch else {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: expr.span,
                        kind: ExprCheckIssueKind::MissingElseBranch,
                    });
                    return self.store_and_return(
                        expr_id,
                        expr.span,
                        Type::error(),
                        types_by_expr_id,
                    );
                };

                let mut else_checker = Self {
                    body: self.body,
                    local_types: self.local_types.clone(),
                    imports: self.imports,
                    external_lookup: self.external_lookup,
                    expr_ids_by_span: BTreeMap::new(),
                    next_expr_index: self.next_expr_index,
                };
                let else_ty = else_checker.check_expr(
                    else_branch,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                self.next_expr_index = else_checker.next_expr_index;
                for (key, ids) in else_checker.expr_ids_by_span {
                    self.expr_ids_by_span.entry(key).or_default().extend(ids);
                }

                if then_ty == else_ty {
                    then_ty
                } else if then_ty.is_error() || else_ty.is_error() {
                    Type::error()
                } else {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: expr.span,
                        kind: ExprCheckIssueKind::IncompatibleIfBranches {
                            then_type: then_ty,
                            else_type: else_ty,
                        },
                    });
                    Type::error()
                }
            }
            Expr::Match { subject, arms } => {
                let _ = self.check_expr(
                    subject,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                let mut arm_types = Vec::new();
                for arm in arms {
                    let arm_ty = match &arm.node.body {
                        MatchArmBody::Expr(value) => self.check_expr(
                            value,
                            typed_items,
                            issues,
                            types_by_expr_id,
                        ),
                        MatchArmBody::Block(block) => self.check_block(
                            block,
                            typed_items,
                            issues,
                            types_by_expr_id,
                        ),
                    };
                    arm_types.push(arm_ty);
                }
                if arm_types.is_empty() {
                    Type::void()
                } else if arm_types.windows(2).all(|pair| pair[0] == pair[1]) {
                    arm_types[0].clone()
                } else {
                    Type::error()
                }
            }
            Expr::Try { expr: inner }
            | Expr::ForceUnwrap { expr: inner }
            | Expr::Spread { expr: inner } => {
                self.check_expr(inner, typed_items, issues, types_by_expr_id)
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                let condition_ty = self.check_expr(
                    condition,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                if condition_ty != Type::builtin(BuiltinType::Bool)
                    && !condition_ty.is_error()
                {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: condition.span,
                        kind: ExprCheckIssueKind::InvalidBinaryOp,
                    });
                }
                let then_ty = self.check_expr(
                    then_expr,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                let else_ty = self.check_expr(
                    else_expr,
                    typed_items,
                    issues,
                    types_by_expr_id,
                );
                if then_ty == else_ty {
                    then_ty
                } else {
                    Type::error()
                }
            }
            Expr::ArrayLiteral(elements) => {
                for element in elements {
                    match element {
                        crate::frontend::ast::ArrayElement::Expr(value)
                        | crate::frontend::ast::ArrayElement::Spread(value) => {
                            self.check_expr(
                                value,
                                typed_items,
                                issues,
                                types_by_expr_id,
                            );
                        }
                    }
                }
                Type::error()
            }
            Expr::StructLiteral { fields, .. } => {
                for field in fields {
                    match field {
                        crate::frontend::ast::StructLiteralField::Named { value, .. }
                        | crate::frontend::ast::StructLiteralField::Spread {
                            value,
                        } => {
                            self.check_expr(
                                value,
                                typed_items,
                                issues,
                                types_by_expr_id,
                            );
                        }
                        crate::frontend::ast::StructLiteralField::Shorthand {
                            ..
                        } => {}
                    }
                }
                Type::error()
            }
            Expr::Closure { .. }
            | Expr::Macro { .. }
            | Expr::MemberAccess { .. }
            | Expr::OptionalMemberAccess { .. }
            | Expr::Index { .. }
            | Expr::OptionalIndex { .. }
            | Expr::Cast { .. }
            | Expr::Range { .. }
            | Expr::ShorthandMember { .. }
            | Expr::QualifiedMember { .. }
            | Expr::SelfType => Type::error(),
        };

        self.store_and_return(expr_id, expr.span, ty, types_by_expr_id)
    }

    fn store_and_return(
        &mut self,
        expr_id: BodyExprId,
        span: Span,
        ty: Type,
        types_by_expr_id: &mut BTreeMap<BodyExprId, Type>,
    ) -> Type {
        let key = (
            expr_id.owner.clone(),
            expr_id.body_index,
            span.start,
            span.end,
        );
        self.expr_ids_by_span
            .entry(key)
            .or_default()
            .push(expr_id.clone());
        types_by_expr_id.insert(expr_id, ty.clone());
        ty
    }

    fn infer_local_from_pattern(
        &mut self,
        pattern: &crate::frontend::ast::Spanned<crate::frontend::ast::Pattern>,
        inferred_type: Type,
        requires_mutable: bool,
    ) {
        let crate::frontend::ast::Pattern::Identifier(name) = &pattern.node
        else {
            return;
        };
        if inferred_type.is_error() {
            return;
        }

        let local = self.body.locals.iter().find(|local| {
            local.declared_span == pattern.span && local.name == *name
        });
        let Some(local) = local else {
            return;
        };
        if requires_mutable && local.mutability != LocalMutability::Mutable {
            return;
        }

        let current = self
            .local_types
            .get(&local.id)
            .cloned()
            .unwrap_or_else(Type::error);
        if current.is_error() {
            self.local_types.insert(local.id, inferred_type);
        }
    }

    fn check_clause_list(
        &mut self,
        clauses: &crate::frontend::ast::ClauseList,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
        types_by_expr_id: &mut BTreeMap<BodyExprId, Type>,
    ) {
        for clause in &clauses.clauses {
            match &clause.node {
                Clause::Expr(expr) => {
                    let ty = self.check_expr(
                        expr,
                        typed_items,
                        issues,
                        types_by_expr_id,
                    );
                    if ty != Type::builtin(BuiltinType::Bool) && !ty.is_error()
                    {
                        issues.push(ExprCheckIssue {
                            owner: self.body.owner.clone(),
                            body_index: self.body.body_index,
                            span: clause.span,
                            kind: ExprCheckIssueKind::InvalidBinaryOp,
                        });
                    }
                }
                Clause::LetBinding(binding) => {
                    let value_ty = self.check_expr(
                        &binding.value,
                        typed_items,
                        issues,
                        types_by_expr_id,
                    );
                    self.infer_local_from_pattern(
                        &binding.pattern,
                        value_ty,
                        false,
                    );
                }
                Clause::VarBinding(binding) => {
                    let value_ty = self.check_expr(
                        &binding.value,
                        typed_items,
                        issues,
                        types_by_expr_id,
                    );
                    self.infer_local_from_pattern(
                        &binding.pattern,
                        value_ty,
                        true,
                    );
                }
            }
        }
    }

    fn type_for_path_reference(
        &self,
        span: Span,
        segments: Vec<String>,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
    ) -> Type {
        let Some(resolved) = self
            .body
            .references
            .iter()
            .find(|reference| {
                reference.span == span && reference.segments == segments
            })
            .map(|reference| reference.resolved.clone())
        else {
            issues.push(ExprCheckIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: ExprCheckIssueKind::MissingResolvedReference { segments },
            });
            return Type::error();
        };

        match resolved {
            ResolvedBodyRef::Local(local_id) => {
                self.local_types.get(&local_id).cloned().unwrap_or_else(|| {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: ExprCheckIssueKind::MissingLocalType { local_id },
                    });
                    Type::error()
                })
            }
            ResolvedBodyRef::Item(item_id)
            | ResolvedBodyRef::Import(item_id) => {
                self.type_for_item_reference(span, item_id, typed_items, issues)
            }
            ResolvedBodyRef::Unresolved => Type::error(),
        }
    }

    fn type_for_item_reference(
        &self,
        span: Span,
        item_id: ItemId,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
    ) -> Type {
        match typed_items.get(item_id) {
            Some(TypedItemData::Struct(_))
            | Some(TypedItemData::Enum(_))
            | Some(TypedItemData::Protocol(_)) => Type::error(),
            Some(TypedItemData::Function(_)) => Type::error(),
            None => {
                issues.push(ExprCheckIssue {
                    owner: self.body.owner.clone(),
                    body_index: self.body.body_index,
                    span,
                    kind: ExprCheckIssueKind::MissingTypedItem { item_id },
                });
                Type::error()
            }
        }
    }

    fn call_signature_for_callee(
        &self,
        callee: &crate::frontend::ast::Spanned<Expr>,
        typed_items: &TypedItemTable,
    ) -> CallSignatureResolution {
        let Some(path) = Self::extract_namespace_path(&callee.node) else {
            return CallSignatureResolution::Missing;
        };

        if let Some(signature) =
            self.local_signature_for_callee(callee, &path, typed_items)
        {
            return CallSignatureResolution::Signature(signature);
        }

        if let Some(signature) = self.external_import_signature_for_path(&path)
        {
            return CallSignatureResolution::Signature(signature);
        }

        if let Some(signature) =
            self.direct_named_root_signature_for_path(&path)
        {
            return CallSignatureResolution::Signature(signature);
        }

        if let Some(signature) = self.extern_signature_for_path(&path) {
            return CallSignatureResolution::Signature(signature);
        }

        if path.len() == 1
            && let Some(namespace) =
                self.external_lookup.extern_namespace_for_function(&path[0])
        {
            return CallSignatureResolution::BareExtern {
                function: path[0].clone(),
                namespace: namespace.to_string(),
            };
        }

        CallSignatureResolution::Missing
    }

    fn local_signature_for_callee(
        &self,
        callee: &crate::frontend::ast::Spanned<Expr>,
        path: &[String],
        typed_items: &TypedItemTable,
    ) -> Option<TypedFunctionSignature> {
        let resolved = self
            .body
            .references
            .iter()
            .find(|reference| {
                reference.span == callee.span && reference.segments == path
            })
            .map(|reference| reference.resolved.clone())?;
        let item_id = match resolved {
            ResolvedBodyRef::Item(item_id)
            | ResolvedBodyRef::Import(item_id) => item_id,
            ResolvedBodyRef::Local(_) | ResolvedBodyRef::Unresolved => {
                return None;
            }
        };
        typed_items.function(item_id).cloned()
    }

    fn external_import_signature_for_path(
        &self,
        path: &[String],
    ) -> Option<TypedFunctionSignature> {
        let (first, rest) = path.split_first()?;
        let imports = self.imports.get(&self.body.containing_scope_file_id)?;
        let binding = imports.get(first)?;
        let mut full_path = if rest.is_empty() {
            binding.target_path.clone()
        } else {
            match binding.kind {
                ImportBindingKind::Scope => {
                    let mut combined = binding.target_path.clone();
                    combined.extend(rest.iter().cloned());
                    combined
                }
                ImportBindingKind::Symbol(_) => return None,
            }
        };

        let source_root = binding.source_root.as_deref()?;
        if full_path.is_empty() {
            full_path = binding.target_path.clone();
        }
        self.external_lookup
            .function_for_named_root_path(source_root, &full_path)
            .cloned()
    }

    fn direct_named_root_signature_for_path(
        &self,
        path: &[String],
    ) -> Option<TypedFunctionSignature> {
        let (root, rest) = path.split_first()?;
        if rest.is_empty() {
            return None;
        }
        self.external_lookup
            .function_for_named_root_path(root, rest)
            .cloned()
    }

    fn extern_signature_for_path(
        &self,
        path: &[String],
    ) -> Option<TypedFunctionSignature> {
        if path.len() != 2 {
            return None;
        }
        let library = path[0].as_str();
        let function = path[1].as_str();
        self.external_lookup
            .extern_function_signature(library, function)
            .cloned()
    }

    fn type_unary(
        &self,
        span: Span,
        op: UnaryOp,
        operand: Type,
        issues: &mut Vec<ExprCheckIssue>,
    ) -> Type {
        if operand.is_error() {
            return Type::error();
        }
        match op {
            UnaryOp::Negate if is_numeric_type(&operand) => operand,
            UnaryOp::Not if operand == Type::builtin(BuiltinType::Bool) => {
                Type::builtin(BuiltinType::Bool)
            }
            _ => {
                issues.push(ExprCheckIssue {
                    owner: self.body.owner.clone(),
                    body_index: self.body.body_index,
                    span,
                    kind: ExprCheckIssueKind::InvalidUnaryOp,
                });
                Type::error()
            }
        }
    }

    fn type_binary(
        &self,
        span: Span,
        op: BinaryOp,
        lhs: Type,
        rhs: Type,
        issues: &mut Vec<ExprCheckIssue>,
    ) -> Type {
        if lhs.is_error() || rhs.is_error() {
            return Type::error();
        }

        match op {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Remainder => {
                if is_numeric_type(&lhs) && lhs == rhs {
                    lhs
                } else {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: ExprCheckIssueKind::InvalidBinaryOp,
                    });
                    Type::error()
                }
            }
            BinaryOp::LogicalAnd | BinaryOp::LogicalOr => {
                if lhs == Type::builtin(BuiltinType::Bool)
                    && rhs == Type::builtin(BuiltinType::Bool)
                {
                    Type::builtin(BuiltinType::Bool)
                } else {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: ExprCheckIssueKind::InvalidBinaryOp,
                    });
                    Type::error()
                }
            }
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => {
                if lhs == rhs {
                    Type::builtin(BuiltinType::Bool)
                } else {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: ExprCheckIssueKind::InvalidBinaryOp,
                    });
                    Type::error()
                }
            }
            BinaryOp::NullCoalescing => {
                if lhs == rhs {
                    lhs
                } else {
                    Type::error()
                }
            }
            BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::BitAnd
            | BinaryOp::ShiftLeft
            | BinaryOp::ShiftRight => {
                if is_integer_type(&lhs) && lhs == rhs {
                    lhs
                } else {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: ExprCheckIssueKind::InvalidBinaryOp,
                    });
                    Type::error()
                }
            }
        }
    }

    fn assignment_target_status(
        &self,
        target: &crate::frontend::ast::Spanned<Expr>,
    ) -> AssignmentTargetStatus {
        let Some(path) = Self::extract_namespace_path(&target.node) else {
            return AssignmentTargetStatus::MutableLocalOrNonPath;
        };
        let Some(reference) = self.body.references.iter().find(|reference| {
            reference.span == target.span && reference.segments == path
        }) else {
            return AssignmentTargetStatus::Invalid;
        };
        match reference.resolved {
            ResolvedBodyRef::Local(local_id) => {
                match self.body.locals.iter().find(|local| local.id == local_id)
                {
                    Some(local)
                        if local.mutability == LocalMutability::Mutable =>
                    {
                        AssignmentTargetStatus::MutableLocalOrNonPath
                    }
                    Some(_) => AssignmentTargetStatus::ImmutableLocal(local_id),
                    None => AssignmentTargetStatus::Invalid,
                }
            }
            ResolvedBodyRef::Item(_)
            | ResolvedBodyRef::Import(_)
            | ResolvedBodyRef::Unresolved => AssignmentTargetStatus::Invalid,
        }
    }

    fn extract_namespace_path(expr: &Expr) -> Option<Vec<String>> {
        match expr {
            Expr::Identifier(name) => Some(vec![name.clone()]),
            Expr::NamespaceAccess { base, member, .. } => {
                let mut path = Self::extract_namespace_path(&base.node)?;
                path.push(member.clone());
                Some(path)
            }
            _ => None,
        }
    }
}

enum CallSignatureResolution {
    Signature(TypedFunctionSignature),
    BareExtern { function: String, namespace: String },
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignmentTargetStatus {
    MutableLocalOrNonPath,
    ImmutableLocal(LocalId),
    Invalid,
}

fn is_numeric_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Builtin(BuiltinType::I8)
            | Type::Builtin(BuiltinType::I16)
            | Type::Builtin(BuiltinType::I32)
            | Type::Builtin(BuiltinType::I64)
            | Type::Builtin(BuiltinType::U8)
            | Type::Builtin(BuiltinType::U16)
            | Type::Builtin(BuiltinType::U32)
            | Type::Builtin(BuiltinType::U64)
            | Type::Builtin(BuiltinType::ISize)
            | Type::Builtin(BuiltinType::USize)
            | Type::Builtin(BuiltinType::F32)
            | Type::Builtin(BuiltinType::F64)
    )
}

fn is_integer_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Builtin(BuiltinType::I8)
            | Type::Builtin(BuiltinType::I16)
            | Type::Builtin(BuiltinType::I32)
            | Type::Builtin(BuiltinType::I64)
            | Type::Builtin(BuiltinType::U8)
            | Type::Builtin(BuiltinType::U16)
            | Type::Builtin(BuiltinType::U32)
            | Type::Builtin(BuiltinType::U64)
            | Type::Builtin(BuiltinType::ISize)
            | Type::Builtin(BuiltinType::USize)
    )
}
