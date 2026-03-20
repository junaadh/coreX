use super::Type;
use super::body_env::{BodyLocalBindingInfo, BodyTypeEnvironmentTable};
use super::control_flow::{BodyControlFlowId, ControlFlowTable};
use super::expr_check::{BodyExprId, ExpressionTypeTable};
use super::hir_input::SemanticHirInput;
use super::stmt_check::StatementTypeTable;
use crate::frontend::hir::HirBodyId;
use crate::frontend::resolver::{BodyKind, DeclarationOwner, LocalId};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypedBodyId {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub hir_body_id: HirBodyId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypedBodyIssueKind {
    Environment,
    Expression,
    Statement,
    ControlFlow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedBodyIssueMarker {
    pub kind: TypedBodyIssueKind,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedBody {
    pub id: TypedBodyId,
    pub kind: BodyKind,
    pub containing_scope_file_id: FileId,
    pub expected_return_type: Type,
    pub body_result_type: Type,
    pub control_flow_is_compatible: bool,
    pub local_types: BTreeMap<LocalId, Type>,
    pub local_bindings: BTreeMap<LocalId, BodyLocalBindingInfo>,
    pub expression_types: BTreeMap<BodyExprId, Type>,
    pub issue_markers: Vec<TypedBodyIssueMarker>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedBodyTableIssueKind {
    MissingBodyReference,
    MissingBodyEnvironment,
    MissingControlFlowResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedBodyTableIssue {
    pub id: TypedBodyId,
    pub kind: TypedBodyTableIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedBodyTable {
    by_owner: BTreeMap<DeclarationOwner, Vec<TypedBody>>,
    hir_local_id_by_resolved_local_id_by_body:
        BTreeMap<(DeclarationOwner, usize), BTreeMap<LocalId, LocalId>>,
    pub issues: Vec<TypedBodyTableIssue>,
}

impl TypedBodyTable {
    #[must_use]
    pub fn bodies_for_owner(&self, owner: &DeclarationOwner) -> &[TypedBody] {
        self.by_owner.get(owner).map(Vec::as_slice).unwrap_or(&[])
    }

    #[must_use]
    pub fn body(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Option<&TypedBody> {
        self.bodies_for_owner(owner)
            .iter()
            .find(|body| body.id.body_index == body_index)
    }

    #[must_use]
    pub fn expression_type(&self, expr_id: &BodyExprId) -> Option<&Type> {
        self.by_owner
            .values()
            .flat_map(|bodies| bodies.iter())
            .find_map(|body| body.expression_types.get(expr_id))
    }

    #[must_use]
    pub fn local_type(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        local_id: LocalId,
    ) -> Option<&Type> {
        let hir_local_id = self
            .hir_local_id_by_resolved_local_id_by_body
            .get(&(owner.clone(), body_index))?
            .get(&local_id)?;
        self.local_type_for_hir_local(owner, body_index, *hir_local_id)
    }

    #[must_use]
    pub fn local_type_for_hir_local(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
        local_id: LocalId,
    ) -> Option<&Type> {
        self.body(owner, body_index)?.local_types.get(&local_id)
    }

    #[must_use]
    pub fn body_result_type(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Option<&Type> {
        Some(&self.body(owner, body_index)?.body_result_type)
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &TypedBody> {
        self.by_owner.values().flat_map(|bodies| bodies.iter())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_owner.values().map(Vec::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_owner.values().all(Vec::is_empty)
    }
}

#[must_use]
pub fn build_typed_body_table(
    hir_input: &SemanticHirInput,
    body_envs: &BodyTypeEnvironmentTable,
    expr_types: &ExpressionTypeTable,
    stmt_types: &StatementTypeTable,
    control_flow: &ControlFlowTable,
) -> TypedBodyTable {
    let mut by_owner: BTreeMap<DeclarationOwner, Vec<TypedBody>> =
        BTreeMap::new();
    let mut hir_local_id_by_resolved_local_id_by_body = BTreeMap::new();
    let mut issues = Vec::new();

    let mut grouped_envs: BTreeMap<DeclarationOwner, Vec<_>> = BTreeMap::new();
    for env in body_envs.iter() {
        grouped_envs.entry(env.owner.clone()).or_default().push(env);
    }

    for (owner, mut envs) in grouped_envs {
        envs.sort_by_key(|env| env.body_index);
        let mut typed_for_owner = Vec::with_capacity(envs.len());

        for env in envs {
            let Some(body_ref) = hir_input.body_ref(&owner, env.body_index)
            else {
                issues.push(TypedBodyTableIssue {
                    id: TypedBodyId {
                        owner: owner.clone(),
                        body_index: env.body_index,
                        hir_body_id: HirBodyId::new(0),
                    },
                    kind: TypedBodyTableIssueKind::MissingBodyReference,
                });
                continue;
            };

            let id = TypedBodyId {
                owner: owner.clone(),
                body_index: env.body_index,
                hir_body_id: body_ref.body_id,
            };

            let mut resolved_to_hir = BTreeMap::new();
            for hir_local_id in env.local_bindings.keys() {
                if let Some(resolved_local_id) =
                    env.resolved_local_id_for_hir_local(*hir_local_id)
                {
                    resolved_to_hir.insert(resolved_local_id, *hir_local_id);
                }
            }
            if !resolved_to_hir.is_empty() {
                hir_local_id_by_resolved_local_id_by_body
                    .insert((owner.clone(), env.body_index), resolved_to_hir);
            }

            let control = control_flow.body(&BodyControlFlowId {
                owner: owner.clone(),
                body_index: env.body_index,
                hir_body_id: body_ref.body_id,
            });
            if control.is_none() {
                issues.push(TypedBodyTableIssue {
                    id: id.clone(),
                    kind: TypedBodyTableIssueKind::MissingControlFlowResult,
                });
            }

            let expected_return_type = env.expected_return_type.clone();
            let body_result_type = control
                .map(|result| result.block_result_type.clone())
                .unwrap_or_else(Type::error);
            let control_flow_is_compatible =
                control.is_some_and(|result| result.is_compatible);

            let local_bindings = env.local_bindings.clone();
            let local_types = stmt_types
                .local_types_for_body(&owner, env.body_index)
                .cloned()
                .unwrap_or_else(|| env.local_types.clone());

            let mut expression_types = BTreeMap::new();
            for expr_id in expr_types.expr_ids_for_body(&owner, env.body_index)
            {
                if let Some(ty) = expr_types.expr_type(&expr_id).cloned() {
                    expression_types.insert(expr_id, ty);
                }
            }

            typed_for_owner.push(TypedBody {
                id,
                kind: env.kind,
                containing_scope_file_id: env.containing_scope_file_id,
                expected_return_type,
                body_result_type,
                control_flow_is_compatible,
                local_types,
                local_bindings,
                expression_types,
                issue_markers: issue_markers_for_body(
                    body_envs,
                    expr_types,
                    stmt_types,
                    control_flow,
                    &owner,
                    env.body_index,
                ),
            });
        }

        by_owner.insert(owner, typed_for_owner);
    }

    TypedBodyTable {
        by_owner,
        hir_local_id_by_resolved_local_id_by_body,
        issues,
    }
}

fn issue_markers_for_body(
    body_envs: &BodyTypeEnvironmentTable,
    expr_types: &ExpressionTypeTable,
    stmt_types: &StatementTypeTable,
    control_flow: &ControlFlowTable,
    owner: &DeclarationOwner,
    body_index: usize,
) -> Vec<TypedBodyIssueMarker> {
    let environment_count = body_envs
        .issues
        .iter()
        .filter(|issue| issue.owner == *owner && issue.body_index == body_index)
        .count();
    let expression_count = expr_types
        .issues
        .iter()
        .filter(|issue| issue.owner == *owner && issue.body_index == body_index)
        .count();
    let statement_count = stmt_types
        .issues
        .iter()
        .filter(|issue| issue.owner == *owner && issue.body_index == body_index)
        .count();
    let control_flow_count = control_flow
        .issues
        .iter()
        .filter(|issue| issue.owner == *owner && issue.body_index == body_index)
        .count();

    [
        (TypedBodyIssueKind::Environment, environment_count),
        (TypedBodyIssueKind::Expression, expression_count),
        (TypedBodyIssueKind::Statement, statement_count),
        (TypedBodyIssueKind::ControlFlow, control_flow_count),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(kind, count)| TypedBodyIssueMarker { kind, count })
    .collect()
}
