use super::ids::TypeVarId;
use super::types::{ConcreteType, InferenceType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceConstraint {
    Equal(InferenceType, InferenceType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceIssueKind {
    TypeMismatch {
        lhs: ConcreteType,
        rhs: ConcreteType,
    },
    OccursCheckFailed {
        var: TypeVarId,
        ty: InferenceType,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceIssue {
    pub kind: InferenceIssueKind,
}
