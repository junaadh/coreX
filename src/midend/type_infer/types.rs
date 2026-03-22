use super::ids::TypeVarId;
use crate::frontend::resolver::ItemId;
use crate::midend::type_check::{BuiltinType, Mutability, NamedTypeKind, Type};

/// Concrete types handled by the inference engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConcreteType {
    Builtin(BuiltinType),
    Nominal {
        item_id: ItemId,
        kind: NamedTypeKind,
    },
    Pointer {
        pointee: Box<ConcreteType>,
        mutability: Mutability,
    },
    Optional(Box<ConcreteType>),
    Result {
        ok: Box<ConcreteType>,
        err: Box<ConcreteType>,
    },
}

impl ConcreteType {
    /// Converts a semantic type into a concrete inference type when representable.
    #[must_use]
    pub fn from_semantic_type(ty: &Type) -> Option<Self> {
        match ty {
            Type::Builtin(builtin) => Some(Self::Builtin(*builtin)),
            Type::Named { item_id, kind } => Some(Self::Nominal {
                item_id: *item_id,
                kind: *kind,
            }),
            Type::Pointer {
                pointee,
                mutability,
            } => Some(Self::Pointer {
                pointee: Box::new(Self::from_semantic_type(pointee)?),
                mutability: *mutability,
            }),
            Type::Error => None,
        }
    }

    /// Converts a concrete inference type back to semantic type when representable.
    #[must_use]
    pub fn to_semantic_type(&self) -> Option<Type> {
        match self {
            Self::Builtin(builtin) => Some(Type::builtin(*builtin)),
            Self::Nominal { item_id, kind } => {
                Some(Type::named(*item_id, *kind))
            }
            Self::Pointer {
                pointee,
                mutability,
            } => Some(Type::pointer(pointee.to_semantic_type()?, *mutability)),
            Self::Optional(_) | Self::Result { .. } => None,
        }
    }
}

/// Inference type domain:
/// - known concrete type
/// - inference variable
/// - error
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InferenceType {
    Known(ConcreteType),
    Var(TypeVarId),
    Error,
}

impl InferenceType {
    #[must_use]
    pub const fn error() -> Self {
        Self::Error
    }
}

impl From<ConcreteType> for InferenceType {
    fn from(value: ConcreteType) -> Self {
        Self::Known(value)
    }
}
