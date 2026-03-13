use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectLoadError {
    MissingManifest {
        manifest_path: PathBuf,
    },
    InvalidManifest {
        manifest_path: PathBuf,
        message: String,
    },
    ManifestReadFailed {
        manifest_path: PathBuf,
        message: String,
    },
    WorkspaceMemberMissingManifest {
        workspace_manifest: PathBuf,
        member_path: PathBuf,
    },
    DuplicateBinaryTargetName {
        manifest_path: PathBuf,
        name: String,
    },
    DuplicateTargetRoot {
        manifest_path: PathBuf,
        path: PathBuf,
    },
    MultipleLibrariesDeclared {
        manifest_path: PathBuf,
    },
    MissingTargetRootFile {
        manifest_path: PathBuf,
        target_name: String,
        expected_path: PathBuf,
    },
    AmbiguousManifestRole {
        manifest_path: PathBuf,
    },
    UnsupportedManifestShape {
        manifest_path: PathBuf,
        message: String,
    },
    DuplicateImportRootName {
        project_manifest: PathBuf,
        name: String,
    },
}

impl Display for ProjectLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingManifest { manifest_path } => {
                write!(
                    f,
                    "missing manifest file at {}",
                    manifest_path.display()
                )
            }
            Self::InvalidManifest {
                manifest_path,
                message,
            } => {
                write!(
                    f,
                    "invalid manifest at {}: {}",
                    manifest_path.display(),
                    message
                )
            }
            Self::ManifestReadFailed {
                manifest_path,
                message,
            } => {
                write!(
                    f,
                    "failed to read manifest at {}: {}",
                    manifest_path.display(),
                    message
                )
            }
            Self::WorkspaceMemberMissingManifest {
                workspace_manifest,
                member_path,
            } => {
                write!(
                    f,
                    "workspace manifest {} references member without manifest: {}",
                    workspace_manifest.display(),
                    member_path.display()
                )
            }
            Self::DuplicateBinaryTargetName {
                manifest_path,
                name,
            } => {
                write!(
                    f,
                    "duplicate binary target name `{name}` in {}",
                    manifest_path.display()
                )
            }
            Self::DuplicateTargetRoot {
                manifest_path,
                path,
            } => {
                write!(
                    f,
                    "duplicate target root {} in {}",
                    path.display(),
                    manifest_path.display()
                )
            }
            Self::MultipleLibrariesDeclared { manifest_path } => {
                write!(
                    f,
                    "multiple library targets declared in {}",
                    manifest_path.display()
                )
            }
            Self::MissingTargetRootFile {
                manifest_path,
                target_name,
                expected_path,
            } => {
                write!(
                    f,
                    "missing root file for target `{target_name}` in {}: expected {}",
                    manifest_path.display(),
                    expected_path.display()
                )
            }
            Self::AmbiguousManifestRole { manifest_path } => {
                write!(
                    f,
                    "manifest {} declares both workspace and project roles",
                    manifest_path.display()
                )
            }
            Self::UnsupportedManifestShape {
                manifest_path,
                message,
            } => {
                write!(
                    f,
                    "unsupported manifest shape at {}: {}",
                    manifest_path.display(),
                    message
                )
            }
            Self::DuplicateImportRootName {
                project_manifest,
                name,
            } => {
                write!(
                    f,
                    "duplicate import root name `{name}` in {}",
                    project_manifest.display()
                )
            }
        }
    }
}

impl std::error::Error for ProjectLoadError {}
