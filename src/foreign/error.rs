use crate::dyld::DlError;
use crate::ffi::CallError;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum ForeignError {
    InvalidSignature { symbol: String, message: String },
    DuplicateDeclaration { symbol: String },
    SymbolResolve { symbol: String, source: DlError },
    Invocation { symbol: String, source: CallError },
}

impl Display for ForeignError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSignature { symbol, message } => {
                write!(
                    f,
                    "invalid declaration signature for foreign symbol {symbol}: {message}"
                )
            }
            Self::DuplicateDeclaration { symbol } => {
                write!(
                    f,
                    "foreign symbol {symbol} is already registered in this library"
                )
            }
            Self::SymbolResolve { symbol, source } => {
                write!(f, "failed to resolve foreign symbol {symbol}: {source}")
            }
            Self::Invocation { symbol, source } => {
                write!(f, "failed to invoke foreign symbol {symbol}: {source}")
            }
        }
    }
}

impl std::error::Error for ForeignError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidSignature { .. }
            | Self::DuplicateDeclaration { .. } => None,
            Self::SymbolResolve { source, .. } => Some(source),
            Self::Invocation { source, .. } => Some(source),
        }
    }
}
