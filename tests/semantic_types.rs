use core_x::frontend::{BuiltinType, ItemId, Mutability, NamedTypeKind, Type};
use std::collections::{BTreeSet, HashSet};

#[test]
fn builtin_type_construction() {
    let ty = Type::builtin(BuiltinType::I32);
    assert_eq!(ty, Type::Builtin(BuiltinType::I32));
    assert_eq!(ty.to_string(), "i32");
}

#[test]
fn pointer_type_construction() {
    let pointee = Type::builtin(BuiltinType::Bool);
    let const_ptr = Type::pointer(pointee.clone(), Mutability::Const);
    let mut_ptr = Type::pointer(pointee, Mutability::Mut);

    assert_eq!(
        const_ptr,
        Type::Pointer {
            pointee: Box::new(Type::builtin(BuiltinType::Bool)),
            mutability: Mutability::Const,
        }
    );
    assert_eq!(const_ptr.to_string(), "*bool");
    assert_eq!(mut_ptr.to_string(), "*mut bool");
}

#[test]
fn named_item_type_construction_from_item_id() {
    let ty = Type::named(ItemId::new(42), NamedTypeKind::Struct);
    assert_eq!(
        ty,
        Type::Named {
            item_id: ItemId::new(42),
            kind: NamedTypeKind::Struct,
        }
    );
    assert_eq!(ty.to_string(), "struct#42");
}

#[test]
fn void_and_error_sentinel_behavior() {
    let void = Type::void();
    let error = Type::error();

    assert_eq!(void, Type::Builtin(BuiltinType::Void));
    assert_eq!(error, Type::Error);
    assert!(!void.is_error());
    assert!(error.is_error());
    assert_eq!(void.to_string(), "void");
    assert_eq!(error.to_string(), "<error>");
}

#[test]
fn type_order_hash_and_equality_are_deterministic() {
    let types = vec![
        Type::builtin(BuiltinType::I32),
        Type::named(ItemId::new(2), NamedTypeKind::Enum),
        Type::pointer(Type::builtin(BuiltinType::U8), Mutability::Mut),
        Type::void(),
        Type::error(),
        Type::named(ItemId::new(1), NamedTypeKind::Struct),
    ];

    let sorted_once = {
        let mut values = types.clone();
        values.sort();
        values
    };
    let sorted_twice = {
        let mut values = types.clone();
        values.sort();
        values
    };
    assert_eq!(sorted_once, sorted_twice);

    let tree = types.iter().cloned().collect::<BTreeSet<_>>();
    let hashed = types.iter().cloned().collect::<HashSet<_>>();
    assert_eq!(tree.len(), hashed.len());
    assert_eq!(tree.len(), types.len());
}
