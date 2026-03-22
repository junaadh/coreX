use super::constraints::{
    InferenceConstraint, InferenceIssue, InferenceIssueKind,
};
use super::defaults::{InferenceDefaults, LiteralDefaultKind};
use super::ids::TypeVarId;
use super::types::{ConcreteType, InferenceType};
use std::collections::BTreeMap;

/// Inference context/state with variable allocation, substitutions,
/// constraints, and finalization/defaulting.
#[derive(Debug, Clone)]
pub struct InferenceContext {
    next_type_var: u32,
    bindings: BTreeMap<TypeVarId, InferenceType>,
    constraints: Vec<InferenceConstraint>,
    literal_default_hints: BTreeMap<TypeVarId, LiteralDefaultKind>,
    defaults: InferenceDefaults,
    pub issues: Vec<InferenceIssue>,
}

impl Default for InferenceContext {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceContext {
    #[must_use]
    pub fn new() -> Self {
        Self::with_defaults(InferenceDefaults::default())
    }

    #[must_use]
    pub fn with_defaults(defaults: InferenceDefaults) -> Self {
        Self {
            next_type_var: 0,
            bindings: BTreeMap::new(),
            constraints: Vec::new(),
            literal_default_hints: BTreeMap::new(),
            defaults,
            issues: Vec::new(),
        }
    }

    #[must_use]
    pub fn defaults(&self) -> InferenceDefaults {
        self.defaults
    }

    pub fn set_defaults(&mut self, defaults: InferenceDefaults) {
        self.defaults = defaults;
    }

    #[must_use]
    pub fn fresh_type_var(&mut self) -> TypeVarId {
        let id = TypeVarId::new(self.next_type_var);
        self.next_type_var = self.next_type_var.saturating_add(1);
        id
    }

    #[must_use]
    pub fn fresh_type(&mut self) -> InferenceType {
        InferenceType::Var(self.fresh_type_var())
    }

    #[must_use]
    pub fn constraints(&self) -> &[InferenceConstraint] {
        &self.constraints
    }

    #[must_use]
    pub fn binding(&mut self, var: TypeVarId) -> Option<InferenceType> {
        self.bindings.get(&var).cloned().map(|ty| self.resolve(ty))
    }

    pub fn mark_integer_literal_var(&mut self, var: TypeVarId) {
        self.literal_default_hints
            .entry(var)
            .or_insert(LiteralDefaultKind::Integer);
    }

    pub fn mark_float_literal_var(&mut self, var: TypeVarId) {
        self.literal_default_hints
            .entry(var)
            .or_insert(LiteralDefaultKind::Float);
    }

    pub fn mark_integer_literal(&mut self, ty: &InferenceType) {
        if let InferenceType::Var(var) = self.resolve(ty.clone()) {
            self.mark_integer_literal_var(var);
        }
    }

    pub fn mark_float_literal(&mut self, ty: &InferenceType) {
        if let InferenceType::Var(var) = self.resolve(ty.clone()) {
            self.mark_float_literal_var(var);
        }
    }

    pub fn record_constraint(&mut self, constraint: InferenceConstraint) {
        self.constraints.push(constraint);
    }

    #[must_use]
    pub fn constrain_equal(
        &mut self,
        lhs: InferenceType,
        rhs: InferenceType,
    ) -> InferenceType {
        self.record_constraint(InferenceConstraint::Equal(
            lhs.clone(),
            rhs.clone(),
        ));
        self.unify(lhs, rhs)
    }

    /// Resolves substitutions for `ty` with path compression.
    #[must_use]
    pub fn resolve(&mut self, ty: InferenceType) -> InferenceType {
        match ty {
            InferenceType::Var(var) => {
                let Some(bound) = self.bindings.get(&var).cloned() else {
                    return InferenceType::Var(var);
                };
                let resolved = self.resolve(bound);
                self.bindings.insert(var, resolved.clone());
                resolved
            }
            known_or_error => known_or_error,
        }
    }

