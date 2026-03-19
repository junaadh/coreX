use super::body_env::BodyTypeEnvironmentTable;
use super::expr_check::{ExpressionTypeTable, check_expression_types};
use super::{BuiltinType, Type, TypedItemTable};
use crate::frontend::DesugaredFile;
use crate::frontend::ast::{
    Block, Clause, Expr, Item, Pattern, Span, Stmt, StructMember,
};
use crate::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, LocalId, ResolvedBody,
    ResolvedBodyTable, ScopeGraph,
};
use crate::frontend::source::FileId;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BodyStmtId {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub stmt_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    Let,
    Var,
    Expr,
    Return,
    If,
    Guard,
    While,
    For,
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementTypeEntry {
    pub kind: StatementKind,
    pub span: Span,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmtCheckIssueKind {
    MissingBodyAst,
    MissingBodyEnvironment,
    MissingExpressionType {
        span: Span,
    },
    MissingPatternLocal {
        span: Span,
    },
    AnnotatedLocalTypeMismatch {
        local_id: LocalId,
        annotated: Type,
        initializer: Type,
    },
    AssignmentTypeMismatch {
        target: Type,
        value: Type,
    },
    ReturnTypeMismatch {
        expected: Type,
        found: Type,
    },
    MissingReturnValue {
        expected: Type,
    },
    UnexpectedReturnValue {
        found: Type,
    },
    InvalidConditionType {
        found: Type,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StmtCheckIssue {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub span: Span,
    pub kind: StmtCheckIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementTypeTable {
    by_stmt_id: BTreeMap<BodyStmtId, StatementTypeEntry>,
    locals_by_body:
        BTreeMap<(DeclarationOwner, usize), BTreeMap<LocalId, Type>>,
    pub issues: Vec<StmtCheckIssue>,
}

impl StatementTypeTable {
    #[must_use]
    pub fn stmt_entry(
        &self,
        stmt_id: &BodyStmtId,
    ) -> Option<&StatementTypeEntry> {
        self.by_stmt_id.get(stmt_id)
    }

    #[must_use]
    pub fn stmt_ids_for_body(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Vec<BodyStmtId> {
        self.by_stmt_id
            .keys()
            .filter(|id| id.owner == *owner && id.body_index == body_index)
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn local_types_for_body(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Option<&BTreeMap<LocalId, Type>> {
        self.locals_by_body.get(&(owner.clone(), body_index))
    }

    #[must_use]
    pub fn local_type(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        local_id: LocalId,
    ) -> Option<&Type> {
        self.local_types_for_body(owner, body_index)?.get(&local_id)
    }

    #[must_use]
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&BodyStmtId, &StatementTypeEntry)> {
        self.by_stmt_id.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_stmt_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_stmt_id.is_empty()
    }
}

#[must_use]
pub fn check_statements(
    graph: &ScopeGraph,
    parsed_files: &[DesugaredFile],
    global_items: &GlobalItemTable,
    typed_items: &TypedItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
) -> StatementTypeTable {
    let expr_types = check_expression_types(
        graph,
        parsed_files,
        global_items,
        typed_items,
        resolved_bodies,
        body_envs,
    );
    check_statements_with_expression_types(
        graph,
        parsed_files,
        global_items,
        resolved_bodies,
        body_envs,
        &expr_types,
    )
}

#[must_use]
pub fn check_statements_with_expression_types(
    graph: &ScopeGraph,
    parsed_files: &[DesugaredFile],
    global_items: &GlobalItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
    expr_types: &ExpressionTypeTable,
) -> StatementTypeTable {
    let parsed_by_id: BTreeMap<FileId, &DesugaredFile> = parsed_files
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect();
    let body_blocks = collect_body_blocks(graph, &parsed_by_id, global_items);

    let mut by_stmt_id = BTreeMap::new();
    let mut locals_by_body = BTreeMap::new();
    let mut issues = Vec::new();

    for body in resolved_bodies.iter() {
        let Some(env) = body_envs
            .envs_for_owner(&body.owner)
            .iter()
            .find(|candidate| candidate.body_index == body.body_index)
        else {
            issues.push(StmtCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: StmtCheckIssueKind::MissingBodyEnvironment,
            });
            continue;
        };

        let Some(block_entry) = body_blocks
            .get(&body.owner)
            .and_then(|entries| entries.get(body.body_index))
        else {
            issues.push(StmtCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: StmtCheckIssueKind::MissingBodyAst,
            });
            continue;
        };

        let mut checker = BodyStmtChecker::new(body, env.local_types.clone());
        checker.check_block(
            block_entry.block,
            env.expected_return_type.clone(),
            expr_types,
            &mut by_stmt_id,
            &mut issues,
        );
        locals_by_body
            .insert((body.owner.clone(), body.body_index), checker.local_types);
    }

    StatementTypeTable {
        by_stmt_id,
        locals_by_body,
        issues,
    }
}

struct BodyBlockEntry<'a> {
    block: &'a Block,
}

fn collect_body_blocks<'a>(
    graph: &'a ScopeGraph,
    parsed_by_id: &'a BTreeMap<FileId, &'a DesugaredFile>,
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
) -> Option<crate::frontend::resolver::ItemId> {
    let mut full_path = scope_path.to_vec();
    full_path.push(name.to_string());
    let item_id = global_items.item_id_by_full_path(&full_path)?;
    let item = global_items.get(item_id)?;
    (item.containing_scope_file_id == scope_file_id).then_some(item_id)
}

struct BodyStmtChecker<'a> {
    body: &'a ResolvedBody,
    local_types: BTreeMap<LocalId, Type>,
    next_stmt_index: u32,
    missing_expr_spans: HashSet<(usize, usize)>,
}

impl<'a> BodyStmtChecker<'a> {
    fn new(
        body: &'a ResolvedBody,
        local_types: BTreeMap<LocalId, Type>,
    ) -> Self {
        Self {
            body,
            local_types,
            next_stmt_index: 0,
            missing_expr_spans: HashSet::new(),
        }
    }

    fn allocate_stmt_id(&mut self) -> BodyStmtId {
        let stmt_index = self.next_stmt_index;
        self.next_stmt_index = self.next_stmt_index.saturating_add(1);
        BodyStmtId {
            owner: self.body.owner.clone(),
            body_index: self.body.body_index,
            stmt_index,
        }
    }

    fn check_block(
        &mut self,
        block: &Block,
        expected_return_type: Type,
        expr_types: &ExpressionTypeTable,
        by_stmt_id: &mut BTreeMap<BodyStmtId, StatementTypeEntry>,
        issues: &mut Vec<StmtCheckIssue>,
    ) {
        for stmt in &block.statements {
            self.check_stmt(
                stmt,
                expected_return_type.clone(),
                expr_types,
                by_stmt_id,
                issues,
            );
        }
    }

    fn check_stmt(
        &mut self,
        stmt: &crate::frontend::ast::Spanned<Stmt>,
        expected_return_type: Type,
        expr_types: &ExpressionTypeTable,
        by_stmt_id: &mut BTreeMap<BodyStmtId, StatementTypeEntry>,
        issues: &mut Vec<StmtCheckIssue>,
    ) {
        let stmt_id = self.allocate_stmt_id();
        match &stmt.node {
            Stmt::Let(let_stmt) => {
                if let Some(value) = &let_stmt.node.value {
                    let init_ty =
                        self.expr_type_or_error(value.span, expr_types, issues);
                    self.apply_binding_type(
                        &let_stmt.node.pattern,
                        init_ty,
                        issues,
                    );
                }
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::Let,
                        span: stmt.span,
                        ty: Type::void(),
                    },
                );
            }
            Stmt::Var(var_stmt) => {
                if let Some(value) = &var_stmt.node.value {
                    let init_ty =
                        self.expr_type_or_error(value.span, expr_types, issues);
                    self.apply_binding_type(
                        &var_stmt.node.pattern,
                        init_ty,
                        issues,
                    );
                }
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::Var,
                        span: stmt.span,
                        ty: Type::void(),
                    },
                );
            }
            Stmt::Expr { expr, .. } => {
                let expr_ty =
                    self.expr_type_or_error(expr.span, expr_types, issues);
                if let Expr::Assignment { target, value, .. } = &expr.node {
                    let target_ty = self.expr_type_or_error(
                        target.span,
                        expr_types,
                        issues,
                    );
                    let value_ty =
                        self.expr_type_or_error(value.span, expr_types, issues);
                    if !target_ty.is_error()
                        && !value_ty.is_error()
                        && target_ty != value_ty
                    {
                        issues.push(StmtCheckIssue {
                            owner: self.body.owner.clone(),
                            body_index: self.body.body_index,
                            span: expr.span,
                            kind: StmtCheckIssueKind::AssignmentTypeMismatch {
                                target: target_ty,
                                value: value_ty,
                            },
                        });
                    }
                }
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::Expr,
                        span: stmt.span,
                        ty: expr_ty,
                    },
                );
            }
            Stmt::Return(value) => {
                let return_ty = value
                    .as_ref()
                    .map(|expr| {
                        self.expr_type_or_error(expr.span, expr_types, issues)
                    })
                    .unwrap_or_else(Type::void);
                if value.is_none()
                    && expected_return_type != Type::void()
                    && !expected_return_type.is_error()
                {
                    issues.push(StmtCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: stmt.span,
                        kind: StmtCheckIssueKind::MissingReturnValue {
                            expected: expected_return_type.clone(),
                        },
                    });
                } else if value.is_some()
                    && expected_return_type == Type::void()
                    && !return_ty.is_error()
                {
                    issues.push(StmtCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: stmt.span,
                        kind: StmtCheckIssueKind::UnexpectedReturnValue {
                            found: return_ty.clone(),
                        },
                    });
                } else if value.is_some()
                    && !expected_return_type.is_error()
                    && !return_ty.is_error()
                    && expected_return_type != return_ty
                {
                    issues.push(StmtCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: stmt.span,
                        kind: StmtCheckIssueKind::ReturnTypeMismatch {
                            expected: expected_return_type.clone(),
                            found: return_ty.clone(),
                        },
                    });
                }
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::Return,
                        span: stmt.span,
                        ty: return_ty,
                    },
                );
            }
            Stmt::If(if_stmt) => {
                self.check_clause_list(
                    &if_stmt.node.clauses,
                    expr_types,
                    issues,
                );
                self.check_block(
                    &if_stmt.node.then_branch,
                    expected_return_type.clone(),
                    expr_types,
                    by_stmt_id,
                    issues,
                );
                if let Some(else_branch) = &if_stmt.node.else_branch {
                    match else_branch {
                        crate::frontend::ast::IfStmtElse::If(nested) => {
                            let nested_stmt =
                                crate::frontend::ast::Spanned::new(
                                    Stmt::If(*nested.clone()),
                                    nested.span,
                                );
                            self.check_stmt(
                                &nested_stmt,
                                expected_return_type,
                                expr_types,
                                by_stmt_id,
                                issues,
                            );
                        }
                        crate::frontend::ast::IfStmtElse::Block(block) => {
                            self.check_block(
                                block,
                                expected_return_type,
                                expr_types,
                                by_stmt_id,
                                issues,
                            );
                        }
                    }
                }
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::If,
                        span: stmt.span,
                        ty: Type::void(),
                    },
                );
            }
            Stmt::Guard(guard_stmt) => {
                self.check_clause_list(
                    &guard_stmt.node.clauses,
                    expr_types,
                    issues,
                );
                self.check_block(
                    &guard_stmt.node.else_block,
                    expected_return_type,
                    expr_types,
                    by_stmt_id,
                    issues,
                );
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::Guard,
                        span: stmt.span,
                        ty: Type::void(),
                    },
                );
            }
            Stmt::While(while_stmt) => {
                self.check_clause_list(
                    &while_stmt.node.clauses,
                    expr_types,
                    issues,
                );
                self.check_block(
                    &while_stmt.node.body,
                    expected_return_type,
                    expr_types,
                    by_stmt_id,
                    issues,
                );
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::While,
                        span: stmt.span,
                        ty: Type::void(),
                    },
                );
            }
            Stmt::For(for_stmt) => {
                let _ = self.expr_type_or_error(
                    for_stmt.node.iterator.span,
                    expr_types,
                    issues,
                );
                self.check_block(
                    &for_stmt.node.body,
                    expected_return_type,
                    expr_types,
                    by_stmt_id,
                    issues,
                );
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::For,
                        span: stmt.span,
                        ty: Type::void(),
                    },
                );
            }
            Stmt::Break => {
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::Break,
                        span: stmt.span,
                        ty: Type::void(),
                    },
                );
            }
            Stmt::Continue => {
                by_stmt_id.insert(
                    stmt_id,
                    StatementTypeEntry {
                        kind: StatementKind::Continue,
                        span: stmt.span,
                        ty: Type::void(),
                    },
                );
            }
        }
    }

    fn check_clause_list(
        &mut self,
        clauses: &crate::frontend::ast::ClauseList,
        expr_types: &ExpressionTypeTable,
        issues: &mut Vec<StmtCheckIssue>,
    ) {
        for clause in &clauses.clauses {
            match &clause.node {
                Clause::Expr(expr) => {
                    let found =
                        self.expr_type_or_error(expr.span, expr_types, issues);
                    if found != Type::builtin(BuiltinType::Bool)
                        && !found.is_error()
                    {
                        issues.push(StmtCheckIssue {
                            owner: self.body.owner.clone(),
                            body_index: self.body.body_index,
                            span: clause.span,
                            kind: StmtCheckIssueKind::InvalidConditionType {
                                found,
                            },
                        });
                    }
                }
                Clause::LetBinding(binding) | Clause::VarBinding(binding) => {
                    let init_ty = self.expr_type_or_error(
                        binding.value.span,
                        expr_types,
                        issues,
                    );
                    self.apply_binding_type(&binding.pattern, init_ty, issues);
                }
            }
        }
    }

    fn apply_binding_type(
        &mut self,
        pattern: &crate::frontend::ast::Spanned<Pattern>,
        initializer: Type,
        issues: &mut Vec<StmtCheckIssue>,
    ) {
        let Some(local) = self.local_for_pattern(pattern) else {
            issues.push(StmtCheckIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span: pattern.span,
                kind: StmtCheckIssueKind::MissingPatternLocal {
                    span: pattern.span,
                },
            });
            return;
        };

        let current = self
            .local_types
            .get(&local.id)
            .cloned()
            .unwrap_or_else(Type::error);

        if local.declared_type.is_some() {
            if !current.is_error()
                && !initializer.is_error()
                && current != initializer
            {
                issues.push(StmtCheckIssue {
                    owner: self.body.owner.clone(),
                    body_index: self.body.body_index,
                    span: pattern.span,
                    kind: StmtCheckIssueKind::AnnotatedLocalTypeMismatch {
                        local_id: local.id,
                        annotated: current,
                        initializer,
                    },
                });
            }
            return;
        }

        if current.is_error() && !initializer.is_error() {
            self.local_types.insert(local.id, initializer);
        }
    }

    fn local_for_pattern(
        &self,
        pattern: &crate::frontend::ast::Spanned<Pattern>,
    ) -> Option<&crate::frontend::ResolvedLocalBinding> {
        let Pattern::Identifier(name) = &pattern.node else {
            return None;
        };
        self.body.locals.iter().find(|local| {
            local.declared_span == pattern.span && local.name == *name
        })
    }

    fn expr_type_or_error(
        &mut self,
        span: Span,
        expr_types: &ExpressionTypeTable,
        issues: &mut Vec<StmtCheckIssue>,
    ) -> Type {
        match expr_types.expr_type_for_span(
            &self.body.owner,
            self.body.body_index,
            span,
        ) {
            Some(ty) => ty.clone(),
            None => {
                let span_key = (span.start, span.end);
                if self.missing_expr_spans.insert(span_key) {
                    issues.push(StmtCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: StmtCheckIssueKind::MissingExpressionType {
                            span,
                        },
                    });
                }
                Type::error()
            }
        }
    }
}
