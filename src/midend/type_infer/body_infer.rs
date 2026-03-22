use super::constraints::InferenceIssueKind;
use super::context::InferenceContext;
use super::signature_env::{
    InferFunctionSignature, infer_function_signature_from_typed,
};
use super::types::{ConcreteType, InferenceType};
use crate::frontend::ast::Span;
use crate::frontend::hir::{
    HirAssignOp, HirBinaryOp, HirBodyId, HirCallArg, HirExprId, HirExprKind,
    HirLiteral, HirModule, HirPatId, HirPatKind, HirStmtId, HirStmtKind,
    HirUnaryOp,
};
use crate::frontend::resolver::{
    AssociatedMemberKind, DeclarationOwner, ItemId, LocalId, LocalKind,
    ResolvedBody, ResolvedBodyTable,
};
use crate::frontend::semantic::body_env::{
    BodyTypeEnvironment, BodyTypeEnvironmentTable,
};
use crate::frontend::semantic::hir_input::{SemanticBodyRef, SemanticHirInput};
use crate::frontend::semantic::{
    BuiltinType, Type, TypedItemData, TypedItemTable,
};
use crate::midend::type_check::{TypedFunctionSignature, TypedParamLabel};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyInferIssueKind {
    MissingBodyAst,
    MissingBodyEnvironment,
    MissingResolvedPath {
        expr_id: HirExprId,
    },
    MissingElseBranch,
    InvalidCallTarget,
    NoMatchingCallCandidate {
        candidate_count: usize,
    },
    AmbiguousCallCandidate {
        candidate_count: usize,
    },
    MissingLocalBinding {
        pat_id: HirPatId,
    },
    RequiresExplicitLocalTypeAnnotation {
        hir_local_id: LocalId,
        resolved_local_id: Option<LocalId>,
    },
    /// Multiple enums in scope have a case with this name
    AmbiguousEnumCase {
        case_name: String,
        candidates: Vec<(ItemId, String)>,
    },
    /// No enum in scope has a case with this name
    MissingEnumCase {
        case_name: String,
        available_enums: Vec<String>,
    },
    CoreInferenceIssue {
        kind: InferenceIssueKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyInferIssue {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub span: Span,
    pub kind: BodyInferIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferredCallTarget {
    Function {
        path: Vec<String>,
    },
    AssociatedMember {
        type_item_id: Option<ItemId>,
        member_name: String,
        member_kind: AssociatedMemberKind,
    },
    Method {
        receiver_item_id: Option<ItemId>,
        method_name: String,
    },
    EnumCase {
        enum_item_id: ItemId,
        case_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyInferenceTable {
    expr_types_by_hir_expr_id:
        BTreeMap<(DeclarationOwner, usize, HirExprId), Type>,
    local_types_by_hir_id_by_body:
        BTreeMap<(DeclarationOwner, usize), BTreeMap<LocalId, Type>>,
    local_types_by_resolved_id_by_body:
        BTreeMap<(DeclarationOwner, usize), BTreeMap<LocalId, Type>>,
    root_types_by_body: BTreeMap<(DeclarationOwner, usize), Type>,
    call_targets_by_hir_expr_id:
        BTreeMap<(DeclarationOwner, usize, HirExprId), InferredCallTarget>,
    pub issues: Vec<BodyInferIssue>,
}

impl BodyInferenceTable {
    #[must_use]
    pub fn expr_type_for_hir_expr(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        expr_id: HirExprId,
    ) -> Option<&Type> {
        self.expr_types_by_hir_expr_id.get(&(
            owner.clone(),
            body_index,
            expr_id,
        ))
    }

    #[must_use]
    pub fn local_types_for_hir_body(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Option<&BTreeMap<LocalId, Type>> {
        self.local_types_by_hir_id_by_body
            .get(&(owner.clone(), body_index))
    }

    #[must_use]
    pub fn local_type_for_hir_local(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        hir_local_id: LocalId,
    ) -> Option<&Type> {
        self.local_types_for_hir_body(owner, body_index)?
            .get(&hir_local_id)
    }

    #[must_use]
    pub fn local_types_for_resolved_body(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Option<&BTreeMap<LocalId, Type>> {
        self.local_types_by_resolved_id_by_body
            .get(&(owner.clone(), body_index))
    }

    #[must_use]
    pub fn local_type_for_resolved_local(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        resolved_local_id: LocalId,
    ) -> Option<&Type> {
        self.local_types_for_resolved_body(owner, body_index)?
            .get(&resolved_local_id)
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
    pub fn call_target_for_hir_expr(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        expr_id: HirExprId,
    ) -> Option<&InferredCallTarget> {
        self.call_targets_by_hir_expr_id.get(&(
            owner.clone(),
            body_index,
            expr_id,
        ))
    }

    #[must_use]
    pub fn call_target_count(&self) -> usize {
        self.call_targets_by_hir_expr_id.len()
    }

    #[must_use]
    pub fn expr_type_count(&self) -> usize {
        self.expr_types_by_hir_expr_id.len()
    }

    #[must_use]
    pub fn inferred_hir_local_count(&self) -> usize {
        self.local_types_by_hir_id_by_body
            .values()
            .map(BTreeMap::len)
            .sum()
    }

    #[must_use]
    pub fn root_type_count(&self) -> usize {
        self.root_types_by_body.len()
    }

    pub fn call_targets_for_body(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Vec<(HirExprId, InferredCallTarget)> {
        let mut selected = self
            .call_targets_by_hir_expr_id
            .iter()
            .filter_map(|((target_owner, target_body, expr_id), target)| {
                (target_owner == owner && *target_body == body_index)
                    .then_some((*expr_id, target.clone()))
            })
            .collect::<Vec<_>>();
        selected.sort_by_key(|(expr_id, _)| expr_id.raw());
        selected
    }
}

#[must_use]
pub fn infer_body_types(
    hir_input: &SemanticHirInput,
    typed_items: &TypedItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
) -> BodyInferenceTable {
    let mut expr_types_by_hir_expr_id = BTreeMap::new();
    let mut local_types_by_hir_id_by_body = BTreeMap::new();
    let mut local_types_by_resolved_id_by_body = BTreeMap::new();
    let mut root_types_by_body = BTreeMap::new();
    let mut call_targets_by_hir_expr_id = BTreeMap::new();
    let mut issues = Vec::new();

    for body in resolved_bodies.iter() {
        let Some(env) = body_envs
            .envs_for_owner(&body.owner)
            .iter()
            .find(|candidate| candidate.body_index == body.body_index)
        else {
            issues.push(BodyInferIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: BodyInferIssueKind::MissingBodyEnvironment,
            });
            continue;
        };

        let Some(body_ref) = hir_input.body_ref(&body.owner, body.body_index)
        else {
            issues.push(BodyInferIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: BodyInferIssueKind::MissingBodyAst,
            });
            continue;
        };

        let Some(module) = hir_input.hir_modules.get(&body_ref.file_id) else {
            issues.push(BodyInferIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: BodyInferIssueKind::MissingBodyAst,
            });
            continue;
        };

        if !module.bodies.contains_key(&body_ref.body_id) {
            issues.push(BodyInferIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: BodyInferIssueKind::MissingBodyAst,
            });
            continue;
        }

        let mut inferencer = BodyInferencer::new(
            body,
            body_ref,
            module,
            hir_input,
            typed_items,
            env,
        );

        let root_ty = inferencer.infer_root_body();
        let expr_types = inferencer.finalize_expr_types();
        let hir_local_types = inferencer.finalize_local_types();
        let call_targets = inferencer.finalize_call_targets();
        let body_issues = inferencer.take_issues();

        for (expr_id, ty) in expr_types {
            expr_types_by_hir_expr_id
                .insert((body.owner.clone(), body.body_index, expr_id), ty);
        }
        for (expr_id, target) in call_targets {
            call_targets_by_hir_expr_id
                .insert((body.owner.clone(), body.body_index, expr_id), target);
        }

        let mut resolved_local_types = BTreeMap::new();
        for (hir_local_id, ty) in &hir_local_types {
            if let Some(resolved_local_id) =
                env.resolved_local_id_for_hir_local(*hir_local_id)
            {
                resolved_local_types.insert(resolved_local_id, ty.clone());
            }
        }

        local_types_by_hir_id_by_body
            .insert((body.owner.clone(), body.body_index), hir_local_types);
        local_types_by_resolved_id_by_body.insert(
            (body.owner.clone(), body.body_index),
            resolved_local_types,
        );
        root_types_by_body
            .insert((body.owner.clone(), body.body_index), root_ty);
        issues.extend(body_issues);
    }

    BodyInferenceTable {
        expr_types_by_hir_expr_id,
        local_types_by_hir_id_by_body,
        local_types_by_resolved_id_by_body,
        root_types_by_body,
        call_targets_by_hir_expr_id,
        issues,
    }
}

struct BodyInferencer<'a> {
    body: &'a ResolvedBody,
    body_ref: SemanticBodyRef,
    module: &'a HirModule,
    hir_input: &'a SemanticHirInput,
    typed_items: &'a TypedItemTable,
    env: &'a BodyTypeEnvironment,
    ctx: InferenceContext,
    function_return_type: InferenceType,
    local_types: BTreeMap<LocalId, InferenceType>,
    unannotated_hir_locals: BTreeSet<LocalId>,
    expr_types: BTreeMap<HirExprId, InferenceType>,
    selected_call_targets: BTreeMap<HirExprId, InferredCallTarget>,
    issues: Vec<BodyInferIssue>,
}

#[derive(Debug, Clone)]
struct MethodCandidate {
    receiver_type: InferenceType,
    signature: InferFunctionSignature,
}

impl<'a> BodyInferencer<'a> {
    fn new(
        body: &'a ResolvedBody,
        body_ref: SemanticBodyRef,
        module: &'a HirModule,
        hir_input: &'a SemanticHirInput,
        typed_items: &'a TypedItemTable,
        env: &'a BodyTypeEnvironment,
    ) -> Self {
        let mut ctx = InferenceContext::new();
        let mut local_types = BTreeMap::new();
        let mut unannotated_hir_locals = BTreeSet::new();
        let resolved_locals_by_id = body
            .locals
            .iter()
            .map(|local| (local.id, local))
            .collect::<BTreeMap<_, _>>();

        for (hir_local_id, semantic_type) in &env.local_types {
            let is_unannotated_local = env
                .resolved_local_id_for_hir_local(*hir_local_id)
                .and_then(|resolved_id| resolved_locals_by_id.get(&resolved_id))
                .is_some_and(|local| {
                    local.kind != LocalKind::Parameter
                        && local.declared_type.is_none()
                });

            if is_unannotated_local {
                unannotated_hir_locals.insert(*hir_local_id);
            }

            let inference_ty = if is_unannotated_local {
                InferenceType::Var(ctx.fresh_type_var())
            } else {
                inference_type_from_semantic(semantic_type)
            };
            local_types.insert(*hir_local_id, inference_ty);
        }

        let function_return_type =
            inference_type_from_semantic(&env.expected_return_type);

        Self {
            body,
            body_ref,
            module,
            hir_input,
            typed_items,
            env,
            ctx,
            function_return_type,
            local_types,
            unannotated_hir_locals,
            expr_types: BTreeMap::new(),
            selected_call_targets: BTreeMap::new(),
            issues: Vec::new(),
        }
    }

    fn take_issues(self) -> Vec<BodyInferIssue> {
        self.issues
    }

    fn infer_root_body(&mut self) -> Type {
        let root_ty = self.infer_hir_body(
            self.body_ref.body_id,
            Some(self.function_return_type.clone()),
        );
        self.ctx.finalize();
        self.capture_core_inference_issues();
        self.finalize_type(root_ty)
    }

    fn finalize_expr_types(&mut self) -> BTreeMap<HirExprId, Type> {
        let expr_types = std::mem::take(&mut self.expr_types);
        expr_types
            .into_iter()
            .map(|(expr_id, ty)| (expr_id, self.finalize_type(ty)))
            .collect()
    }

    fn finalize_local_types(&mut self) -> BTreeMap<LocalId, Type> {
        let local_types = std::mem::take(&mut self.local_types);
        local_types
            .into_iter()
            .map(|(hir_local_id, ty)| {
                let resolved = self.ctx.resolve(ty.clone());
                if self.unannotated_hir_locals.contains(&hir_local_id)
                    && matches!(resolved, InferenceType::Var(_))
                {
                    self.issues.push(BodyInferIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: local_declared_span(
                            self.body,
                            self.env,
                            hir_local_id,
                        ),
                        kind: BodyInferIssueKind::RequiresExplicitLocalTypeAnnotation {
                            hir_local_id,
                            resolved_local_id: self
                                .env
                                .resolved_local_id_for_hir_local(hir_local_id),
                        },
                    });
                }

                (hir_local_id, self.finalize_type(ty))
            })
            .collect()
    }

    fn finalize_call_targets(
        &mut self,
    ) -> BTreeMap<HirExprId, InferredCallTarget> {
        std::mem::take(&mut self.selected_call_targets)
    }

    fn capture_core_inference_issues(&mut self) {
        for issue in &self.ctx.issues {
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span: Span::new(0, 0),
                kind: BodyInferIssueKind::CoreInferenceIssue {
                    kind: issue.kind.clone(),
                },
            });
        }
    }

    fn finalize_type(&mut self, ty: InferenceType) -> Type {
        match self.ctx.resolve(ty) {
            InferenceType::Known(known) => {
                known.to_semantic_type().unwrap_or_else(Type::error)
            }
            InferenceType::Var(_) | InferenceType::Error => Type::error(),
        }
    }

    fn infer_hir_body(
        &mut self,
        body_id: HirBodyId,
        expected: Option<InferenceType>,
    ) -> InferenceType {
        let Some(body) = self.module.bodies.get(&body_id) else {
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span: Span::new(0, 0),
                kind: BodyInferIssueKind::MissingBodyAst,
            });
            return InferenceType::Error;
        };

        for stmt_id in &body.stmts {
            self.infer_stmt(*stmt_id);
        }

        let mut body_ty = if let Some(tail_expr) = body.tail_expr {
            self.infer_expr(tail_expr, expected.clone())
        } else {
            let void =
                InferenceType::Known(ConcreteType::Builtin(BuiltinType::Void));
            if let Some(expected) = expected.clone() {
                self.constrain_expected(void, expected)
            } else {
                void
            }
        };

        if let Some(expected) = expected {
            body_ty = self.constrain_expected(body_ty, expected);
        }

        body_ty
    }

    fn infer_stmt(&mut self, stmt_id: HirStmtId) {
        let Some(stmt) = self.module.stmts.get(&stmt_id) else {
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span: Span::new(0, 0),
                kind: BodyInferIssueKind::MissingBodyAst,
            });
            return;
        };

        match &stmt.kind {
            HirStmtKind::Let(let_stmt) => {
                let binding_ids =
                    self.collect_pattern_binding_ids(let_stmt.pat);
                let expected_init = binding_ids
                    .first()
                    .and_then(|local_id| self.local_types.get(local_id))
                    .cloned();
                let init_ty = let_stmt.value.map(|expr_id| {
                    self.infer_expr(expr_id, expected_init.clone())
                });

                if binding_ids.is_empty() {
                    self.issues.push(BodyInferIssue {
                        owner: self.body.owner.clone(),
                        body_index: self.body.body_index,
                        span: stmt.origin.span,
                        kind: BodyInferIssueKind::MissingLocalBinding {
                            pat_id: let_stmt.pat,
                        },
                    });
                }

                if let Some(init_ty) = init_ty {
                    for hir_local_id in binding_ids {
                        let local_ty = self
                            .local_types
                            .entry(hir_local_id)
                            .or_insert_with(|| self.ctx.fresh_type())
                            .clone();
                        let _ =
                            self.ctx.constrain_equal(local_ty, init_ty.clone());
                    }
                }
            }
            HirStmtKind::Expr { expr } | HirStmtKind::Semi { expr } => {
                let _ = self.infer_expr(*expr, None);
            }
            HirStmtKind::Item { .. } => {}
        }
    }

    fn infer_expr(
        &mut self,
        expr_id: HirExprId,
        expected: Option<InferenceType>,
    ) -> InferenceType {
        let Some(expr) = self.module.exprs.get(&expr_id) else {
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span: Span::new(0, 0),
                kind: BodyInferIssueKind::MissingBodyAst,
            });
            return InferenceType::Error;
        };

        let span = expr.origin.span;
        let kind = expr.kind.clone();

        let mut ty = match kind {
            HirExprKind::Literal(literal) => self.infer_literal(&literal),
            HirExprKind::Path(_) | HirExprKind::NamespaceField { .. } => {
                self.infer_path_expr(expr_id, span, expected.clone())
            }
            HirExprKind::Call { callee, args } => self.infer_call_expr(
                expr_id,
                callee,
                &args,
                span,
                expected.clone(),
            ),
            HirExprKind::Block { body } => {
                self.infer_hir_body(body, expected.clone())
            }
            HirExprKind::If {
                condition,
                then_body,
                else_expr,
            } => self.infer_if_expr(
                condition,
                then_body,
                else_expr,
                span,
                expected.clone(),
            ),
            HirExprKind::Assign { op, target, value } => {
                self.infer_assign_expr(op, target, value)
            }
            HirExprKind::Return { value } => self.infer_return_expr(value),
            HirExprKind::Unary { op, expr } => self.infer_unary_expr(op, expr),
            HirExprKind::Binary { op, lhs, rhs } => {
                self.infer_binary_expr(op, lhs, rhs)
            }
            HirExprKind::While { condition, body } => {
                let bool_ty = InferenceType::Known(ConcreteType::Builtin(
                    BuiltinType::Bool,
                ));
                let _ = self.infer_expr(condition, Some(bool_ty));
                let _ = self.infer_hir_body(body, None);
                InferenceType::Known(ConcreteType::Builtin(BuiltinType::Void))
            }
            HirExprKind::For { iterator, body, .. } => {
                let _ = self.infer_expr(iterator, None);
                let _ = self.infer_hir_body(body, None);
                InferenceType::Known(ConcreteType::Builtin(BuiltinType::Void))
            }
            HirExprKind::Try { expr }
            | HirExprKind::ForceUnwrap { expr }
            | HirExprKind::Spread { expr } => {
                self.infer_expr(expr, expected.clone())
            }
            HirExprKind::Break | HirExprKind::Continue => {
                InferenceType::Known(ConcreteType::Builtin(BuiltinType::Never))
            }
            HirExprKind::Array { elements } => {
                for element in elements {
                    match element {
                        crate::frontend::hir::HirArrayElement::Expr(value)
                        | crate::frontend::hir::HirArrayElement::Spread(
                            value,
                        ) => {
                            let _ = self.infer_expr(value, None);
                        }
                    }
                }
                InferenceType::Error
            }
            HirExprKind::Struct { fields, .. } => {
                for field in fields {
                    match field {
                        crate::frontend::hir::HirStructExprField::Named {
                            value,
                            ..
                        }
                        | crate::frontend::hir::HirStructExprField::Spread {
                            value,
                        } => {
                            let _ = self.infer_expr(value, None);
                        }
                    }
                }
                InferenceType::Error
            }
            HirExprKind::Tuple { elements } => {
                for element in elements {
                    let _ = self.infer_expr(element, None);
                }
                InferenceType::Error
            }
            HirExprKind::Match { subject, arms } => {
                let _ = self.infer_expr(subject, None);
                let mut arm_ty: Option<InferenceType> = None;
                for arm in arms {
                    let current = self.infer_expr(arm.expr, expected.clone());
                    arm_ty = Some(match arm_ty {
                        Some(existing) => {
                            self.ctx.constrain_equal(existing, current)
                        }
                        None => current,
                    });
                }
                arm_ty.unwrap_or_else(|| {
                    InferenceType::Known(ConcreteType::Builtin(
                        BuiltinType::Void,
                    ))
                })
            }
            HirExprKind::Field { base, .. }
            | HirExprKind::OptionalField { base, .. } => {
                let _ = self.infer_expr(base, None);
                InferenceType::Error
            }
            HirExprKind::MethodCall {
                receiver,
                method_name,
                args,
            } => self.infer_method_call_expr(
                expr_id,
                receiver,
                &method_name,
                &args,
                span,
                expected.clone(),
            ),
            HirExprKind::Index { base, index }
            | HirExprKind::OptionalIndex { base, index } => {
                let _ = self.infer_expr(base, None);
                let _ = self.infer_expr(index, None);
                InferenceType::Error
            }
            HirExprKind::Closure { .. }
            | HirExprKind::Cast { .. }
            | HirExprKind::Range { .. } => InferenceType::Error,
        };

        if let Some(expected) = expected {
            ty = self.constrain_expected(ty, expected);
        }

        self.expr_types.insert(expr_id, ty.clone());
        ty
    }

    fn infer_literal(&mut self, literal: &HirLiteral) -> InferenceType {
        match literal {
            HirLiteral::Boolean(_) => {
                InferenceType::Known(ConcreteType::Builtin(BuiltinType::Bool))
            }
            HirLiteral::Char(_) => {
                InferenceType::Known(ConcreteType::Builtin(BuiltinType::Char))
            }
            HirLiteral::String(_) => {
                InferenceType::Known(ConcreteType::Builtin(BuiltinType::String))
            }
            HirLiteral::Integer(_) => {
                let var = self.ctx.fresh_type_var();
                self.ctx.mark_integer_literal_var(var);
                InferenceType::Var(var)
            }
            HirLiteral::Float(_) => {
                let var = self.ctx.fresh_type_var();
                self.ctx.mark_float_literal_var(var);
                InferenceType::Var(var)
            }
        }
    }

    fn infer_path_expr(
        &mut self,
        expr_id: HirExprId,
        span: Span,
        expected: Option<InferenceType>,
    ) -> InferenceType {
        let Some(resolution) = self
            .hir_input
            .hir_path_table
            .by_expr(self.body_ref.file_id, expr_id)
        else {
            // Try contextual inference first (when expected type exists)
            if let Some(enum_case_ty) =
                self.contextual_enum_case_path_type(expr_id, expected)
            {
                return enum_case_ty;
            }

            // Try scope-based search when no expected type
            let expr = self.module.exprs.get(&expr_id);
            if let Some(expr) = expr {
                if let HirExprKind::Path(path) = &expr.kind {
                    if path.segments.len() == 1 {
                        let case_name = path.segments[0].clone();
                        if let Some(enum_ty) =
                            self.scoped_enum_case_path_type(expr_id, case_name)
                        {
                            return enum_ty;
                        }
                    }
                }
            }

            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: BodyInferIssueKind::MissingResolvedPath { expr_id },
            });
            return InferenceType::Error;
        };

        match resolution {
            crate::frontend::resolver::HirPathResolution::Local(local_id) => {
                self.local_types
                    .get(&local_id)
                    .cloned()
                    .unwrap_or(InferenceType::Error)
            }
            crate::frontend::resolver::HirPathResolution::Item(_)
            | crate::frontend::resolver::HirPathResolution::AssociatedMember {
                ..
            } => InferenceType::Error,
        }
    }

    fn infer_call_expr(
        &mut self,
        call_expr_id: HirExprId,
        callee_expr_id: HirExprId,
        args: &[HirCallArg],
        span: Span,
        expected: Option<InferenceType>,
    ) -> InferenceType {
        // In some pipelines (e.g. tests that skip grouped desugaring),
        // `a.m(b)` is represented as `Call(callee = Field(a, "m"))`.
        // Treat this shape as a method call so receiver-based inference still applies.
        if let Some(callee_expr) = self.module.exprs.get(&callee_expr_id) {
            match &callee_expr.kind {
                HirExprKind::Field { base, name }
                | HirExprKind::OptionalField { base, name } => {
                    return self.infer_method_call_expr(
                        call_expr_id,
                        *base,
                        name,
                        args,
                        span,
                        expected,
                    );
                }
                _ => {}
            }
        }

        let _ = self.infer_expr(callee_expr_id, None);

        let candidates = self.call_candidates_for_callee(callee_expr_id);
        if candidates.is_empty() {
            // Try contextual enum case inference first
            if let Some(enum_case_ty) = self.contextual_enum_case_call_type(
                call_expr_id,
                callee_expr_id,
                args,
                expected.clone(),
            ) {
                return enum_case_ty;
            }

            // Try scope-based enum case search when no expected type
            if expected.is_none() {
                if let Some(enum_case_ty) = self.scoped_enum_case_call_type(
                    call_expr_id,
                    callee_expr_id,
                    args,
                ) {
                    return enum_case_ty;
                }
            }

            for arg in args {
                let _ = self.infer_expr(arg.value, None);
            }
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: BodyInferIssueKind::InvalidCallTarget,
            });
            return InferenceType::Error;
        }

        let matched_by_shape = candidates
            .into_iter()
            .filter(|candidate| call_signature_matches(candidate, args))
            .collect::<Vec<_>>();
        let arg_types_for_filter = args
            .iter()
            .map(|arg| self.infer_expr(arg.value, None))
            .collect::<Vec<_>>();
        let matched = matched_by_shape
            .into_iter()
            .filter(|candidate| {
                self.call_candidate_compatible(
                    candidate,
                    &arg_types_for_filter,
                    expected.clone(),
                )
            })
            .collect::<Vec<_>>();

        if matched.is_empty() {
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: BodyInferIssueKind::NoMatchingCallCandidate {
                    candidate_count: self
                        .call_candidates_for_callee(callee_expr_id)
                        .len(),
                },
            });
            return InferenceType::Error;
        }

        if matched.len() > 1 {
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: BodyInferIssueKind::AmbiguousCallCandidate {
                    candidate_count: matched.len(),
                },
            });
            return InferenceType::Error;
        }

        let signature = &matched[0];
        self.record_selected_call_target(
            call_expr_id,
            self.call_target_for_callee(callee_expr_id, signature),
        );
        for (index, arg) in args.iter().enumerate() {
            let expected = signature.param_types.get(index).cloned();
            let arg_ty = self.infer_expr(arg.value, expected.clone());
            if let Some(expected) = expected {
                let _ = self.ctx.constrain_equal(arg_ty, expected);
            }
        }

        self.call_return_type(callee_expr_id, signature)
    }

    fn infer_method_call_expr(
        &mut self,
        call_expr_id: HirExprId,
        receiver_expr_id: HirExprId,
        method_name: &str,
        args: &[HirCallArg],
        span: Span,
        expected: Option<InferenceType>,
    ) -> InferenceType {
        let receiver_ty = self.infer_expr(receiver_expr_id, None);
        let candidates = self.method_candidates(method_name, &receiver_ty);
        if candidates.is_empty() {
            for arg in args {
                let _ = self.infer_expr(arg.value, None);
            }
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: BodyInferIssueKind::InvalidCallTarget,
            });
            return InferenceType::Error;
        }

        let matched_by_shape = candidates
            .into_iter()
            .filter(|candidate| {
                call_signature_matches(&candidate.signature, args)
            })
            .collect::<Vec<_>>();
        let arg_types_for_filter = args
            .iter()
            .map(|arg| self.infer_expr(arg.value, None))
            .collect::<Vec<_>>();
        let matched = matched_by_shape
            .into_iter()
            .filter(|candidate| {
                self.method_candidate_compatible(
                    candidate,
                    &receiver_ty,
                    &arg_types_for_filter,
                    expected.clone(),
                )
            })
            .collect::<Vec<_>>();

        if matched.is_empty() {
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: BodyInferIssueKind::NoMatchingCallCandidate {
                    candidate_count: self
                        .method_candidates(method_name, &receiver_ty)
                        .len(),
                },
            });
            return InferenceType::Error;
        }

        if matched.len() > 1 {
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: BodyInferIssueKind::AmbiguousCallCandidate {
                    candidate_count: matched.len(),
                },
            });
            return InferenceType::Error;
        }

        let candidate = &matched[0];
        self.record_selected_call_target(
            call_expr_id,
            Some(InferredCallTarget::Method {
                receiver_item_id: self
                    .nominal_item_id(&candidate.receiver_type),
                method_name: method_name.to_string(),
            }),
        );
        let _ = self.ctx.constrain_equal(
            receiver_ty.clone(),
            candidate.receiver_type.clone(),
        );
        for (index, arg) in args.iter().enumerate() {
            let expected = candidate.signature.param_types.get(index).cloned();
            let arg_ty = self.infer_expr(arg.value, expected.clone());
            if let Some(expected) = expected {
                let _ = self.ctx.constrain_equal(arg_ty, expected);
            }
        }

        candidate.signature.return_type.clone().unwrap_or_else(|| {
            InferenceType::Known(ConcreteType::Builtin(BuiltinType::Void))
        })
    }

    fn infer_if_expr(
        &mut self,
        condition: HirExprId,
        then_body: HirBodyId,
        else_expr: Option<HirExprId>,
        span: Span,
        expected: Option<InferenceType>,
    ) -> InferenceType {
        let bool_ty =
            InferenceType::Known(ConcreteType::Builtin(BuiltinType::Bool));
        let _ = self.infer_expr(condition, Some(bool_ty));

        let then_ty = self.infer_hir_body(then_body, expected.clone());

        let Some(else_expr) = else_expr else {
            self.issues.push(BodyInferIssue {
                owner: self.body.owner.clone(),
                body_index: self.body.body_index,
                span,
                kind: BodyInferIssueKind::MissingElseBranch,
            });
            return InferenceType::Error;
        };

        let else_ty = self.infer_expr(else_expr, expected);
        self.merge_branch_types(then_ty, else_ty)
    }

    fn infer_assign_expr(
        &mut self,
        _op: HirAssignOp,
        target: HirExprId,
        value: HirExprId,
    ) -> InferenceType {
        let target_ty = self.infer_expr(target, None);
        let value_ty = self.infer_expr(value, Some(target_ty.clone()));
        self.ctx.constrain_equal(target_ty, value_ty)
    }

    fn infer_return_expr(&mut self, value: Option<HirExprId>) -> InferenceType {
        match value {
            Some(value) => {
                let value_ty = self
                    .infer_expr(value, Some(self.function_return_type.clone()));
                let _ = self.ctx.constrain_equal(
                    self.function_return_type.clone(),
                    value_ty,
                );
            }
            None => {
                let void = InferenceType::Known(ConcreteType::Builtin(
                    BuiltinType::Void,
                ));
                let _ = self
                    .ctx
                    .constrain_equal(self.function_return_type.clone(), void);
            }
        }

        InferenceType::Known(ConcreteType::Builtin(BuiltinType::Never))
    }

    fn infer_unary_expr(
        &mut self,
        op: HirUnaryOp,
        expr: HirExprId,
    ) -> InferenceType {
        match op {
            HirUnaryOp::Not => {
                let bool_ty = InferenceType::Known(ConcreteType::Builtin(
                    BuiltinType::Bool,
                ));
                let _ = self.infer_expr(expr, Some(bool_ty.clone()));
                bool_ty
            }
            HirUnaryOp::Negate => {
                let out = self.ctx.fresh_type();
                let inner = self.infer_expr(expr, Some(out.clone()));
                self.ctx.constrain_equal(inner, out)
            }
        }
    }

    fn infer_binary_expr(
        &mut self,
        op: HirBinaryOp,
        lhs: HirExprId,
        rhs: HirExprId,
    ) -> InferenceType {
        match op {
            HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr => {
                let bool_ty = InferenceType::Known(ConcreteType::Builtin(
                    BuiltinType::Bool,
                ));
                let _ = self.infer_expr(lhs, Some(bool_ty.clone()));
                let _ = self.infer_expr(rhs, Some(bool_ty.clone()));
                bool_ty
            }
            HirBinaryOp::Equal
            | HirBinaryOp::NotEqual
            | HirBinaryOp::Less
            | HirBinaryOp::LessEqual
            | HirBinaryOp::Greater
            | HirBinaryOp::GreaterEqual => {
                let lhs_ty = self.infer_expr(lhs, None);
                let rhs_ty = self.infer_expr(rhs, Some(lhs_ty.clone()));
                let _ = self.ctx.constrain_equal(lhs_ty, rhs_ty);
                InferenceType::Known(ConcreteType::Builtin(BuiltinType::Bool))
            }
            HirBinaryOp::NullCoalescing => {
                let lhs_ty = self.infer_expr(lhs, None);
                let rhs_ty = self.infer_expr(rhs, Some(lhs_ty.clone()));
                self.ctx.constrain_equal(lhs_ty, rhs_ty)
            }
            HirBinaryOp::BitOr
            | HirBinaryOp::BitXor
            | HirBinaryOp::BitAnd
            | HirBinaryOp::ShiftLeft
            | HirBinaryOp::ShiftRight
            | HirBinaryOp::Add
            | HirBinaryOp::Subtract
            | HirBinaryOp::Multiply
            | HirBinaryOp::Divide
            | HirBinaryOp::Remainder => {
                let out = self.ctx.fresh_type();
                let lhs_ty = self.infer_expr(lhs, Some(out.clone()));
                let rhs_ty = self.infer_expr(rhs, Some(out.clone()));
                let out = self.ctx.constrain_equal(lhs_ty, out);
                self.ctx.constrain_equal(rhs_ty, out)
            }
        }
    }

    fn merge_branch_types(
        &mut self,
        then_ty: InferenceType,
        else_ty: InferenceType,
    ) -> InferenceType {
        if is_never_type(&mut self.ctx, &then_ty) {
            return else_ty;
        }
        if is_never_type(&mut self.ctx, &else_ty) {
            return then_ty;
        }
        self.ctx.constrain_equal(then_ty, else_ty)
    }

    fn constrain_expected(
        &mut self,
        actual: InferenceType,
        expected: InferenceType,
    ) -> InferenceType {
        if is_never_type(&mut self.ctx, &actual)
            || is_never_type(&mut self.ctx, &expected)
        {
            return actual;
        }
        self.ctx.constrain_equal(actual, expected)
    }

    fn collect_pattern_binding_ids(&self, pat_id: HirPatId) -> Vec<LocalId> {
        let mut output = Vec::new();
        self.collect_pattern_binding_ids_inner(pat_id, &mut output);
        output
    }

    fn collect_pattern_binding_ids_inner(
        &self,
        pat_id: HirPatId,
        output: &mut Vec<LocalId>,
    ) {
        let Some(pattern) = self.module.patterns.get(&pat_id) else {
            return;
        };

        if let Some(local_id) = self
            .hir_input
            .hir_local_bindings
            .binding_for_pat(self.body_ref.file_id, pat_id)
        {
            output.push(local_id);
        }

        match &pattern.kind {
            HirPatKind::Tuple { elements } => {
                for element in elements {
                    self.collect_pattern_binding_ids_inner(*element, output);
                }
            }
            HirPatKind::Struct { fields, .. } => {
                for field in fields {
                    if let Some(inner) = field.pat {
                        self.collect_pattern_binding_ids_inner(inner, output);
                    }
                }
            }
            HirPatKind::EnumVariant { args, .. } => {
                for arg in args {
                    self.collect_pattern_binding_ids_inner(*arg, output);
                }
            }
            HirPatKind::Binding { .. }
            | HirPatKind::Wildcard
            | HirPatKind::Literal(_) => {}
        }
    }

    fn call_candidates_for_callee(
        &self,
        callee_expr_id: HirExprId,
    ) -> Vec<InferFunctionSignature> {
        let Some(resolution) = self
            .hir_input
            .hir_path_table
            .by_expr(self.body_ref.file_id, callee_expr_id)
        else {
            return Vec::new();
        };

        match resolution {
            crate::frontend::resolver::HirPathResolution::Local(_) => Vec::new(),
            crate::frontend::resolver::HirPathResolution::Item(item_ref) => {
                let Some(item_id) = self
                    .hir_input
                    .item_id_by_hir_item_ref
                    .get(&item_ref)
                    .copied()
                else {
                    return Vec::new();
                };

                if let Some(function_signature) = self.typed_items.function(item_id)
                {
                    return vec![infer_function_signature_from_typed(
                        function_signature,
                    )];
                }

                self.initializer_signatures_for_item(item_id)
            }
            crate::frontend::resolver::HirPathResolution::AssociatedMember {
                type_item_ref,
                member_name,
                member_kind,
            } => {
                let Some(type_item_id) = self
                    .hir_input
                    .item_id_by_hir_item_ref
                    .get(&type_item_ref)
                    .copied()
                else {
                    return Vec::new();
                };
                self.associated_member_signatures_for_item(
                    type_item_id,
                    &member_name,
                    member_kind,
                )
            }
        }
    }

    fn initializer_signatures_for_item(
        &self,
        item_id: crate::frontend::resolver::ItemId,
    ) -> Vec<InferFunctionSignature> {
        match self.typed_items.get(item_id) {
            Some(TypedItemData::Struct(signature_data)) => signature_data
                .initializer_signatures
                .iter()
                .map(infer_function_signature_from_typed)
                .collect(),
            Some(TypedItemData::Enum(signature_data)) => signature_data
                .initializer_signatures
                .iter()
                .map(infer_function_signature_from_typed)
                .collect(),
            Some(TypedItemData::Protocol(signature_data)) => signature_data
                .initializer_signatures
                .iter()
                .map(infer_function_signature_from_typed)
                .collect(),
            Some(TypedItemData::Function(_)) | None => Vec::new(),
        }
    }

    fn associated_member_signatures_for_item(
        &self,
        type_item_id: crate::frontend::resolver::ItemId,
        member_name: &str,
        member_kind: AssociatedMemberKind,
    ) -> Vec<InferFunctionSignature> {
        let mut signatures = Vec::new();

        match self.typed_items.get(type_item_id) {
            Some(TypedItemData::Struct(signature_data)) => {
                collect_associated_signatures(
                    &mut signatures,
                    &signature_data.method_signatures,
                    &signature_data.initializer_signatures,
                    member_name,
                    member_kind,
                );
            }
            Some(TypedItemData::Enum(signature_data)) => {
                collect_associated_signatures(
                    &mut signatures,
                    &signature_data.method_signatures,
                    &signature_data.initializer_signatures,
                    member_name,
                    member_kind,
                );
            }
            Some(TypedItemData::Protocol(signature_data)) => {
                collect_associated_signatures(
                    &mut signatures,
                    &signature_data.method_signatures,
                    &signature_data.initializer_signatures,
                    member_name,
                    member_kind,
                );
            }
            Some(TypedItemData::Function(_)) | None => {}
        }

        for impl_owner in self.typed_items.impl_owners_for_target(type_item_id)
        {
            let Some(impl_signature) =
                self.typed_items.impl_signature(impl_owner)
            else {
                continue;
            };
            collect_associated_signatures(
                &mut signatures,
                &impl_signature.method_signatures,
                &impl_signature.initializer_signatures,
                member_name,
                member_kind,
            );
        }

        signatures
    }

    fn method_candidates(
        &self,
        method_name: &str,
        receiver_ty: &InferenceType,
    ) -> Vec<MethodCandidate> {
        let mut candidates = Vec::new();

        // If receiver is already known nominal, scope candidates to that type.
        if let Some(item_id) = self.nominal_item_id(receiver_ty) {
            candidates.extend(
                self.method_candidates_for_item(item_id, method_name)
                    .into_iter(),
            );
            return candidates;
        }

        // Unknown receiver: collect every method with this name.
        for (item_id, item_data) in self.typed_items.iter() {
            let receiver_type = match item_data {
                TypedItemData::Struct(_) => {
                    InferenceType::Known(ConcreteType::Nominal {
                        item_id,
                        kind: crate::frontend::semantic::NamedTypeKind::Struct,
                    })
                }
                TypedItemData::Enum(_) => {
                    InferenceType::Known(ConcreteType::Nominal {
                        item_id,
                        kind: crate::frontend::semantic::NamedTypeKind::Enum,
                    })
                }
                TypedItemData::Protocol(_) => {
                    InferenceType::Known(ConcreteType::Nominal {
                        item_id,
                        kind:
                            crate::frontend::semantic::NamedTypeKind::Protocol,
                    })
                }
                TypedItemData::Function(_) => continue,
            };

            match item_data {
                TypedItemData::Struct(signature_data) => {
                    for method in &signature_data.method_signatures {
                        if method.name == method_name {
                            candidates.push(MethodCandidate {
                                receiver_type: receiver_type.clone(),
                                signature: infer_function_signature_from_typed(
                                    &method.signature,
                                ),
                            });
                        }
                    }
                }
                TypedItemData::Enum(signature_data) => {
                    for method in &signature_data.method_signatures {
                        if method.name == method_name {
                            candidates.push(MethodCandidate {
                                receiver_type: receiver_type.clone(),
                                signature: infer_function_signature_from_typed(
                                    &method.signature,
                                ),
                            });
                        }
                    }
                }
                TypedItemData::Protocol(signature_data) => {
                    for method in &signature_data.method_signatures {
                        if method.name == method_name {
                            candidates.push(MethodCandidate {
                                receiver_type: receiver_type.clone(),
                                signature: infer_function_signature_from_typed(
                                    &method.signature,
                                ),
                            });
                        }
                    }
                }
                TypedItemData::Function(_) => {}
            }
        }

        // Include impl methods in unknown receiver mode.
        for (item_id, _) in self.typed_items.iter() {
            for impl_owner in self.typed_items.impl_owners_for_target(item_id) {
                let Some(impl_signature) =
                    self.typed_items.impl_signature(impl_owner)
                else {
                    continue;
                };
                let receiver_type =
                    inference_type_from_semantic(&impl_signature.target);
                for method in &impl_signature.method_signatures {
                    if method.name == method_name {
                        candidates.push(MethodCandidate {
                            receiver_type: receiver_type.clone(),
                            signature: infer_function_signature_from_typed(
                                &method.signature,
                            ),
                        });
                    }
                }
            }
        }

        candidates
    }

    fn method_candidates_for_item(
        &self,
        item_id: crate::frontend::resolver::ItemId,
        method_name: &str,
    ) -> Vec<MethodCandidate> {
        let mut candidates = Vec::new();
        let Some(receiver_type) = self.nominal_type_for_item(item_id) else {
            return candidates;
        };

        match self.typed_items.get(item_id) {
            Some(TypedItemData::Struct(signature_data)) => {
                for method in &signature_data.method_signatures {
                    if method.name == method_name {
                        candidates.push(MethodCandidate {
                            receiver_type: receiver_type.clone(),
                            signature: infer_function_signature_from_typed(
                                &method.signature,
                            ),
                        });
                    }
                }
            }
            Some(TypedItemData::Enum(signature_data)) => {
                for method in &signature_data.method_signatures {
                    if method.name == method_name {
                        candidates.push(MethodCandidate {
                            receiver_type: receiver_type.clone(),
                            signature: infer_function_signature_from_typed(
                                &method.signature,
                            ),
                        });
                    }
                }
            }
            Some(TypedItemData::Protocol(signature_data)) => {
                for method in &signature_data.method_signatures {
                    if method.name == method_name {
                        candidates.push(MethodCandidate {
                            receiver_type: receiver_type.clone(),
                            signature: infer_function_signature_from_typed(
                                &method.signature,
                            ),
                        });
                    }
                }
            }
            Some(TypedItemData::Function(_)) | None => {}
        }

        for impl_owner in self.typed_items.impl_owners_for_target(item_id) {
            let Some(impl_signature) =
                self.typed_items.impl_signature(impl_owner)
            else {
                continue;
            };
            let receiver_type =
                inference_type_from_semantic(&impl_signature.target);
            for method in &impl_signature.method_signatures {
                if method.name == method_name {
                    candidates.push(MethodCandidate {
                        receiver_type: receiver_type.clone(),
                        signature: infer_function_signature_from_typed(
                            &method.signature,
                        ),
                    });
                }
            }
        }

        candidates
    }

    fn call_candidate_compatible(
        &self,
        candidate: &InferFunctionSignature,
        arg_types: &[InferenceType],
        expected: Option<InferenceType>,
    ) -> bool {
        let mut probe = self.ctx.clone();
        if let Some(expected) = expected
            && !matches!(candidate.return_type, None)
            && let Some(return_ty) = candidate.return_type.clone()
            && !self.compatible_with_error_tolerance(
                &mut probe, return_ty, expected,
            )
        {
            return false;
        }

        for (arg_ty, param_ty) in
            arg_types.iter().zip(candidate.param_types.iter())
        {
            if !self.compatible_with_error_tolerance(
                &mut probe,
                arg_ty.clone(),
                param_ty.clone(),
            ) {
                return false;
            }
        }

        true
    }

    fn method_candidate_compatible(
        &self,
        candidate: &MethodCandidate,
        receiver_ty: &InferenceType,
        arg_types: &[InferenceType],
        expected: Option<InferenceType>,
    ) -> bool {
        let mut probe = self.ctx.clone();
        if !self.compatible_with_error_tolerance(
            &mut probe,
            receiver_ty.clone(),
            candidate.receiver_type.clone(),
        ) {
            return false;
        }

        if let Some(expected) = expected
            && let Some(return_ty) = candidate.signature.return_type.clone()
            && !self.compatible_with_error_tolerance(
                &mut probe, return_ty, expected,
            )
        {
            return false;
        }

        for (arg_ty, param_ty) in
            arg_types.iter().zip(candidate.signature.param_types.iter())
        {
            if !self.compatible_with_error_tolerance(
                &mut probe,
                arg_ty.clone(),
                param_ty.clone(),
            ) {
                return false;
            }
        }
        true
    }

    fn compatible_with_error_tolerance(
        &self,
        probe: &mut InferenceContext,
        lhs: InferenceType,
        rhs: InferenceType,
    ) -> bool {
        if matches!(probe.resolve(lhs.clone()), InferenceType::Error)
            || matches!(probe.resolve(rhs.clone()), InferenceType::Error)
        {
            return true;
        }
        !matches!(probe.constrain_equal(lhs, rhs), InferenceType::Error)
    }

    fn record_selected_call_target(
        &mut self,
        call_expr_id: HirExprId,
        target: Option<InferredCallTarget>,
    ) {
        if let Some(target) = target {
            self.selected_call_targets.insert(call_expr_id, target);
        }
    }

    fn call_target_for_callee(
        &self,
        callee_expr_id: HirExprId,
        _signature: &InferFunctionSignature,
    ) -> Option<InferredCallTarget> {
        let callee_expr = self.module.exprs.get(&callee_expr_id)?;
        let resolution = self
            .hir_input
            .hir_path_table
            .by_expr(self.body_ref.file_id, callee_expr_id)?;

        match resolution {
            crate::frontend::resolver::HirPathResolution::Item(_) => {
                let path = match &callee_expr.kind {
                    HirExprKind::Path(path) => path.segments.clone(),
                    HirExprKind::NamespaceField { base, name, .. } => {
                        let base_expr = self.module.exprs.get(base)?;
                        let HirExprKind::Path(base_path) = &base_expr.kind else {
                            return None;
                        };
                        let mut segments = base_path.segments.clone();
                        segments.push(name.clone());
                        segments
                    }
                    _ => return None,
                };
                Some(InferredCallTarget::Function { path })
            }
            crate::frontend::resolver::HirPathResolution::AssociatedMember {
                type_item_ref,
                member_name,
                member_kind,
            } => Some(InferredCallTarget::AssociatedMember {
                type_item_id: self
                    .hir_input
                    .item_id_by_hir_item_ref
                    .get(&type_item_ref)
                    .copied(),
                member_name: member_name.clone(),
                member_kind,
            }),
            crate::frontend::resolver::HirPathResolution::Local(_) => None,
        }
    }

    fn contextual_enum_case_path_type(
        &self,
        expr_id: HirExprId,
        expected: Option<InferenceType>,
    ) -> Option<InferenceType> {
        let expected = expected?;
        let enum_item_id = self.expected_enum_item_id(expected)?;
        let expr = self.module.exprs.get(&expr_id)?;
        let HirExprKind::Path(path) = &expr.kind else {
            return None;
        };
        if path.segments.len() != 1 {
            return None;
        }
        let case_name = &path.segments[0];
        let enum_data = self.typed_items.enum_data(enum_item_id)?;
        let case = enum_data
            .case_signatures
            .iter()
            .find(|case| case.name == *case_name)?;
        if !case.payload_types.is_empty() {
            return None;
        }
        self.nominal_type_for_item(enum_item_id)
    }

    fn contextual_enum_case_call_type(
        &mut self,
        call_expr_id: HirExprId,
        callee_expr_id: HirExprId,
        args: &[HirCallArg],
        expected: Option<InferenceType>,
    ) -> Option<InferenceType> {
        let expected = expected?;
        let enum_item_id = self.expected_enum_item_id(expected.clone())?;
        let callee_expr = self.module.exprs.get(&callee_expr_id)?;
        let HirExprKind::Path(path) = &callee_expr.kind else {
            return None;
        };
        if path.segments.len() != 1 {
            return None;
        }
        let case_name = &path.segments[0];
        let enum_data = self.typed_items.enum_data(enum_item_id)?;
        let case = enum_data
            .case_signatures
            .iter()
            .find(|case| case.name == *case_name)?;
        if case.payload_types.len() != args.len() {
            return None;
        }

        for (arg, payload_type) in args.iter().zip(case.payload_types.iter()) {
            let expected_payload = inference_type_from_semantic(payload_type);
            let arg_ty =
                self.infer_expr(arg.value, Some(expected_payload.clone()));
            let _ = self.ctx.constrain_equal(arg_ty, expected_payload);
        }

        self.record_selected_call_target(
            call_expr_id,
            Some(InferredCallTarget::EnumCase {
                enum_item_id,
                case_name: case_name.clone(),
            }),
        );

        self.nominal_type_for_item(enum_item_id)
    }

    fn scoped_enum_case_path_type(
        &mut self,
        expr_id: HirExprId,
        case_name: String,
    ) -> Option<InferenceType> {
        let expr = self.module.exprs.get(&expr_id)?;
        let HirExprKind::Path(path) = &expr.kind else {
            return None;
        };

        if path.segments.len() != 1 {
            return None;
        }

        // Collect all enum types visible in this scope
        let mut candidate_enums = Vec::new();

        // Get visible enums from typed_items
        for (item_id, item_data) in self.typed_items.iter() {
            if let TypedItemData::Enum(enum_data) = item_data {
                // Check if this enum has a case with matching name
                let has_matching_case = enum_data
                    .case_signatures
                    .iter()
                    .any(|case| case.name == case_name);

                if has_matching_case {
                    candidate_enums
                        .push((item_id, format!("Enum{:?}", item_id)));
                }
            }
        }

        match candidate_enums.len() {
            0 => {
                // No enum in scope has this case
                let available_enums = self
                    .typed_items
                    .iter()
                    .filter_map(|(id, data)| {
                        if let TypedItemData::Enum(_) = data {
                            Some(format!("Enum{:?}", id))
                        } else {
                            None
                        }
                    })
                    .collect();

                self.issues.push(BodyInferIssue {
                    owner: self.body.owner.clone(),
                    body_index: self.body.body_index,
                    span: expr.origin.span,
                    kind: BodyInferIssueKind::MissingEnumCase {
                        case_name,
                        available_enums,
                    },
                });
                None
            }
            1 => {
                // Unique match - use this enum type
                let (enum_item_id, _) = candidate_enums[0];
                self.nominal_type_for_item(enum_item_id)
            }
            _ => {
                // Multiple matches - ambiguous
                self.issues.push(BodyInferIssue {
                    owner: self.body.owner.clone(),
                    body_index: self.body.body_index,
                    span: expr.origin.span,
                    kind: BodyInferIssueKind::AmbiguousEnumCase {
                        case_name,
                        candidates: candidate_enums,
                    },
                });
                None
            }
        }
    }

    fn scoped_enum_case_call_type(
        &mut self,
        call_expr_id: HirExprId,
        callee_expr_id: HirExprId,
        args: &[HirCallArg],
    ) -> Option<InferenceType> {
        let callee_expr = self.module.exprs.get(&callee_expr_id)?;
        let HirExprKind::Path(path) = &callee_expr.kind else {
            return None;
        };

        if path.segments.len() != 1 {
            return None;
        }

        let case_name = &path.segments[0];

        // Collect all enum types that have this case with matching arity
        let mut candidate_enums = Vec::new();

        for (item_id, item_data) in self.typed_items.iter() {
            if let TypedItemData::Enum(enum_data) = item_data {
                if let Some(case) = enum_data
                    .case_signatures
                    .iter()
                    .find(|c| c.name == *case_name)
                {
                    // Check if payload count matches
                    if case.payload_types.len() == args.len() {
                        candidate_enums.push((
                            item_id,
                            format!("Enum{:?}", item_id),
                            case.payload_types.clone(),
                        ));
                    }
                }
            }
        }

        match candidate_enums.len() {
            0 => {
                // No matching enum case
                None
            }
            1 => {
                // Unique match
                let (enum_item_id, _, payload_types) = &candidate_enums[0];

                // Infer arguments against payload types
                for (arg, payload_type) in args.iter().zip(payload_types.iter())
                {
                    let expected_payload =
                        ConcreteType::from_semantic_type(payload_type)
                            .map(InferenceType::Known)
                            .unwrap_or(InferenceType::Error);
                    let arg_ty = self
                        .infer_expr(arg.value, Some(expected_payload.clone()));
                    let _ = self.ctx.constrain_equal(arg_ty, expected_payload);
                }

                self.record_selected_call_target(
                    call_expr_id,
                    Some(InferredCallTarget::EnumCase {
                        enum_item_id: *enum_item_id,
                        case_name: case_name.clone(),
                    }),
                );

                self.nominal_type_for_item(*enum_item_id)
            }
            _ => {
                // Multiple matches - ambiguous
                let callee_expr = self.module.exprs.get(&callee_expr_id)?;
                self.issues.push(BodyInferIssue {
                    owner: self.body.owner.clone(),
                    body_index: self.body.body_index,
                    span: callee_expr.origin.span,
                    kind: BodyInferIssueKind::AmbiguousEnumCase {
                        case_name: case_name.clone(),
                        candidates: candidate_enums
                            .into_iter()
                            .map(|(id, name, _)| (id, name))
                            .collect(),
                    },
                });
                None
            }
        }
    }

    fn expected_enum_item_id(
        &self,
        expected: InferenceType,
    ) -> Option<crate::frontend::resolver::ItemId> {
        let mut probe = self.ctx.clone();
        match probe.resolve(expected) {
            InferenceType::Known(ConcreteType::Nominal { item_id, kind })
                if kind == crate::frontend::semantic::NamedTypeKind::Enum =>
            {
                Some(item_id)
            }
            _ => None,
        }
    }

    fn nominal_item_id(
        &self,
        ty: &InferenceType,
    ) -> Option<crate::frontend::resolver::ItemId> {
        let mut probe = self.ctx.clone();
        match probe.resolve(ty.clone()) {
            InferenceType::Known(ConcreteType::Nominal { item_id, .. }) => {
                Some(item_id)
            }
            _ => None,
        }
    }

    fn call_return_type(
        &self,
        callee_expr_id: HirExprId,
        signature: &InferFunctionSignature,
    ) -> InferenceType {
        if let Some(return_type) = &signature.return_type {
            return return_type.clone();
        }

        let Some(resolution) = self
            .hir_input
            .hir_path_table
            .by_expr(self.body_ref.file_id, callee_expr_id)
        else {
            return InferenceType::Known(ConcreteType::Builtin(
                BuiltinType::Void,
            ));
        };

        match resolution {
            crate::frontend::resolver::HirPathResolution::Item(item_ref) => {
                self.hir_input
                    .item_id_by_hir_item_ref
                    .get(&item_ref)
                    .copied()
                    .and_then(|item_id| self.nominal_type_for_item(item_id))
                    .unwrap_or_else(|| {
                        InferenceType::Known(ConcreteType::Builtin(BuiltinType::Void))
                    })
            }
            crate::frontend::resolver::HirPathResolution::AssociatedMember {
                type_item_ref,
                member_kind,
                ..
            } if member_kind == AssociatedMemberKind::Initializer => {
                self.hir_input
                    .item_id_by_hir_item_ref
                    .get(&type_item_ref)
                    .copied()
                    .and_then(|item_id| self.nominal_type_for_item(item_id))
                    .unwrap_or_else(|| {
                        InferenceType::Known(ConcreteType::Builtin(BuiltinType::Void))
                    })
            }
            crate::frontend::resolver::HirPathResolution::Local(_)
            | crate::frontend::resolver::HirPathResolution::AssociatedMember {
                ..
            } => InferenceType::Known(ConcreteType::Builtin(BuiltinType::Void)),
        }
    }

    fn nominal_type_for_item(
        &self,
        item_id: crate::frontend::resolver::ItemId,
    ) -> Option<InferenceType> {
        match self.typed_items.get(item_id)? {
            TypedItemData::Struct(_) => {
                Some(InferenceType::Known(ConcreteType::Nominal {
                    item_id,
                    kind: crate::frontend::semantic::NamedTypeKind::Struct,
                }))
            }
            TypedItemData::Enum(_) => {
                Some(InferenceType::Known(ConcreteType::Nominal {
                    item_id,
                    kind: crate::frontend::semantic::NamedTypeKind::Enum,
                }))
            }
            TypedItemData::Protocol(_) => {
                Some(InferenceType::Known(ConcreteType::Nominal {
                    item_id,
                    kind: crate::frontend::semantic::NamedTypeKind::Protocol,
                }))
            }
            TypedItemData::Function(_) => None,
        }
    }
}

