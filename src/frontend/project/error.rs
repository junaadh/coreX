use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

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

impl ProjectLoadError {
    fn fmt_missing_manifest(
        f: &mut Formatter<'_>,
        manifest_path: &Path,
    ) -> std::fmt::Result {
        write!(f, "missing manifest file at {}", manifest_path.display())
    }

    fn fmt_invalid_manifest(
        f: &mut Formatter<'_>,
        manifest_path: &Path,
        message: &str,
    ) -> std::fmt::Result {
        write!(
            f,
            "invalid manifest at {}: {message}",
            manifest_path.display()
        )
    }

    fn fmt_manifest_read_failed(
        f: &mut Formatter<'_>,
        manifest_path: &Path,
        message: &str,
    ) -> std::fmt::Result {
        write!(
            f,
            "failed to read manifest at {}: {message}",
            manifest_path.display()
        )
    }

    fn fmt_workspace_member_missing_manifest(
        f: &mut Formatter<'_>,
        workspace_manifest: &Path,
        member_path: &Path,
    ) -> std::fmt::Result {
        write!(
            f,
            "workspace manifest {} references member without manifest: {}",
            workspace_manifest.display(),
            member_path.display()
        )
    }

    fn fmt_duplicate_binary_target_name(
        f: &mut Formatter<'_>,
        manifest_path: &Path,
        name: &str,
    ) -> std::fmt::Result {
        write!(
            f,
            "duplicate binary target name `{name}` in {}",
            manifest_path.display()
        )
    }

    fn fmt_duplicate_target_root(
        f: &mut Formatter<'_>,
        manifest_path: &Path,
        path: &Path,
    ) -> std::fmt::Result {
        write!(
            f,
            "duplicate target root {} in {}",
            path.display(),
            manifest_path.display()
        )
    }

    fn fmt_multiple_libraries_declared(
        f: &mut Formatter<'_>,
        manifest_path: &Path,
    ) -> std::fmt::Result {
        write!(
            f,
            "multiple library targets declared in {}",
            manifest_path.display()
        )
    }

    fn fmt_missing_target_root_file(
        f: &mut Formatter<'_>,
        manifest_path: &Path,
        target_name: &str,
        expected_path: &Path,
    ) -> std::fmt::Result {
        write!(
            f,
            "missing root file for target `{target_name}` in {}: expected {}",
            manifest_path.display(),
            expected_path.display()
        )
    }

    fn fmt_ambiguous_manifest_role(
        f: &mut Formatter<'_>,
        manifest_path: &Path,
    ) -> std::fmt::Result {
        write!(
            f,
            "manifest {} declares both workspace and project roles",
            manifest_path.display()
        )
    }

    fn fmt_unsupported_manifest_shape(
        f: &mut Formatter<'_>,
        manifest_path: &Path,
        message: &str,
    ) -> std::fmt::Result {
        write!(
            f,
            "unsupported manifest shape at {}: {message}",
            manifest_path.display()
        )
    }

    fn fmt_duplicate_import_root_name(
        f: &mut Formatter<'_>,
        project_manifest: &Path,
        name: &str,
    ) -> std::fmt::Result {
        write!(
            f,
            "duplicate import root name `{name}` in {}",
            project_manifest.display()
        )
    }
}

impl Display for ProjectLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingManifest { manifest_path } => {
                Self::fmt_missing_manifest(f, manifest_path)
            }
            Self::InvalidManifest {
                manifest_path,
                message,
            } => Self::fmt_invalid_manifest(f, manifest_path, message),
            Self::ManifestReadFailed {
                manifest_path,
                message,
            } => Self::fmt_manifest_read_failed(f, manifest_path, message),
            Self::WorkspaceMemberMissingManifest {
                workspace_manifest,
                member_path,
            } => Self::fmt_workspace_member_missing_manifest(
                f,
                workspace_manifest,
                member_path,
            ),
            Self::DuplicateBinaryTargetName {
                manifest_path,
                name,
            } => Self::fmt_duplicate_binary_target_name(f, manifest_path, name),
            Self::DuplicateTargetRoot {
                manifest_path,
                path,
            } => Self::fmt_duplicate_target_root(f, manifest_path, path),
            Self::MultipleLibrariesDeclared { manifest_path } => {
                Self::fmt_multiple_libraries_declared(f, manifest_path)
            }
            Self::MissingTargetRootFile {
                manifest_path,
                target_name,
                expected_path,
            } => Self::fmt_missing_target_root_file(
                f,
                manifest_path,
                target_name,
                expected_path,
            ),
            Self::AmbiguousManifestRole { manifest_path } => {
                Self::fmt_ambiguous_manifest_role(f, manifest_path)
            }
            Self::UnsupportedManifestShape {
                manifest_path,
                message,
            } => {
                Self::fmt_unsupported_manifest_shape(f, manifest_path, message)
            }
            Self::DuplicateImportRootName {
                project_manifest,
                name,
            } => {
                Self::fmt_duplicate_import_root_name(f, project_manifest, name)
            }
        }
    }
}

impl std::error::Error for ProjectLoadError {}
