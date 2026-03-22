use super::signatures::TypedFunctionSignature;
use super::{BuiltinType, Type};
use crate::frontend::ast::Span;
use crate::frontend::hir::{
    HirArrayElement, HirBinaryOp, HirBodyId, HirExprId, HirExprKind,
    HirLiteral, HirModule, HirMutability, HirPatId, HirPatKind,
    HirStructExprField, HirUnaryOp,
};
use crate::frontend::resolver::{
    DeclarationOwner, ImportBindingKind, ItemId, LocalId, LocalMutability,
    ResolvedBody, ResolvedBodyTable, ResolvedImports,
};
use crate::frontend::semantic::body_env::BodyTypeEnvironmentTable;
use crate::frontend::semantic::external_lookup::ExternalSemanticLookup;
use crate::frontend::semantic::hir_input::{SemanticBodyRef, SemanticHirInput};
use crate::frontend::semantic::{TypedItemData, TypedItemTable};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BodyExprId {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub hir_expr_id: HirExprId,
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
    types_by_hir_expr_id: BTreeMap<(DeclarationOwner, usize, HirExprId), Type>,
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
    pub fn expr_type_for_hir_expr(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        expr_id: HirExprId,
    ) -> Option<&Type> {
        self.types_by_hir_expr_id
            .get(&(owner.clone(), body_index, expr_id))
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
    hir_input: &SemanticHirInput,
    typed_items: &TypedItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
) -> ExpressionTypeTable {
    let empty_imports = BTreeMap::new();
    let empty_external_lookup = ExternalSemanticLookup::default();
    check_expression_types_with_external_lookup(
        hir_input,
        typed_items,
        resolved_bodies,
        body_envs,
        &empty_imports,
        &empty_external_lookup,
    )
}

#[must_use]
pub fn check_expression_types_with_external_lookup(
    hir_input: &SemanticHirInput,
    typed_items: &TypedItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
    imports: &BTreeMap<FileId, ResolvedImports>,
    external_lookup: &ExternalSemanticLookup,
) -> ExpressionTypeTable {
    let mut types_by_expr_id = BTreeMap::new();
    let mut types_by_hir_expr_id = BTreeMap::new();
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

        let Some(body_ref) = hir_input.body_ref(&body.owner, body.body_index)
        else {
            issues.push(ExprCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: ExprCheckIssueKind::MissingBodyAst,
            });
            continue;
        };

        let Some(module) = hir_input.hir_modules.get(&body_ref.file_id) else {
            issues.push(ExprCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: ExprCheckIssueKind::MissingBodyAst,
            });
            continue;
        };

        if !module.bodies.contains_key(&body_ref.body_id) {
            issues.push(ExprCheckIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: ExprCheckIssueKind::MissingBodyAst,
            });
            continue;
        }

        let mut checker = BodyExprChecker::new(
            body,
            body_ref,
            module,
            hir_input,
            env.local_types.clone(),
            env.local_bindings.clone(),
            imports,
            external_lookup,
        );
        let root_type = checker.check_body(
            body_ref.body_id,
            typed_items,
            &mut issues,
            &mut types_by_expr_id,
            &mut types_by_hir_expr_id,
        );

        for (key, ids) in checker.expr_ids_by_span {
            expr_ids_by_span.entry(key).or_default().extend(ids);
        }
        root_types_by_body
            .insert((body.owner.clone(), body.body_index), root_type);
    }

    ExpressionTypeTable {
        types_by_expr_id,
        types_by_hir_expr_id,
        expr_ids_by_span,
        root_types_by_body,
        issues,
    }
}

struct BodyExprChecker<'a> {
    body: &'a ResolvedBody,
    body_ref: SemanticBodyRef,
    module: &'a HirModule,
    hir_input: &'a SemanticHirInput,
    local_types: BTreeMap<LocalId, Type>,
    local_bindings: BTreeMap<
        LocalId,
        crate::frontend::semantic::body_env::BodyLocalBindingInfo,
    >,
    imports: &'a BTreeMap<FileId, ResolvedImports>,
    external_lookup: &'a ExternalSemanticLookup,
    expr_ids_by_span:
        BTreeMap<(DeclarationOwner, usize, usize, usize), Vec<BodyExprId>>,
}

