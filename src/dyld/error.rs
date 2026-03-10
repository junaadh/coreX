use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Debug)]
pub enum DlError {
    Open { path: PathBuf, message: String },
    Symbol { symbol: String, message: String },
    Close { message: String },
    InteriorNul { what: &'static str },
}

impl Display for DlError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, message } => {
                write!(f, "failed to open '{}' ({message})", path.display())
            }
            Self::Symbol { symbol, message } => {
                write!(f, "failed to resolve symbol '{symbol}' ({message})")
            }
            Self::Close { message } => {
                write!(f, "failed to close library ({message})")
            }
            Self::InteriorNul { what } => {
                write!(f, "interior NUL byte in {what}")
            }
        }
    }
}

impl std::error::Error for DlError {}
