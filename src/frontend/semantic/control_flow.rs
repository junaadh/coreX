use super::body_env::BodyTypeEnvironmentTable;
use super::hir_input::SemanticHirInput;
use super::{BuiltinType, Type, TypedItemTable};
use crate::frontend::ast::Span;
use crate::frontend::hir::HirBodyId;
use crate::frontend::resolver::{DeclarationOwner, ResolvedBodyTable};
use crate::midend::type_check::{
    BodyStmtId, ExprCheckIssue, ExprCheckIssueKind, ExpressionTypeTable,
    StatementKind, StatementTypeEntry, StatementTypeTable,
    check_expression_types, check_statements_with_expression_types,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BodyControlFlowId {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub hir_body_id: HirBodyId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyControlFlowResult {
    pub id: BodyControlFlowId,
    pub expected_return_type: Type,
    pub block_result_type: Type,
    pub has_tail_expression: bool,
    pub return_count: usize,
    pub is_compatible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlFlowIssueKind {
    MissingBodyAst,
    MissingBodyEnvironment,
    MissingBlockResultType,
    MissingReturnValue { expected: Type },
    UnexpectedReturnValue { found: Type },
    ReturnTypeMismatch { expected: Type, found: Type },
    MissingTailExpression { expected: Type },
    TailTypeMismatch { expected: Type, found: Type },
    UnexpectedTailValue { found: Type },
    IfBranchTypeMismatch { then_type: Type, else_type: Type },
    MissingElseBranch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowIssue {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub span: Span,
    pub kind: ControlFlowIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowTable {
    by_body: BTreeMap<BodyControlFlowId, BodyControlFlowResult>,
    pub issues: Vec<ControlFlowIssue>,
}

impl ControlFlowTable {
    #[must_use]
    pub fn body(
        &self,
        id: &BodyControlFlowId,
    ) -> Option<&BodyControlFlowResult> {
        self.by_body.get(id)
    }

    #[must_use]
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&BodyControlFlowId, &BodyControlFlowResult)>
    {
        self.by_body.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_body.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_body.is_empty()
    }
}

#[must_use]
pub fn check_control_flow(
    hir_input: &SemanticHirInput,
    typed_items: &TypedItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
) -> ControlFlowTable {
    let expr_types = check_expression_types(
        hir_input,
        typed_items,
        resolved_bodies,
        body_envs,
    );
    let stmt_types = check_statements_with_expression_types(
        hir_input,
        resolved_bodies,
        body_envs,
        &expr_types,
    );
    check_control_flow_with_tables(
        hir_input,
        body_envs,
        &expr_types,
        &stmt_types,
    )
}

#[must_use]
pub fn check_control_flow_with_tables(
    hir_input: &SemanticHirInput,
    body_envs: &BodyTypeEnvironmentTable,
    expr_types: &ExpressionTypeTable,
    stmt_types: &StatementTypeTable,
) -> ControlFlowTable {
    let expr_issues_by_body = index_expr_issues_by_body(&expr_types.issues);

    let mut by_body = BTreeMap::new();
    let mut issues = Vec::new();

    let mut envs = body_envs.iter().collect::<Vec<_>>();
    envs.sort_by(|lhs, rhs| {
        lhs.owner
            .cmp(&rhs.owner)
            .then(lhs.body_index.cmp(&rhs.body_index))
    });

    for env in envs {
        let Some(body_ref) = hir_input.body_ref(&env.owner, env.body_index)
        else {
            issues.push(ControlFlowIssue {
                owner: env.owner.clone(),
                body_index: env.body_index,
                span: Span::new(0, 0),
                kind: ControlFlowIssueKind::MissingBodyAst,
            });
            continue;
        };

        let Some(module) = hir_input.hir_modules.get(&body_ref.file_id) else {
            issues.push(ControlFlowIssue {
                owner: env.owner.clone(),
                body_index: env.body_index,
                span: Span::new(0, 0),
                kind: ControlFlowIssueKind::MissingBodyAst,
            });
            continue;
        };

        let Some(hir_body) = module.bodies.get(&body_ref.body_id) else {
            issues.push(ControlFlowIssue {
                owner: env.owner.clone(),
                body_index: env.body_index,
                span: Span::new(0, 0),
                kind: ControlFlowIssueKind::MissingBodyAst,
            });
            continue;
        };

        let body_id = BodyControlFlowId {
            owner: env.owner.clone(),
            body_index: env.body_index,
            hir_body_id: body_ref.body_id,
        };
        let mut is_compatible = true;
        let expected = env.expected_return_type.clone();
        let issue_span = issue_span_for_body(module, hir_body);
        let tail_span = tail_span_for_body(module, hir_body);
        let has_tail_expression = hir_body.tail_expr.is_some();

        let block_result_type = if let Some(root_type) =
            expr_types.root_type(&env.owner, env.body_index)
        {
            root_type.clone()
        } else {
            issues.push(ControlFlowIssue {
                owner: env.owner.clone(),
                body_index: env.body_index,
                span: issue_span,
                kind: ControlFlowIssueKind::MissingBlockResultType,
            });
            is_compatible = false;
            Type::error()
        };

        let mut return_count = 0usize;
        for hir_stmt_id in &hir_body.stmts {
            let stmt_id = BodyStmtId {
                owner: env.owner.clone(),
                body_index: env.body_index,
                hir_stmt_id: *hir_stmt_id,
            };
            let Some(stmt_entry) = stmt_types.stmt_entry(&stmt_id) else {
                continue;
            };
            if stmt_entry.kind != StatementKind::Return {
                continue;
            }
            return_count = return_count.saturating_add(1);
            if !check_return_statement(
                &env.owner,
                env.body_index,
                stmt_entry,
                &expected,
                &mut issues,
            ) {
                is_compatible = false;
            }
        }

        if has_tail_expression {
            if expected == Type::void() {
                if !is_type_compatible(&expected, &block_result_type) {
                    issues.push(ControlFlowIssue {
                        owner: env.owner.clone(),
                        body_index: env.body_index,
                        span: tail_span,
                        kind: ControlFlowIssueKind::UnexpectedTailValue {
                            found: block_result_type.clone(),
                        },
                    });
                    is_compatible = false;
                }
            } else if !is_type_compatible(&expected, &block_result_type) {
                issues.push(ControlFlowIssue {
                    owner: env.owner.clone(),
                    body_index: env.body_index,
                    span: tail_span,
                    kind: ControlFlowIssueKind::TailTypeMismatch {
                        expected: expected.clone(),
                        found: block_result_type.clone(),
                    },
                });
                is_compatible = false;
            }
        } else if expected != Type::void() && return_count == 0 {
            issues.push(ControlFlowIssue {
                owner: env.owner.clone(),
                body_index: env.body_index,
                span: issue_span,
                kind: ControlFlowIssueKind::MissingTailExpression {
                    expected: expected.clone(),
                },
            });
            is_compatible = false;
        }

        for expr_issue in expr_issues_by_body
            .get(&(env.owner.clone(), env.body_index))
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            if !map_if_branch_issue(
                &env.owner,
                env.body_index,
                expr_issue,
                &mut issues,
            ) {
                continue;
            }
            is_compatible = false;
        }

        let result = BodyControlFlowResult {
            id: body_id.clone(),
            expected_return_type: expected,
            block_result_type,
            has_tail_expression,
            return_count,
            is_compatible,
        };
        by_body.insert(body_id, result);
    }

    ControlFlowTable { by_body, issues }
}

fn tail_span_for_body(
    module: &crate::frontend::HirModule,
    body: &crate::frontend::HirBody,
) -> Span {
    body.tail_expr
        .and_then(|tail_expr| {
            module.exprs.get(&tail_expr).map(|expr| expr.origin.span)
        })
        .unwrap_or_else(|| Span::new(0, 0))
}

fn issue_span_for_body(
    module: &crate::frontend::HirModule,
    body: &crate::frontend::HirBody,
) -> Span {
    body.tail_expr
        .and_then(|tail_expr| {
            module.exprs.get(&tail_expr).map(|expr| expr.origin.span)
        })
        .or_else(|| {
            body.stmts.last().and_then(|stmt_id| {
                module.stmts.get(stmt_id).map(|stmt| stmt.origin.span)
            })
        })
        .unwrap_or_else(|| body.origin.span)
}

fn check_return_statement(
    owner: &DeclarationOwner,
    body_index: usize,
    stmt_entry: &StatementTypeEntry,
    expected: &Type,
    issues: &mut Vec<ControlFlowIssue>,
) -> bool {
    let found = &stmt_entry.ty;
    if expected == &Type::void() {
        if found != &Type::void() && !is_type_compatible(expected, found) {
            issues.push(ControlFlowIssue {
                owner: owner.clone(),
                body_index,
                span: stmt_entry.span,
                kind: ControlFlowIssueKind::UnexpectedReturnValue {
                    found: found.clone(),
                },
            });
            return false;
        }
        return true;
    }

    if found == &Type::void() {
        issues.push(ControlFlowIssue {
            owner: owner.clone(),
            body_index,
            span: stmt_entry.span,
            kind: ControlFlowIssueKind::MissingReturnValue {
                expected: expected.clone(),
            },
        });
        return false;
    }

    if !is_type_compatible(expected, found) {
        issues.push(ControlFlowIssue {
            owner: owner.clone(),
            body_index,
            span: stmt_entry.span,
            kind: ControlFlowIssueKind::ReturnTypeMismatch {
                expected: expected.clone(),
                found: found.clone(),
            },
        });
        return false;
    }

    true
}

fn is_type_compatible(expected: &Type, found: &Type) -> bool {
    expected == found
        || expected.is_error()
        || found.is_error()
        || *found == Type::builtin(BuiltinType::Never)
}

fn index_expr_issues_by_body(
    expr_issues: &[ExprCheckIssue],
) -> BTreeMap<(DeclarationOwner, usize), Vec<&ExprCheckIssue>> {
    let mut grouped = BTreeMap::new();
    for issue in expr_issues {
        grouped
            .entry((issue.owner.clone(), issue.body_index))
            .or_insert_with(Vec::new)
            .push(issue);
    }
    grouped
}

fn map_if_branch_issue(
    owner: &DeclarationOwner,
    body_index: usize,
    expr_issue: &ExprCheckIssue,
    issues: &mut Vec<ControlFlowIssue>,
) -> bool {
    match &expr_issue.kind {
        ExprCheckIssueKind::IncompatibleIfBranches {
            then_type,
            else_type,
        } => {
            issues.push(ControlFlowIssue {
                owner: owner.clone(),
                body_index,
                span: expr_issue.span,
                kind: ControlFlowIssueKind::IfBranchTypeMismatch {
                    then_type: then_type.clone(),
                    else_type: else_type.clone(),
                },
            });
            true
        }
        ExprCheckIssueKind::MissingElseBranch => {
            issues.push(ControlFlowIssue {
                owner: owner.clone(),
                body_index,
                span: expr_issue.span,
                kind: ControlFlowIssueKind::MissingElseBranch,
            });
            true
        }
        _ => false,
    }
}
