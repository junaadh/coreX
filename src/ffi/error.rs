use super::types::NativeType;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum CallError {
    ArityMismatch {
        expected: usize,
        actual: usize,
    },
    TypeMismatch {
        index: usize,
        expected: NativeType,
        actual: &'static str,
    },
    UnsupportedType {
        ty: NativeType,
    },
    NullSymbol,
}

impl Display for CallError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ArityMismatch { expected, actual } => write!(
                f,
                "arity mismatch: expected {expected} argument(s), got {actual}"
            ),
            Self::TypeMismatch {
                index,
                expected,
                actual,
            } => write!(
                f,
                "type mismatch at argument {index}: expected {expected:?}, got {actual}"
            ),
            Self::UnsupportedType { ty } => {
                write!(f, "unsupported native type: {ty:?}")
            }
            Self::NullSymbol => write!(f, "cannot call null symbol"),
        }
    }
}

impl std::error::Error for CallError {}
