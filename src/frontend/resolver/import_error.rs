use crate::frontend::source::FileId;
use std::fmt::{Display, Formatter};

/// Structural import-resolution failures for project-local scope graphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportResolveError {
    UnknownRoot {
        from_file_id: FileId,
        root: String,
    },
    UnloadedDependencyRoot {
        from_file_id: FileId,
        root: String,
    },
    UnresolvedPath {
        from_file_id: FileId,
        path: Vec<String>,
    },
    InvalidSelfImport {
        from_file_id: FileId,
    },
    InvalidGlobTarget {
        from_file_id: FileId,
        path: Vec<String>,
    },
    DuplicateBinding {
        file_id: FileId,
        binding_name: String,
    },
}

impl Display for ImportResolveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRoot { from_file_id, root } => write!(
                f,
                "unknown import root '{}' in file id {}",
                root,
                from_file_id.raw()
            ),
            Self::UnloadedDependencyRoot { from_file_id, root } => write!(
                f,
                "import root '{}' is declared but dependency is not loaded (file id {})",
                root,
                from_file_id.raw()
            ),
            Self::UnresolvedPath { from_file_id, path } => write!(
                f,
                "unresolved import path '{}' in file id {}",
                path.join("::"),
                from_file_id.raw()
            ),
            Self::InvalidSelfImport { from_file_id } => write!(
                f,
                "invalid self import form in file id {}",
                from_file_id.raw()
            ),
            Self::InvalidGlobTarget { from_file_id, path } => write!(
                f,
                "invalid glob target '{}' in file id {}",
                path.join("::"),
                from_file_id.raw()
            ),
            Self::DuplicateBinding {
                file_id,
                binding_name,
            } => write!(
                f,
                "duplicate import binding '{}' in file id {}",
                binding_name,
                file_id.raw()
            ),
        }
    }
}

impl std::error::Error for ImportResolveError {}