    #[must_use]
    pub fn unify(
        &mut self,
        lhs: InferenceType,
        rhs: InferenceType,
    ) -> InferenceType {
        let lhs = self.resolve(lhs);
        let rhs = self.resolve(rhs);
        match (lhs, rhs) {
            (InferenceType::Error, _) | (_, InferenceType::Error) => {
                InferenceType::Error
            }
            (InferenceType::Var(var), ty) | (ty, InferenceType::Var(var)) => {
                self.bind_var(var, ty)
            }
            (InferenceType::Known(lhs), InferenceType::Known(rhs)) => {
                match self.unify_concrete(lhs.clone(), rhs.clone()) {
                    Some(known) => InferenceType::Known(known),
                    None => {
                        self.issues.push(InferenceIssue {
                            kind: InferenceIssueKind::TypeMismatch { lhs, rhs },
                        });
                        InferenceType::Error
                    }
                }
            }
        }
    }

    pub fn finalize(&mut self) {
        self.finalize_with_hook(|_, _| None);
    }

    pub fn finalize_with_hook<F>(&mut self, mut hook: F)
    where
        F: FnMut(TypeVarId, LiteralDefaultKind) -> Option<InferenceType>,
    {
        let hints = self.literal_default_hints.clone();
        for (var, hint) in hints {
            let resolved = self.resolve(InferenceType::Var(var));
            let InferenceType::Var(root_var) = resolved else {
                continue;
            };

            let chosen = hook(root_var, hint).unwrap_or_else(|| {
                let default_builtin = match hint {
                    LiteralDefaultKind::Integer => {
                        self.defaults.integer_default
                    }
                    LiteralDefaultKind::Float => self.defaults.float_default,
                };
                InferenceType::Known(ConcreteType::Builtin(default_builtin))
            });
            let _ = self.bind_var(root_var, chosen);
        }
    }

    fn bind_var(&mut self, var: TypeVarId, ty: InferenceType) -> InferenceType {
        let ty = self.resolve(ty);
        if ty == InferenceType::Var(var) {
            return InferenceType::Var(var);
        }
        if self.occurs_in(var, &ty) {
            self.issues.push(InferenceIssue {
                kind: InferenceIssueKind::OccursCheckFailed {
                    var,
                    ty: ty.clone(),
                },
            });
            self.bindings.insert(var, InferenceType::Error);
            return InferenceType::Error;
        }
        self.bindings.insert(var, ty.clone());
        ty
    }

    fn occurs_in(&mut self, var: TypeVarId, ty: &InferenceType) -> bool {
        match self.resolve(ty.clone()) {
            InferenceType::Var(other) => other == var,
            InferenceType::Known(_) | InferenceType::Error => false,
        }
    }

    fn unify_concrete(
        &mut self,
        lhs: ConcreteType,
        rhs: ConcreteType,
    ) -> Option<ConcreteType> {
        match (lhs, rhs) {
            (ConcreteType::Builtin(a), ConcreteType::Builtin(b)) if a == b => {
                Some(ConcreteType::Builtin(a))
            }
            (
                ConcreteType::Nominal {
                    item_id: a_item,
                    kind: a_kind,
                },
                ConcreteType::Nominal {
                    item_id: b_item,
                    kind: b_kind,
                },
            ) if a_item == b_item && a_kind == b_kind => {
                Some(ConcreteType::Nominal {
                    item_id: a_item,
                    kind: a_kind,
                })
            }
            (
                ConcreteType::Pointer {
                    pointee: a_pointee,
                    mutability: a_mutability,
                },
                ConcreteType::Pointer {
                    pointee: b_pointee,
                    mutability: b_mutability,
                },
            ) if a_mutability == b_mutability => {
                let pointee = self.unify_concrete(*a_pointee, *b_pointee)?;
                Some(ConcreteType::Pointer {
                    pointee: Box::new(pointee),
                    mutability: a_mutability,
                })
            }
            (ConcreteType::Optional(a), ConcreteType::Optional(b)) => {
                let inner = self.unify_concrete(*a, *b)?;
                Some(ConcreteType::Optional(Box::new(inner)))
            }
            (
                ConcreteType::Result {
                    ok: a_ok,
                    err: a_err,
                },
                ConcreteType::Result {
                    ok: b_ok,
                    err: b_err,
                },
            ) => {
                let ok = self.unify_concrete(*a_ok, *b_ok)?;
                let err = self.unify_concrete(*a_err, *b_err)?;
                Some(ConcreteType::Result {
                    ok: Box::new(ok),
                    err: Box::new(err),
                })
            }
            _ => None,
        }
    }
}
