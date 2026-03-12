use crate::frontend::source::FileId;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// Structural scope-resolution failures for project-internal scope graphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    MissingRootFile {
        expected_path: PathBuf,
    },
    MissingDeclaredScope {
        parent_file_id: FileId,
        parent_scope_path: Vec<String>,
        declared_name: String,
        candidate_file: PathBuf,
        candidate_dir_file: PathBuf,
    },
    AmbiguousDeclaredScope {
        parent_file_id: FileId,
        parent_scope_path: Vec<String>,
        declared_name: String,
        file_candidate: PathBuf,
        dir_candidate: PathBuf,
    },
    ScopeCycle {
        cycle: Vec<FileId>,
    },
    NonUtf8Path,
}

impl Display for ResolveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRootFile { expected_path } => write!(
                f,
                "missing root scope file: {}",
                expected_path.display()
            ),
            Self::MissingDeclaredScope {
                parent_file_id,
                parent_scope_path,
                declared_name,
                candidate_file,
                candidate_dir_file,
            } => write!(
                f,
                "missing declared scope '{}' from file id {} at path {:?}; probed '{}' and '{}'",
                declared_name,
                parent_file_id.raw(),
                parent_scope_path,
                candidate_file.display(),
                candidate_dir_file.display()
            ),
            Self::AmbiguousDeclaredScope {
                parent_file_id,
                parent_scope_path,
                declared_name,
                file_candidate,
                dir_candidate,
            } => write!(
                f,
                "ambiguous declared scope '{}' from file id {} at path {:?}; both '{}' and '{}' exist",
                declared_name,
                parent_file_id.raw(),
                parent_scope_path,
                file_candidate.display(),
                dir_candidate.display()
            ),
            Self::ScopeCycle { cycle } => {
                let rendered = cycle
                    .iter()
                    .map(|id| id.raw().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                write!(f, "scope cycle detected: {rendered}")
            }
            Self::NonUtf8Path => write!(f, "encountered non-utf8 path"),
        }
    }
}

impl std::error::Error for ResolveError {}
