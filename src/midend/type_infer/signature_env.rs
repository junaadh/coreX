use super::types::{ConcreteType, InferenceType};
use crate::midend::type_check::signatures::{
    TypedFunctionSignature, TypedParamLabel,
};

/// Inference-ready view of one typed function signature.
///
/// This keeps labels and turns type surfaces into inference-domain types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferFunctionSignature {
    pub param_labels: Vec<TypedParamLabel>,
    pub param_types: Vec<InferenceType>,
    pub return_type: Option<InferenceType>,
}

#[must_use]
pub fn infer_function_signature_from_typed(
    signature: &TypedFunctionSignature,
) -> InferFunctionSignature {
    InferFunctionSignature {
        param_labels: signature.param_labels.clone(),
        param_types: signature
            .param_types
            .iter()
            .map(|ty| {
                ConcreteType::from_semantic_type(ty)
                    .map(InferenceType::Known)
                    .unwrap_or(InferenceType::Error)
            })
            .collect(),
        return_type: signature.return_type.as_ref().map(|ty| {
            ConcreteType::from_semantic_type(ty)
                .map(InferenceType::Known)
                .unwrap_or(InferenceType::Error)
        }),
    }
}
