use super::body_env::BodyTypeEnvironmentTable;
use super::expr_check::{ExpressionTypeTable, check_expression_types};
use super::hir_input::{SemanticBodyRef, SemanticHirInput};
use super::{BuiltinType, Type, TypedItemTable};
use crate::frontend::ast::Span;
use crate::frontend::hir::{
    HirBodyId, HirExprId, HirExprKind, HirModule, HirMutability, HirPatId,
    HirStmtId,
};
use crate::frontend::resolver::{
    DeclarationOwner, LocalId, ResolvedBody, ResolvedBodyTable,
};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BodyStmtId {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub hir_stmt_id: HirStmtId,
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
    local_types_by_hir_id_by_body:
        BTreeMap<(DeclarationOwner, usize), BTreeMap<LocalId, Type>>,
    hir_local_id_by_resolved_local_id_by_body:
        BTreeMap<(DeclarationOwner, usize), BTreeMap<LocalId, LocalId>>,
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
        self.local_types_by_hir_id_by_body
            .get(&(owner.clone(), body_index))
    }

    #[must_use]
    pub fn local_type(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        local_id: LocalId,
    ) -> Option<&Type> {
        let body_key = (owner.clone(), body_index);
        let hir_local_id = self
            .hir_local_id_by_resolved_local_id_by_body
            .get(&body_key)?
            .get(&local_id)?;
        self.local_types_for_body(owner, body_index)?
            .get(hir_local_id)
    }

    #[must_use]
    pub fn local_type_for_hir_local(
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
    hir_input: &SemanticHirInput,
    typed_items: &TypedItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
) -> StatementTypeTable {
    let expr_types = check_expression_types(
        hir_input,
        typed_items,
        resolved_bodies,
        body_envs,
    );
    check_statements_with_expression_types(
        hir_input,
        resolved_bodies,
        body_envs,
        &expr_types,
    )
}

#[must_use]
pub fn check_statements_with_expression_types(
    hir_input: &SemanticHirInput,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
    expr_types: &ExpressionTypeTable,
) -> StatementTypeTable {
    let mut by_stmt_id = BTreeMap::new();
    let mut local_types_by_hir_id_by_body = BTreeMap::new();
    let mut hir_local_id_by_resolved_local_id_by_body = BTreeMap::new();
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

        let Some(body_ref) = hir_input.body_ref(&body.owner, body.body_index)
        else {
            issues.push(StmtCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: StmtCheckIssueKind::MissingBodyAst,
            });
            continue;
        };

        let Some(module) = hir_input.hir_modules.get(&body_ref.file_id) else {
            issues.push(StmtCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: StmtCheckIssueKind::MissingBodyAst,
            });
            continue;
        };

        if !module.bodies.contains_key(&body_ref.body_id) {
            issues.push(StmtCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: StmtCheckIssueKind::MissingBodyAst,
            });
            continue;
        }

        let mut checker = BodyStmtChecker::new(
            body,
            body_ref,
            module,
            hir_input,
            env.local_types.clone(),
            env.local_bindings_by_resolved_id()
                .keys()
                .filter_map(|resolved_local_id| {
                    env.hir_local_id_for_resolved_local(*resolved_local_id)
                        .map(|hir_local_id| (*resolved_local_id, hir_local_id))
                })
                .collect(),
        );
        checker.check_body(
            body_ref.body_id,
            env.expected_return_type.clone(),
            expr_types,
            &mut by_stmt_id,
            &mut issues,
        );
        local_types_by_hir_id_by_body
            .insert((body.owner.clone(), body.body_index), checker.local_types);
        hir_local_id_by_resolved_local_id_by_body.insert(
            (body.owner.clone(), body.body_index),
            checker.hir_local_id_by_resolved_local_id,
        );
    }

    StatementTypeTable {
        by_stmt_id,
        local_types_by_hir_id_by_body,
        hir_local_id_by_resolved_local_id_by_body,
        issues,
    }
}

struct BodyStmtChecker<'a> {
    body: &'a ResolvedBody,
    body_ref: SemanticBodyRef,
    module: &'a HirModule,
    hir_input: &'a SemanticHirInput,
    local_types: BTreeMap<LocalId, Type>,
    hir_local_id_by_resolved_local_id: BTreeMap<LocalId, LocalId>,
    declared_type_hir_locals: HashSet<LocalId>,
    missing_expr_ids: HashSet<HirExprId>,
}