impl<'a> BodyExprChecker<'a> {
    fn new(
        body: &'a ResolvedBody,
        body_ref: SemanticBodyRef,
        module: &'a HirModule,
        hir_input: &'a SemanticHirInput,
        local_types: BTreeMap<LocalId, Type>,
        local_bindings: BTreeMap<
            LocalId,
            crate::frontend::semantic::body_env::BodyLocalBindingInfo,
        >,
        imports: &'a BTreeMap<FileId, ResolvedImports>,
        external_lookup: &'a ExternalSemanticLookup,
    ) -> Self {
        Self {
            body,
            body_ref,
            module,
            hir_input,
            local_types,
            local_bindings,
            imports,
            external_lookup,
            expr_ids_by_span: BTreeMap::new(),
        }
    }

    fn branch_checker(&self) -> Self {
        Self {
            body: self.body,
            body_ref: self.body_ref,
            module: self.module,
            hir_input: self.hir_input,
            local_types: self.local_types.clone(),
            local_bindings: self.local_bindings.clone(),
            imports: self.imports,
            external_lookup: self.external_lookup,
            expr_ids_by_span: BTreeMap::new(),
        }
    }

    fn merge_branch(&mut self, branch: Self) {
        for (key, ids) in branch.expr_ids_by_span {
            self.expr_ids_by_span.entry(key).or_default().extend(ids);
        }
    }

    fn check_body(
        &mut self,
        body_id: HirBodyId,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
        types_by_expr_id: &mut BTreeMap<BodyExprId, Type>,
        types_by_hir_expr_id: &mut BTreeMap<
            (DeclarationOwner, usize, HirExprId),
            Type,
        >,
    ) -> Type {
        let Some(body) = self.module.bodies.get(&body_id) else {
            return Type::error();
        };

        for stmt_id in &body.stmts {
            self.check_stmt(
                *stmt_id,
                typed_items,
                issues,
                types_by_expr_id,
                types_by_hir_expr_id,
            );
        }

        if let Some(tail_expr) = body.tail_expr {
            self.check_expr(
                tail_expr,
                typed_items,
                issues,
                types_by_expr_id,
                types_by_hir_expr_id,
            )
        } else {
            Type::void()
        }
    }

