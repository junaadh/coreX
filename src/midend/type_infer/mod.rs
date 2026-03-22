//! Reusable core type inference engine.
//!
//! This module is intentionally independent from expression/body checking.
//! `midend::type_check` can use this as a driver backend.

mod body_infer;
mod constraints;
mod context;
mod defaults;
mod ids;
mod signature_env;
mod types;

pub use body_infer::{
    BodyInferIssue, BodyInferIssueKind, BodyInferenceTable, InferredCallTarget,
    infer_body_types,
};
pub use constraints::{
    InferenceConstraint, InferenceIssue, InferenceIssueKind,
};
pub use context::InferenceContext;
pub use defaults::{InferenceDefaults, LiteralDefaultKind};
pub use ids::TypeVarId;
pub use signature_env::{
    InferFunctionSignature, infer_function_signature_from_typed,
};
pub use types::{ConcreteType, InferenceType};

#[cfg(test)]
mod tests;