impl<'a> BodyStmtChecker<'a> {
    fn new(
        body: &'a ResolvedBody,
        body_ref: SemanticBodyRef,
        module: &'a HirModule,
        hir_input: &'a SemanticHirInput,
        local_types: BTreeMap<LocalId, Type>,
        hir_local_id_by_resolved_local_id: BTreeMap<LocalId, LocalId>,
    ) -> Self {
        let declared_type_hir_locals = hir_local_id_by_resolved_local_id
            .iter()
            .filter_map(|(resolved_local_id, hir_local_id)| {
                body.locals
                    .iter()
                    .find(|local| local.id == *resolved_local_id)
                    .is_some_and(|local| local.declared_type.is_some())
                    .then_some(*hir_local_id)
            })
            .collect();
        Self {
            body,
            body_ref,
            module,
            hir_input,
            local_types,
            hir_local_id_by_resolved_local_id,
            declared_type_hir_locals,
            missing_expr_ids: HashSet::new(),
        }
    }

    fn stmt_id(&self, hir_stmt_id: HirStmtId) -> BodyStmtId {
        BodyStmtId {
            owner: self.body.owner.clone(),
            body_index: self.body.body_index,
            hir_stmt_id,
        }
    }

    fn check_body(
        &mut self,
        body_id: HirBodyId,
        expected_return_type: Type,
        expr_types: &ExpressionTypeTable,
        by_stmt_id: &mut BTreeMap<BodyStmtId, StatementTypeEntry>,
        issues: &mut Vec<StmtCheckIssue>,
    ) {
        let Some(body) = self.module.bodies.get(&body_id) else {
            return;
        };

        for stmt_id in &body.stmts {
            self.check_stmt(
                *stmt_id,
                expected_return_type.clone(),
                expr_types,
                by_stmt_id,
                issues,
            );
        }
    }