    fn check_stmt(
        &mut self,
        stmt_id: crate::frontend::HirStmtId,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
        types_by_expr_id: &mut BTreeMap<BodyExprId, Type>,
        types_by_hir_expr_id: &mut BTreeMap<
            (DeclarationOwner, usize, HirExprId),
            Type,
        >,
    ) {
        let Some(stmt) = self.module.stmts.get(&stmt_id) else {
            return;
        };

        match &stmt.kind {
            crate::frontend::HirStmtKind::Let(let_stmt) => {
                let value_type = let_stmt
                    .value
                    .map(|value| {
                        self.check_expr(
                            value,
                            typed_items,
                            issues,
                            types_by_expr_id,
                            types_by_hir_expr_id,
                        )
                    })
                    .unwrap_or_else(Type::void);
                self.infer_local_from_pattern(
                    let_stmt.pat,
                    value_type,
                    let_stmt.mutability == HirMutability::Mutable,
                );
            }
            crate::frontend::HirStmtKind::Expr { expr }
            | crate::frontend::HirStmtKind::Semi { expr } => {
                self.check_expr(
                    *expr,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
            }
            crate::frontend::HirStmtKind::Item { .. } => {}
        }
    }

    fn check_expr(
        &mut self,
        expr_id: HirExprId,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
        types_by_expr_id: &mut BTreeMap<BodyExprId, Type>,
        types_by_hir_expr_id: &mut BTreeMap<
            (DeclarationOwner, usize, HirExprId),
            Type,
        >,
    ) -> Type {
        let body_expr_id = BodyExprId {
            owner: self.body.owner.clone(),
            body_index: self.body.body_index,
            hir_expr_id: expr_id,
        };

        let Some(expr) = self.module.exprs.get(&expr_id) else {
            return self.store_and_return(
                body_expr_id,
                Span::new(0, 0),
                Type::error(),
                types_by_expr_id,
                types_by_hir_expr_id,
            );
        };

        let span = expr.origin.span;
        let ty = match &expr.kind {
            HirExprKind::Literal(HirLiteral::Integer(_)) => {
                Type::builtin(BuiltinType::I32)
            }
            HirExprKind::Literal(HirLiteral::Float(_)) => {
                Type::builtin(BuiltinType::F64)
            }
            HirExprKind::Literal(HirLiteral::Char(_)) => {
                Type::builtin(BuiltinType::Char)
            }
            HirExprKind::Literal(HirLiteral::Boolean(_)) => {
                Type::builtin(BuiltinType::Bool)
            }
            HirExprKind::Literal(HirLiteral::String(_)) => {
                Type::builtin(BuiltinType::String)
            }
            HirExprKind::Path(path) => self.type_for_path_reference(
                expr_id,
                span,
                path.segments.clone(),
                typed_items,
                issues,
            ),
            HirExprKind::NamespaceField { .. } => {
                let path =
                    match Self::extract_namespace_path(self.module, expr_id) {
                        Some(path) => path,
                        None => {
                            return self.store_and_return(
                                body_expr_id,
                                span,
                                Type::error(),
                                types_by_expr_id,
                                types_by_hir_expr_id,
                            );
                        }
                    };
                self.type_for_path_reference(
                    expr_id,
                    span,
                    path,
                    typed_items,
                    issues,
                )
            }
            HirExprKind::Unary { op, expr: inner } => {
                let operand = self.check_expr(
                    *inner,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                self.type_unary(span, *op, operand, issues)
            }
            HirExprKind::Binary { op, lhs, rhs } => {
                let lhs_ty = self.check_expr(
                    *lhs,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                let rhs_ty = self.check_expr(
                    *rhs,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                self.type_binary(span, *op, lhs_ty, rhs_ty, issues)
            }
            HirExprKind::Assign { target, value, .. } => {
                let target_ty = self.check_expr(
                    *target,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                let value_ty = self.check_expr(
                    *value,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );

                match self.assignment_target_status(*target) {
                    AssignmentTargetStatus::MutableLocalOrNonPath => {}
                    AssignmentTargetStatus::ImmutableLocal(local_id) => {
                        issues.push(ExprCheckIssue {
                            owner: self.body.owner.clone(),
                            body_index: self.body.body_index,
                            span,
                            kind: ExprCheckIssueKind::MutabilityViolation {
                                local_id,
                            },
                        });
                        return self.store_and_return(
                            body_expr_id,
                            span,
                            Type::error(),
                            types_by_expr_id,
                            types_by_hir_expr_id,
                        );
                    }
                    AssignmentTargetStatus::Invalid => {
                        issues.push(ExprCheckIssue {
                            owner: self.body.owner.clone(),
                            body_index: self.body.body_index,
                            span,
                            kind: ExprCheckIssueKind::InvalidAssignmentTarget,
                        });
                        return self.store_and_return(
                            body_expr_id,
                            span,
                            Type::error(),
                            types_by_expr_id,
                            types_by_hir_expr_id,
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
                        span,
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
            HirExprKind::Call { callee, args } => {
                let callee_sig =
                    self.call_signature_for_callee(*callee, typed_items);
                let _ = self.check_expr(
                    *callee,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );

                let mut arg_types = Vec::with_capacity(args.len());
                for arg in args {
                    arg_types.push(self.check_expr(
                        arg.value,
                        typed_items,
                        issues,
                        types_by_expr_id,
                        types_by_hir_expr_id,
                    ));
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
                            span,
                            kind: ExprCheckIssueKind::BareExternFunctionCall {
                                function,
                                namespace,
                            },
                        });
                        return self.store_and_return(
                            body_expr_id,
                            span,
                            Type::error(),
                            types_by_expr_id,
                            types_by_hir_expr_id,
                        );
                    }
                    CallSignatureResolution::Missing => {
                        // Check if this is a call to a local variable
                        let is_local_variable_call = self
                            .hir_input
                            .hir_path_table
                            .by_expr(self.body_ref.file_id, *callee)
                            .and_then(|resolution| match resolution {
                                crate::frontend::resolver::HirPathResolution::Local(_) => {
                                    Some(true)
                                }
                                _ => Some(false),
                            })
                            .unwrap_or(false);

                        // Report error for local variable calls, but allow single-segment
                        // paths (like enum cases) through to type inference
                        if is_local_variable_call {
                            issues.push(ExprCheckIssue {
                                owner: self.body.owner.clone(),
                                body_index: self.body.body_index,
                                span,
                                kind: ExprCheckIssueKind::InvalidCallCallee,
                            });
                        }
                        return self.store_and_return(
                            body_expr_id,
                            span,
                            Type::error(),
                            types_by_expr_id,
                            types_by_hir_expr_id,
                        );
                    }
                };

                if signature.param_types.len() != arg_types.len() {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
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
                                span,
                                kind: ExprCheckIssueKind::CallArgTypeMismatch {
                                    index,
                                    expected: expected.clone(),
                                    found: actual.clone(),
                                },
                            });
                            return self.store_and_return(
                                body_expr_id,
                                span,
                                Type::error(),
                                types_by_expr_id,
                                types_by_hir_expr_id,
                            );
                        }
                    }

                    signature.return_type.clone().unwrap_or_else(Type::void)
                }
            }
            HirExprKind::Block { body } => self.check_body(
                *body,
                typed_items,
                issues,
                types_by_expr_id,
                types_by_hir_expr_id,
            ),
            HirExprKind::If {
                condition,
                then_body,
                else_expr,
            } => {
                let condition_ty = self.check_expr(
                    *condition,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                if condition_ty != Type::builtin(BuiltinType::Bool)
                    && !condition_ty.is_error()
                {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: ExprCheckIssueKind::InvalidBinaryOp,
                    });
                }

                let mut then_checker = self.branch_checker();
                let then_ty = then_checker.check_body(
                    *then_body,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                self.merge_branch(then_checker);

                let Some(else_expr_id) = else_expr else {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: ExprCheckIssueKind::MissingElseBranch,
                    });
                    return self.store_and_return(
                        body_expr_id,
                        span,
                        Type::error(),
                        types_by_expr_id,
                        types_by_hir_expr_id,
                    );
                };

                let mut else_checker = self.branch_checker();
                let else_ty = else_checker.check_expr(
                    *else_expr_id,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                self.merge_branch(else_checker);

                if then_ty == else_ty {
                    then_ty
                } else if then_ty.is_error() || else_ty.is_error() {
                    Type::error()
                } else {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: ExprCheckIssueKind::IncompatibleIfBranches {
                            then_type: then_ty,
                            else_type: else_ty,
                        },
                    });
                    Type::error()
                }
            }
            HirExprKind::Match { subject, arms } => {
                let _ = self.check_expr(
                    *subject,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                let mut arm_types = Vec::new();
                for arm in arms {
                    arm_types.push(self.check_expr(
                        arm.expr,
                        typed_items,
                        issues,
                        types_by_expr_id,
                        types_by_hir_expr_id,
                    ));
                }
                if arm_types.is_empty() {
                    Type::void()
                } else if arm_types.windows(2).all(|pair| pair[0] == pair[1]) {
                    arm_types[0].clone()
                } else {
                    Type::error()
                }
            }
            HirExprKind::Array { elements } => {
                for element in elements {
                    match element {
                        HirArrayElement::Expr(value)
                        | HirArrayElement::Spread(value) => {
                            self.check_expr(
                                *value,
                                typed_items,
                                issues,
                                types_by_expr_id,
                                types_by_hir_expr_id,
                            );
                        }
                    }
                }
                Type::error()
            }
            HirExprKind::Struct { fields, .. } => {
                for field in fields {
                    match field {
                        HirStructExprField::Named { value, .. }
                        | HirStructExprField::Spread { value } => {
                            self.check_expr(
                                *value,
                                typed_items,
                                issues,
                                types_by_expr_id,
                                types_by_hir_expr_id,
                            );
                        }
                    }
                }
                Type::error()
            }
            HirExprKind::Tuple { elements } => {
                for element in elements {
                    self.check_expr(
                        *element,
                        typed_items,
                        issues,
                        types_by_expr_id,
                        types_by_hir_expr_id,
                    );
                }
                Type::error()
            }
            HirExprKind::While { condition, body } => {
                let condition_ty = self.check_expr(
                    *condition,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                if condition_ty != Type::builtin(BuiltinType::Bool)
                    && !condition_ty.is_error()
                {
                    issues.push(ExprCheckIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span,
                        kind: ExprCheckIssueKind::InvalidBinaryOp,
                    });
                }
                self.check_body(
                    *body,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                Type::void()
            }
            HirExprKind::For { iterator, body, .. } => {
                self.check_expr(
                    *iterator,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                self.check_body(
                    *body,
                    typed_items,
                    issues,
                    types_by_expr_id,
                    types_by_hir_expr_id,
                );
                Type::void()
            }
            HirExprKind::Return { value } => {
                if let Some(value) = value {
                    self.check_expr(
                        *value,
                        typed_items,
                        issues,
                        types_by_expr_id,
                        types_by_hir_expr_id,
                    );
                }
                Type::builtin(BuiltinType::Never)
            }
            HirExprKind::Try { expr: inner }
            | HirExprKind::ForceUnwrap { expr: inner }
            | HirExprKind::Spread { expr: inner } => self.check_expr(
                *inner,
                typed_items,
                issues,
                types_by_expr_id,
                types_by_hir_expr_id,
            ),
            HirExprKind::Break | HirExprKind::Continue => {
                Type::builtin(BuiltinType::Never)
            }
            HirExprKind::Field { .. }
            | HirExprKind::OptionalField { .. }
            | HirExprKind::MethodCall { .. }
            | HirExprKind::Index { .. }
            | HirExprKind::OptionalIndex { .. }
            | HirExprKind::Closure { .. }
            | HirExprKind::Cast { .. }
            | HirExprKind::Range { .. } => Type::error(),
        };

        self.store_and_return(
            body_expr_id,
            span,
            ty,
            types_by_expr_id,
            types_by_hir_expr_id,
        )
    }

    fn store_and_return(
        &mut self,
        body_expr_id: BodyExprId,
        span: Span,
        ty: Type,
        types_by_expr_id: &mut BTreeMap<BodyExprId, Type>,
        types_by_hir_expr_id: &mut BTreeMap<
            (DeclarationOwner, usize, HirExprId),
            Type,
        >,
    ) -> Type {
        let key = (
            body_expr_id.owner.clone(),
            body_expr_id.body_index,
            span.start,
            span.end,
        );
        self.expr_ids_by_span
            .entry(key)
            .or_default()
            .push(body_expr_id.clone());
        types_by_expr_id.insert(body_expr_id.clone(), ty.clone());
        types_by_hir_expr_id.insert(
            (
                body_expr_id.owner.clone(),
                body_expr_id.body_index,
                body_expr_id.hir_expr_id,
            ),
            ty.clone(),
        );
        ty
    }

    fn infer_local_from_pattern(
        &mut self,
        pat_id: HirPatId,
        inferred_type: Type,
        requires_mutable: bool,
    ) {
        let Some(pattern) = self.module.patterns.get(&pat_id) else {
            return;
        };
        let HirPatKind::Binding { .. } = pattern.kind else {
            return;
        };

        if inferred_type.is_error() {
            return;
        }

        let Some(hir_local_id) = self
            .hir_input
            .hir_local_bindings
            .binding_for_pat(self.body_ref.file_id, pat_id)
        else {
            return;
        };
        if requires_mutable {
            let is_mutable =
                self.local_bindings.get(&hir_local_id).is_some_and(|local| {
                    local.mutability == LocalMutability::Mutable
                });
            if !is_mutable {
                return;
            }
        }

        let current = self
            .local_types
            .get(&hir_local_id)
            .cloned()
            .unwrap_or_else(Type::error);
        if current.is_error() {
            self.local_types.insert(hir_local_id, inferred_type);
        }
    }

    fn type_for_path_reference(
        &self,
        expr_id: HirExprId,
        span: Span,
        segments: Vec<String>,
        typed_items: &TypedItemTable,
        issues: &mut Vec<ExprCheckIssue>,
    ) -> Type {
        if let Some(resolution) = self
            .hir_input
            .hir_path_table
            .by_expr(self.body_ref.file_id, expr_id)
        {
            match resolution {
                crate::frontend::resolver::HirPathResolution::Local(
                    hir_local_id,
                ) => {
                    return self
                        .local_types
                        .get(&hir_local_id)
                        .cloned()
                        .unwrap_or_else(|| {
                            issues.push(ExprCheckIssue {
                                owner: self.body.owner.clone(),
                                body_index: self.body.body_index,
                                span,
                                kind: ExprCheckIssueKind::MissingLocalType {
                                    local_id: hir_local_id,
                                },
                            });
                            Type::error()
                        });
                }
                crate::frontend::resolver::HirPathResolution::Item(
                    item_ref,
                ) => {
                    if let Some(item_id) = self
                        .hir_input
                        .item_id_by_hir_item_ref
                        .get(&item_ref)
                        .copied()
                    {
                        return self.type_for_item_reference(
                            span,
                            item_id,
                            typed_items,
                            issues,
                        );
                    }
                }
                crate::frontend::resolver::HirPathResolution::AssociatedMember {
                    type_item_ref,
                    ..
                } => {
                    if let Some(item_id) = self
                        .hir_input
                        .item_id_by_hir_item_ref
                        .get(&type_item_ref)
                        .copied()
                    {
                        return self.type_for_item_reference(
                            span,
                            item_id,
                            typed_items,
                            issues,
                        );
                    }
                }
            }
        }

        if let Some(item_id) = self.resolve_item_id_from_hir_path(&segments) {
            return self.type_for_item_reference(
                span,
                item_id,
                typed_items,
                issues,
            );
        }
        if self.external_import_signature_for_path(&segments).is_some()
            || self
                .direct_named_root_signature_for_path(&segments)
                .is_some()
            || self.extern_signature_for_path(&segments).is_some()
        {
            return Type::error();
        }

        // Single-segment paths without resolution might be enum cases
        // Type inference will handle them via scope search
        if segments.len() == 1 {
            // Return error without adding issue - let type inference handle it
            return Type::error();
        }

        issues.push(ExprCheckIssue {
            owner: self.body.owner.clone(),
            body_index: self.body.body_index,
            span,
            kind: ExprCheckIssueKind::MissingResolvedReference { segments },
        });
        Type::error()
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
            | Some(TypedItemData::Protocol(_))
            | Some(TypedItemData::Function(_)) => Type::error(),
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
        callee_expr_id: HirExprId,
        typed_items: &TypedItemTable,
    ) -> CallSignatureResolution {
        let Some(path) =
            Self::extract_namespace_path(self.module, callee_expr_id)
        else {
            return CallSignatureResolution::Missing;
        };

        if let Some(signature) =
            self.local_signature_for_callee(callee_expr_id, &path, typed_items)
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
        callee_expr_id: HirExprId,
        path: &[String],
        typed_items: &TypedItemTable,
    ) -> Option<TypedFunctionSignature> {
        if let Some(resolution) = self
            .hir_input
            .hir_path_table
            .by_expr(self.body_ref.file_id, callee_expr_id)
        {
            match resolution {
                crate::frontend::resolver::HirPathResolution::Item(item_ref) => {
                    if let Some(item_id) = self
                        .hir_input
                        .item_id_by_hir_item_ref
                        .get(&item_ref)
                        .copied()
                        && let Some(signature) = typed_items.function(item_id)
                    {
                        return Some(signature.clone());
                    }
                }
                crate::frontend::resolver::HirPathResolution::AssociatedMember {
                    type_item_ref,
                    member_name,
                    member_kind,
                } => {
                    if let Some(signature) =
                        self.associated_member_signature_for_type_item(
                            type_item_ref,
                            &member_name,
                            member_kind,
                            typed_items,
                        )
                    {
                        return Some(signature);
                    }
                }
                crate::frontend::resolver::HirPathResolution::Local(_) => {}
            }
        }

        if let Some(item_id) = self.resolve_item_id_from_hir_path(path)
            && let Some(signature) = typed_items.function(item_id)
        {
            return Some(signature.clone());
        }

        self.initializer_signature_for_path(path, typed_items)
    }

    fn initializer_signature_for_path(
        &self,
        path: &[String],
        typed_items: &TypedItemTable,
    ) -> Option<TypedFunctionSignature> {
        if path.len() < 2 || path.last().is_none_or(|segment| segment != "init")
        {
            return None;
        }

        let owner_path = &path[..path.len() - 1];
        let owner_item_id = self.resolve_item_id_from_hir_path(owner_path)?;
        match typed_items.get(owner_item_id)? {
            TypedItemData::Struct(signature_data) => {
                signature_data.initializer_signatures.first().cloned()
            }
            TypedItemData::Enum(signature_data) => {
                signature_data.initializer_signatures.first().cloned()
            }
            TypedItemData::Protocol(signature_data) => {
                signature_data.initializer_signatures.first().cloned()
            }
            TypedItemData::Function(_) => None,
        }
    }

    fn associated_member_signature_for_type_item(
        &self,
        type_item_ref: crate::frontend::resolver::HirItemRef,
        member_name: &str,
        member_kind: crate::frontend::resolver::AssociatedMemberKind,
        typed_items: &TypedItemTable,
    ) -> Option<TypedFunctionSignature> {
        let type_item_id = self
            .hir_input
            .item_id_by_hir_item_ref
            .get(&type_item_ref)
            .copied()?;
        self.associated_member_signature_for_item(
            type_item_id,
            member_name,
            member_kind,
            typed_items,
        )
    }

    fn associated_member_signature_for_item(
        &self,
        type_item_id: ItemId,
        member_name: &str,
        member_kind: crate::frontend::resolver::AssociatedMemberKind,
        typed_items: &TypedItemTable,
    ) -> Option<TypedFunctionSignature> {
        let direct_signature = match typed_items.get(type_item_id)? {
            TypedItemData::Struct(signature_data) => {
                Self::select_associated_member_signature(
                    &signature_data.method_signatures,
                    &signature_data.initializer_signatures,
                    member_name,
                    member_kind,
                )
            }
            TypedItemData::Enum(signature_data) => {
                Self::select_associated_member_signature(
                    &signature_data.method_signatures,
                    &signature_data.initializer_signatures,
                    member_name,
                    member_kind,
                )
            }
            TypedItemData::Protocol(signature_data) => {
                Self::select_associated_member_signature(
                    &signature_data.method_signatures,
                    &signature_data.initializer_signatures,
                    member_name,
                    member_kind,
                )
            }
            TypedItemData::Function(_) => None,
        };
        if direct_signature.is_some() {
            return direct_signature;
        }

        for impl_owner in typed_items.impl_owners_for_target(type_item_id) {
            let Some(impl_signature) = typed_items.impl_signature(impl_owner)
            else {
                continue;
            };
            if let Some(signature) = Self::select_associated_member_signature(
                &impl_signature.method_signatures,
                &impl_signature.initializer_signatures,
                member_name,
                member_kind,
            ) {
                return Some(signature);
            }
        }

        None
    }

    fn select_associated_member_signature(
        method_signatures: &[super::signatures::TypedNamedFunctionSignature],
        initializer_signatures: &[TypedFunctionSignature],
        member_name: &str,
        member_kind: crate::frontend::resolver::AssociatedMemberKind,
    ) -> Option<TypedFunctionSignature> {
        match member_kind {
            crate::frontend::resolver::AssociatedMemberKind::Method => {
                method_signatures
                    .iter()
                    .find(|method| method.name == member_name)
                    .map(|method| method.signature.clone())
            }
            crate::frontend::resolver::AssociatedMemberKind::Initializer => {
                (member_name == "init")
                    .then(|| initializer_signatures.first().cloned())
                    .flatten()
            }
        }
    }

    fn resolve_item_id_from_hir_path(&self, path: &[String]) -> Option<ItemId> {
        let first = path.first()?;
        let file_id = self.body.containing_scope_file_id;
        let imports = &self.hir_input.hir_imports;

        if let Some(binding) =
            imports.get(file_id).and_then(|table| table.get(first))
        {
            if path.len() == 1 {
                if binding.kind
                    == crate::frontend::resolver::HirImportBindingKind::Item
                {
                    let item_ref = binding.target_item?;
                    return self
                        .hir_input
                        .item_id_by_hir_item_ref
                        .get(&item_ref)
                        .copied();
                }
            } else if binding.kind
                == crate::frontend::resolver::HirImportBindingKind::Scope
            {
                let mut full_path = binding.target_path.clone();
                full_path.extend(path.iter().skip(1).cloned());
                let root_name = binding.source_root.as_deref();
                if let Some(item_ref) = imports
                    .item_paths_for_root(root_name)
                    .and_then(|paths| paths.get(&full_path))
                {
                    return self
                        .hir_input
                        .item_id_by_hir_item_ref
                        .get(item_ref)
                        .copied();
                }
            }
        }

        let mut local_full_path =
            imports.scope_path_for_file(file_id)?.to_vec();
        local_full_path.extend(path.iter().cloned());
        let item_ref = imports
            .item_paths_for_root(None)?
            .get(&local_full_path)
            .copied()?;
        self.hir_input
            .item_id_by_hir_item_ref
            .get(&item_ref)
            .copied()
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
        op: HirUnaryOp,
        operand: Type,
        issues: &mut Vec<ExprCheckIssue>,
    ) -> Type {
        if operand.is_error() {
            return Type::error();
        }
        match op {
            HirUnaryOp::Negate if is_numeric_type(&operand) => operand,
            HirUnaryOp::Not if operand == Type::builtin(BuiltinType::Bool) => {
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
        op: HirBinaryOp,
        lhs: Type,
        rhs: Type,
        issues: &mut Vec<ExprCheckIssue>,
    ) -> Type {
        if lhs.is_error() || rhs.is_error() {
            return Type::error();
        }

        match op {
            HirBinaryOp::Add
            | HirBinaryOp::Subtract
            | HirBinaryOp::Multiply
            | HirBinaryOp::Divide
            | HirBinaryOp::Remainder => {
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
            HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr => {
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
            HirBinaryOp::Equal
            | HirBinaryOp::NotEqual
            | HirBinaryOp::Less
            | HirBinaryOp::LessEqual
            | HirBinaryOp::Greater
            | HirBinaryOp::GreaterEqual => {
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
            HirBinaryOp::NullCoalescing => {
                if lhs == rhs {
                    lhs
                } else {
                    Type::error()
                }
            }
            HirBinaryOp::BitOr
            | HirBinaryOp::BitXor
            | HirBinaryOp::BitAnd
            | HirBinaryOp::ShiftLeft
            | HirBinaryOp::ShiftRight => {
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
        target_expr_id: HirExprId,
    ) -> AssignmentTargetStatus {
        let Some(path) =
            Self::extract_namespace_path(self.module, target_expr_id)
        else {
            return AssignmentTargetStatus::MutableLocalOrNonPath;
        };
        let _ = path;

        let Some(resolution) = self
            .hir_input
            .hir_path_table
            .by_expr(self.body_ref.file_id, target_expr_id)
        else {
            return AssignmentTargetStatus::Invalid;
        };

        match resolution {
            crate::frontend::resolver::HirPathResolution::Local(
                hir_local_id,
            ) => match self.local_bindings.get(&hir_local_id) {
                Some(local) if local.mutability == LocalMutability::Mutable => {
                    AssignmentTargetStatus::MutableLocalOrNonPath
                }
                Some(_) => AssignmentTargetStatus::ImmutableLocal(hir_local_id),
                None => AssignmentTargetStatus::Invalid,
            },
            crate::frontend::resolver::HirPathResolution::Item(_) => {
                AssignmentTargetStatus::Invalid
            }
            crate::frontend::resolver::HirPathResolution::AssociatedMember {
                ..
            } => AssignmentTargetStatus::Invalid,
        }
    }

    fn extract_namespace_path(
        module: &HirModule,
        expr_id: HirExprId,
    ) -> Option<Vec<String>> {
        let expr = module.exprs.get(&expr_id)?;
        match &expr.kind {
            HirExprKind::Path(path) => Some(path.segments.clone()),
            HirExprKind::NamespaceField { base, name, .. } => {
                let mut path = Self::extract_namespace_path(module, *base)?;
                path.push(name.clone());
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