fn collect_associated_signatures(
    output: &mut Vec<InferFunctionSignature>,
    methods: &[crate::frontend::semantic::TypedNamedFunctionSignature],
    initializers: &[TypedFunctionSignature],
    member_name: &str,
    member_kind: AssociatedMemberKind,
) {
    match member_kind {
        AssociatedMemberKind::Method => {
            output.extend(
                methods
                    .iter()
                    .filter(|method| method.name == member_name)
                    .map(|method| {
                        infer_function_signature_from_typed(&method.signature)
                    }),
            );
        }
        AssociatedMemberKind::Initializer => {
            if member_name == "init" {
                output.extend(
                    initializers
                        .iter()
                        .map(infer_function_signature_from_typed),
                );
            }
        }
    }
}

fn call_signature_matches(
    signature: &InferFunctionSignature,
    args: &[HirCallArg],
) -> bool {
    if signature.param_types.len() != args.len() {
        return false;
    }

    signature
        .param_labels
        .iter()
        .zip(args.iter())
        .all(|(label, arg)| match label {
            TypedParamLabel::Explicit(expected) => {
                arg.label.as_deref() == Some(expected.as_str())
            }
            TypedParamLabel::None | TypedParamLabel::FromName => {
                arg.label.is_none()
            }
        })
}

fn is_never_type(ctx: &mut InferenceContext, ty: &InferenceType) -> bool {
    matches!(
        ctx.resolve(ty.clone()),
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::Never))
    )
}

fn inference_type_from_semantic(ty: &Type) -> InferenceType {
    ConcreteType::from_semantic_type(ty)
        .map(InferenceType::Known)
        .unwrap_or(InferenceType::Error)
}

fn local_declared_span(
    body: &ResolvedBody,
    env: &BodyTypeEnvironment,
    hir_local_id: LocalId,
) -> Span {
    let Some(resolved_local_id) =
        env.resolved_local_id_for_hir_local(hir_local_id)
    else {
        return Span::new(0, 0);
    };

    body.locals
        .iter()
        .find(|local| local.id == resolved_local_id)
        .map(|local| local.declared_span)
        .unwrap_or_else(|| Span::new(0, 0))
}