    fn check_stmt(
        &mut self,
        stmt_id: crate::frontend::HirStmtId,
        expected_return_type: Type,
        expr_types: &ExpressionTypeTable,
        by_stmt_id: &mut BTreeMap<BodyStmtId, StatementTypeEntry>,
        issues: &mut Vec<StmtCheckIssue>,
    ) {
        let Some(stmt) = self.module.stmts.get(&stmt_id) else {
            return;
        };
        let stmt_span = stmt.origin.span;
        let entry_id = self.stmt_id(stmt_id);

        match &stmt.kind {
            crate::frontend::HirStmtKind::Let(let_stmt) => {
                if let Some(value) = let_stmt.value {
                    let init_ty =
                        self.expr_type_or_error(value, expr_types, issues);
                    self.apply_binding_type(let_stmt.pat, init_ty, issues);
                }

                by_stmt_id.insert(
                    entry_id,
                    StatementTypeEntry {
                        kind: if let_stmt.mutability == HirMutability::Mutable {
                            StatementKind::Var
                        } else {
                            StatementKind::Let
                        },
                        span: stmt_span,
                        ty: Type::void(),
                    },
                );
            }
            crate::frontend::HirStmtKind::Expr { expr }
            | crate::frontend::HirStmtKind::Semi { expr } => {
                let Some(expression) = self.module.exprs.get(expr) else {
                    by_stmt_id.insert(
                        entry_id,
                        StatementTypeEntry {
                            kind: StatementKind::Expr,
                            span: stmt_span,
                            ty: Type::error(),
                        },
                    );
                    return;
                };

                match &expression.kind {
                    HirExprKind::Return { value } => {
                        let return_ty = value
                            .and_then(|expr_id| {
                                self.expr_type_or_error(
                                    expr_id, expr_types, issues,
                                )
                                .into()
                            })
                            .unwrap_or_else(Type::void);

                        if value.is_none()
                            && expected_return_type != Type::void()
                            && !expected_return_type.is_error()
                        {
                            issues.push(StmtCheckIssue {
                                owner: self.body.owner.clone(),
                                body_index: self.body.body_index,
                                span: stmt_span,
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
                                span: stmt_span,
                                kind:
                                    StmtCheckIssueKind::UnexpectedReturnValue {
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
                                span: stmt_span,
                                kind: StmtCheckIssueKind::ReturnTypeMismatch {
                                    expected: expected_return_type.clone(),
                                    found: return_ty.clone(),
                                },
                            });
                        }

                        by_stmt_id.insert(
                            entry_id,
                            StatementTypeEntry {
                                kind: StatementKind::Return,
                                span: stmt_span,
                                ty: return_ty,
                            },
                        );
                    }
                    HirExprKind::If {
                        condition,
                        then_body,
                        else_expr,
                    } => {
                        self.check_condition_expr(
                            *condition,
                            expr_types,
                            issues,
                            expression.origin.span,
                        );
                        self.check_body(
                            *then_body,
                            expected_return_type.clone(),
                            expr_types,
                            by_stmt_id,
                            issues,
                        );
                        if let Some(else_expr) = else_expr {
                            self.check_else_branch(
                                *else_expr,
                                expected_return_type,
                                expr_types,
                                by_stmt_id,
                                issues,
                            );
                        }

                        let kind = if is_lowered_guard_if(self.module, *expr) {
                            StatementKind::Guard
                        } else {
                            StatementKind::If
                        };
                        by_stmt_id.insert(
                            entry_id,
                            StatementTypeEntry {
                                kind,
                                span: stmt_span,
                                ty: Type::void(),
                            },
                        );
                    }
                    HirExprKind::While { condition, body } => {
                        self.check_condition_expr(
                            *condition,
                            expr_types,
                            issues,
                            expression.origin.span,
                        );
                        self.check_body(
                            *body,
                            expected_return_type,
                            expr_types,
                            by_stmt_id,
                            issues,
                        );
                        by_stmt_id.insert(
                            entry_id,
                            StatementTypeEntry {
                                kind: StatementKind::While,
                                span: stmt_span,
                                ty: Type::void(),
                            },
                        );
                    }
                    HirExprKind::For { body, .. } => {
                        self.check_body(
                            *body,
                            expected_return_type,
                            expr_types,
                            by_stmt_id,
                            issues,
                        );
                        by_stmt_id.insert(
                            entry_id,
                            StatementTypeEntry {
                                kind: StatementKind::For,
                                span: stmt_span,
                                ty: Type::void(),
                            },
                        );
                    }
                    HirExprKind::Break => {
                        by_stmt_id.insert(
                            entry_id,
                            StatementTypeEntry {
                                kind: StatementKind::Break,
                                span: stmt_span,
                                ty: Type::void(),
                            },
                        );
                    }
                    HirExprKind::Continue => {
                        by_stmt_id.insert(
                            entry_id,
                            StatementTypeEntry {
                                kind: StatementKind::Continue,
                                span: stmt_span,
                                ty: Type::void(),
                            },
                        );
                    }
                    HirExprKind::Assign { target, value, .. } => {
                        let expr_ty =
                            self.expr_type_or_error(*expr, expr_types, issues);
                        let target_ty = self
                            .expr_type_or_error(*target, expr_types, issues);
                        let value_ty =
                            self.expr_type_or_error(*value, expr_types, issues);
                        if !target_ty.is_error()
                            && !value_ty.is_error()
                            && target_ty != value_ty
                        {
                            issues.push(StmtCheckIssue {
                                owner: self.body.owner.clone(),
                                body_index: self.body.body_index,
                                span: expression.origin.span,
                                kind:
                                    StmtCheckIssueKind::AssignmentTypeMismatch {
                                        target: target_ty,
                                        value: value_ty,
                                    },
                            });
                        }
                        by_stmt_id.insert(
                            entry_id,
                            StatementTypeEntry {
                                kind: StatementKind::Expr,
                                span: stmt_span,
                                ty: expr_ty,
                            },
                        );
                    }
                    _ => {
                        let expr_ty =
                            self.expr_type_or_error(*expr, expr_types, issues);
                        by_stmt_id.insert(
                            entry_id,
                            StatementTypeEntry {
                                kind: StatementKind::Expr,
                                span: stmt_span,
                                ty: expr_ty,
                            },
                        );
                    }
                }
            }
            crate::frontend::HirStmtKind::Item { .. } => {}
        }
    }

    fn check_else_branch(
        &mut self,
        else_expr: HirExprId,
        expected_return_type: Type,
        expr_types: &ExpressionTypeTable,
        by_stmt_id: &mut BTreeMap<BodyStmtId, StatementTypeEntry>,
        issues: &mut Vec<StmtCheckIssue>,
    ) {
        let Some(expr) = self.module.exprs.get(&else_expr) else {
            return;
        };

        match &expr.kind {
            HirExprKind::If {
                condition,
                then_body,
                else_expr,
            } => {
                self.check_condition_expr(
                    *condition,
                    expr_types,
                    issues,
                    expr.origin.span,
                );
                self.check_body(
                    *then_body,
                    expected_return_type.clone(),
                    expr_types,
                    by_stmt_id,
                    issues,
                );
                if let Some(else_expr) = else_expr {
                    self.check_else_branch(
                        *else_expr,
                        expected_return_type,
                        expr_types,
                        by_stmt_id,
                        issues,
                    );
                }
            }
            HirExprKind::Block { body } => {
                self.check_body(
                    *body,
                    expected_return_type,
                    expr_types,
                    by_stmt_id,
                    issues,
                );
            }
            _ => {}
        }
    }

    fn check_condition_expr(
        &mut self,
        expr_id: HirExprId,
        expr_types: &ExpressionTypeTable,
        issues: &mut Vec<StmtCheckIssue>,
        span: Span,
    ) {
        let found = self.expr_type_or_error(expr_id, expr_types, issues);
        if found != Type::builtin(BuiltinType::Bool) && !found.is_error() {
            issues.push(StmtCheckIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: StmtCheckIssueKind::InvalidConditionType { found },
            });
        }
    }

