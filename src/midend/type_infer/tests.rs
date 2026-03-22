use super::{
    ConcreteType, InferenceContext, InferenceType, LiteralDefaultKind,
    TypeVarId, infer_function_signature_from_typed,
};
use crate::frontend::resolver::ItemId;
use crate::midend::type_check::signatures::{
    TypedFunctionSignature, TypedParamLabel,
};
use crate::midend::type_check::{BuiltinType, Mutability, NamedTypeKind, Type};

#[test]
fn unifies_identical_concrete_types() {
    let mut ctx = InferenceContext::new();
    let lhs = InferenceType::Known(ConcreteType::Builtin(BuiltinType::I32));
    let rhs = InferenceType::Known(ConcreteType::Builtin(BuiltinType::I32));
    assert_eq!(ctx.constrain_equal(lhs.clone(), rhs), lhs);
    assert!(ctx.issues.is_empty());
}

#[test]
fn nominal_unification_requires_same_item_identity() {
    let mut ctx = InferenceContext::new();
    let lhs = InferenceType::Known(ConcreteType::Nominal {
        item_id: ItemId::new(1),
        kind: NamedTypeKind::Struct,
    });
    let rhs = InferenceType::Known(ConcreteType::Nominal {
        item_id: ItemId::new(2),
        kind: NamedTypeKind::Struct,
    });
    assert_eq!(ctx.constrain_equal(lhs, rhs), InferenceType::Error);
    assert_eq!(ctx.issues.len(), 1);
}

#[test]
fn pointer_types_unify_structurally() {
    let mut ctx = InferenceContext::new();
    let lhs = InferenceType::Known(ConcreteType::Pointer {
        pointee: Box::new(ConcreteType::Builtin(BuiltinType::I32)),
        mutability: Mutability::Const,
    });
    let rhs = InferenceType::Known(ConcreteType::Pointer {
        pointee: Box::new(ConcreteType::Builtin(BuiltinType::I32)),
        mutability: Mutability::Const,
    });
    assert_eq!(ctx.constrain_equal(lhs.clone(), rhs), lhs);
}

#[test]
fn optional_and_result_unify_structurally() {
    let mut ctx = InferenceContext::new();
    let optional = InferenceType::Known(ConcreteType::Optional(Box::new(
        ConcreteType::Builtin(BuiltinType::I16),
    )));
    let result = InferenceType::Known(ConcreteType::Result {
        ok: Box::new(ConcreteType::Builtin(BuiltinType::I16)),
        err: Box::new(ConcreteType::Builtin(BuiltinType::I32)),
    });
    assert_eq!(
        ctx.constrain_equal(optional.clone(), optional.clone()),
        optional
    );
    assert_eq!(ctx.constrain_equal(result.clone(), result.clone()), result);
}

#[test]
fn inference_var_unifies_with_concrete_and_var() {
    let mut ctx = InferenceContext::new();
    let v1 = ctx.fresh_type_var();
    let v2 = ctx.fresh_type_var();

    let _ = ctx.constrain_equal(InferenceType::Var(v1), InferenceType::Var(v2));
    let _ = ctx.constrain_equal(
        InferenceType::Var(v2),
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::I64)),
    );

    assert_eq!(
        ctx.resolve(InferenceType::Var(v1)),
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::I64))
    );
}

#[test]
fn error_type_absorbs_follow_on_failures() {
    let mut ctx = InferenceContext::new();
    let result = ctx.constrain_equal(
        InferenceType::Error,
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::I32)),
    );
    assert_eq!(result, InferenceType::Error);
    assert!(ctx.issues.is_empty());
}

#[test]
fn integer_literals_default_to_i32_when_unconstrained() {
    let mut ctx = InferenceContext::new();
    let v = ctx.fresh_type_var();
    ctx.mark_integer_literal_var(v);
    ctx.finalize();
    assert_eq!(
        ctx.resolve(InferenceType::Var(v)),
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::I32))
    );
}

#[test]
fn float_literals_default_to_f64_when_unconstrained() {
    let mut ctx = InferenceContext::new();
    let v = ctx.fresh_type_var();
    ctx.mark_float_literal_var(v);
    ctx.finalize();
    assert_eq!(
        ctx.resolve(InferenceType::Var(v)),
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::F64))
    );
}

#[test]
fn stronger_constraint_beats_literal_default() {
    let mut ctx = InferenceContext::new();
    let v = ctx.fresh_type_var();
    ctx.mark_integer_literal_var(v);
    let _ = ctx.constrain_equal(
        InferenceType::Var(v),
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::I64)),
    );
    ctx.finalize();
    assert_eq!(
        ctx.resolve(InferenceType::Var(v)),
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::I64))
    );
}

#[test]
fn finalize_hook_can_override_literal_defaults() {
    let mut ctx = InferenceContext::new();
    let v = ctx.fresh_type_var();
    ctx.mark_float_literal_var(v);
    ctx.finalize_with_hook(|var, kind| {
        if var == v && kind == LiteralDefaultKind::Float {
            return Some(InferenceType::Known(ConcreteType::Builtin(
                BuiltinType::F32,
            )));
        }
        None
    });
    assert_eq!(
        ctx.resolve(InferenceType::Var(v)),
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::F32))
    );
}

#[test]
fn self_unification_is_a_noop() {
    let mut ctx = InferenceContext::new();
    let v = TypeVarId::new(0);
    let _ = ctx.constrain_equal(InferenceType::Var(v), InferenceType::Var(v));
    assert_eq!(ctx.resolve(InferenceType::Var(v)), InferenceType::Var(v));
    assert!(ctx.issues.is_empty());
}

#[test]
fn typed_signature_adapter_preserves_labels_for_inference() {
    let typed = TypedFunctionSignature {
        param_labels: vec![
            TypedParamLabel::None,
            TypedParamLabel::Explicit("label".to_string()),
            TypedParamLabel::FromName,
        ],
        param_types: vec![
            Type::builtin(BuiltinType::I32),
            Type::builtin(BuiltinType::I64),
            Type::error(),
        ],
        return_type: Some(Type::builtin(BuiltinType::F64)),
    };
    let inferred = infer_function_signature_from_typed(&typed);
    assert_eq!(inferred.param_labels, typed.param_labels);
    assert_eq!(
        inferred.param_types[0],
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::I32))
    );
    assert_eq!(
        inferred.param_types[1],
        InferenceType::Known(ConcreteType::Builtin(BuiltinType::I64))
    );
    assert_eq!(inferred.param_types[2], InferenceType::Error);
    assert_eq!(
        inferred.return_type,
        Some(InferenceType::Known(ConcreteType::Builtin(
            BuiltinType::F64
        )))
    );
}