    fn apply_binding_type(
        &mut self,
        pat_id: HirPatId,
        initializer: Type,
        issues: &mut Vec<StmtCheckIssue>,
    ) {
        let pattern_span = self
            .module
            .patterns
            .get(&pat_id)
            .map(|pattern| pattern.origin.span)
            .unwrap_or_else(|| Span::new(0, 0));

        let Some(local_id) = self.local_for_pattern(pat_id) else {
            issues.push(StmtCheckIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span: pattern_span,
                kind: StmtCheckIssueKind::MissingPatternLocal {
                    span: pattern_span,
                },
            });
            return;
        };

        let current = self
            .local_types
            .get(&local_id)
            .cloned()
            .unwrap_or_else(Type::error);

        if self.declared_type_hir_locals.contains(&local_id) {
            if !current.is_error()
                && !initializer.is_error()
                && current != initializer
            {
                issues.push(StmtCheckIssue {
                    owner: self.body.owner.clone(),
                    body_index: self.body.body_index,
                    span: pattern_span,
                    kind: StmtCheckIssueKind::AnnotatedLocalTypeMismatch {
                        local_id,
                        annotated: current,
                        initializer,
                    },
                });
            }
            return;
        }

        if current.is_error() && !initializer.is_error() {
            self.local_types.insert(local_id, initializer);
        }
    }

    fn local_for_pattern(&self, pat_id: HirPatId) -> Option<LocalId> {
        self.hir_input
            .hir_local_bindings
            .binding_for_pat(self.body_ref.file_id, pat_id)
    }

    fn expr_type_or_error(
        &mut self,
        expr_id: HirExprId,
        expr_types: &ExpressionTypeTable,
        issues: &mut Vec<StmtCheckIssue>,
    ) -> Type {
        if let Some(ty) = expr_types.expr_type_for_hir_expr(
            &self.body.owner,
            self.body.body_index,
            expr_id,
        ) {
            return ty.clone();
        }

        if !self.missing_expr_ids.insert(expr_id) {
            return Type::error();
        }

        let span = self
            .module
            .exprs
            .get(&expr_id)
            .map(|expr| expr.origin.span)
            .unwrap_or_else(|| Span::new(0, 0));
        issues.push(StmtCheckIssue {
            owner: self.body.owner.clone(),
            body_index: self.body.body_index,
            span,
            kind: StmtCheckIssueKind::MissingExpressionType { span },
        });
        Type::error()
    }
}

fn is_lowered_guard_if(module: &HirModule, expr_id: HirExprId) -> bool {
    let Some(expr) = module.exprs.get(&expr_id) else {
        return false;
    };
    let HirExprKind::If {
        then_body,
        else_expr,
        ..
    } = expr.kind
    else {
        return false;
    };
    let Some(then_body_node) = module.bodies.get(&then_body) else {
        return false;
    };
    if !then_body_node.stmts.is_empty() || then_body_node.tail_expr.is_some() {
        return false;
    }
    let Some(else_expr_id) = else_expr else {
        return false;
    };
    matches!(
        module.exprs.get(&else_expr_id).map(|expr| &expr.kind),
        Some(HirExprKind::Block { .. })
    )
}
