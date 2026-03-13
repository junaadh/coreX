use super::error::ProjectLoadError;
use super::manifest::{
    MANIFEST_FILE_NAME, ManifestRole, RawProjectManifest, parse_manifest_toml,
};
use super::model::{
    BinaryTarget, LibraryTarget, LoadedProject, ProjectManifest,
    WorkspaceManifest,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub struct ProjectLoader;

impl ProjectLoader {
    pub fn load_workspace_manifest(
        dir: &Path,
    ) -> Result<WorkspaceManifest, ProjectLoadError> {
        let (manifest_path, role) = load_manifest_role(dir)?;
        match role {
            ManifestRole::Workspace(manifest) => Ok(manifest),
            ManifestRole::Project(_) => {
                Err(ProjectLoadError::UnsupportedManifestShape {
                    manifest_path,
                    message:
                        "expected workspace manifest, found project manifest"
                            .to_string(),
                })
            }
        }
    }

    pub fn load_project_manifest(
        dir: &Path,
    ) -> Result<ProjectManifest, ProjectLoadError> {
        let (project_dir, manifest_path, role) = load_manifest_for_dir(dir)?;
        match role {
            ManifestRole::Project(raw) => {
                normalize_project_manifest(&project_dir, &manifest_path, raw)
            }
            ManifestRole::Workspace(_) => {
                Err(ProjectLoadError::UnsupportedManifestShape {
                    manifest_path,
                    message:
                        "expected project manifest, found workspace manifest"
                            .to_string(),
                })
            }
        }
    }

    pub fn load_project(dir: &Path) -> Result<LoadedProject, ProjectLoadError> {
        let (project_dir, manifest_path, role) = load_manifest_for_dir(dir)?;
        let manifest = match role {
            ManifestRole::Project(raw) => {
                normalize_project_manifest(&project_dir, &manifest_path, raw)?
            }
            ManifestRole::Workspace(_) => {
                return Err(ProjectLoadError::UnsupportedManifestShape {
                    manifest_path,
                    message:
                        "expected project manifest, found workspace manifest"
                            .to_string(),
                });
            }
        };

        Ok(LoadedProject {
            project_dir,
            manifest_path,
            manifest,
        })
    }
}

pub fn load_project_from_dir(
    dir: &Path,
) -> Result<LoadedProject, ProjectLoadError> {
    ProjectLoader::load_project(dir)
}

fn load_manifest_role(
    dir: &Path,
) -> Result<(PathBuf, ManifestRole), ProjectLoadError> {
    let (_, manifest_path, role) = load_manifest_for_dir(dir)?;
    Ok((manifest_path, role))
}

fn load_manifest_for_dir(
    dir: &Path,
) -> Result<(PathBuf, PathBuf, ManifestRole), ProjectLoadError> {
    let project_dir = absolute_lexical_path(dir)?;
    let manifest_path = join_and_normalize(&project_dir, MANIFEST_FILE_NAME);
    let source = read_manifest(&manifest_path)?;
    let role = parse_manifest_toml(&manifest_path, &source)?;
    Ok((project_dir, manifest_path, role))
}

fn read_manifest(manifest_path: &Path) -> Result<String, ProjectLoadError> {
    match fs::read_to_string(manifest_path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(ProjectLoadError::MissingManifest {
                manifest_path: manifest_path.to_path_buf(),
            })
        }
        Err(error) => Err(ProjectLoadError::ManifestReadFailed {
            manifest_path: manifest_path.to_path_buf(),
            message: error.to_string(),
        }),
    }
}

fn normalize_project_manifest(
    project_dir: &Path,
    manifest_path: &Path,
    raw: RawProjectManifest,
) -> Result<ProjectManifest, ProjectLoadError> {
    let mut library = None;
    let mut binaries = Vec::new();

    let project_name = raw.name;
    let root_path = join_and_normalize(project_dir, "src/root.cx");
    if raw.library_declared {
        let library_name =
            raw.library_name.unwrap_or_else(|| project_name.clone());
        ensure_target_file_exists(manifest_path, &library_name, &root_path)?;
        library = Some(LibraryTarget {
            name: library_name,
            root_file: root_path.clone(),
        });
    } else if root_path.is_file() {
        library = Some(LibraryTarget {
            name: project_name.clone(),
            root_file: root_path.clone(),
        });
    }

    let main_path = join_and_normalize(project_dir, "src/main.cx");
    let mut explicit_covers_default_main = false;

    for bin in raw.binaries {
        let bin_root = join_or_normalize(project_dir, &bin.path);
        ensure_target_file_exists(manifest_path, &bin.name, &bin_root)?;
        if bin_root == main_path || bin.name == project_name {
            explicit_covers_default_main = true;
        }
        binaries.push(BinaryTarget {
            name: bin.name,
            root_file: bin_root,
        });
    }

    if main_path.is_file() && !explicit_covers_default_main {
        binaries.push(BinaryTarget {
            name: project_name.clone(),
            root_file: main_path,
        });
    }

    validate_unique_binary_names(manifest_path, &binaries)?;
    validate_unique_target_roots(manifest_path, library.as_ref(), &binaries)?;

    Ok(ProjectManifest {
        name: project_name,
        library,
        binaries,
        dependencies: raw.dependencies,
    })
}

fn validate_unique_binary_names(
    manifest_path: &Path,
    binaries: &[BinaryTarget],
) -> Result<(), ProjectLoadError> {
    let mut names = BTreeSet::new();
    for binary in binaries {
        if !names.insert(binary.name.clone()) {
            return Err(ProjectLoadError::DuplicateBinaryTargetName {
                manifest_path: manifest_path.to_path_buf(),
                name: binary.name.clone(),
            });
        }
    }
    Ok(())
}

fn validate_unique_target_roots(
    manifest_path: &Path,
    library: Option<&LibraryTarget>,
    binaries: &[BinaryTarget],
) -> Result<(), ProjectLoadError> {
    let mut roots = BTreeSet::new();
    if let Some(library) = library {
        roots.insert(library.root_file.clone());
    }

    for binary in binaries {
        if !roots.insert(binary.root_file.clone()) {
            return Err(ProjectLoadError::DuplicateTargetRoot {
                manifest_path: manifest_path.to_path_buf(),
                path: binary.root_file.clone(),
            });
        }
    }

    Ok(())
}

fn ensure_target_file_exists(
    manifest_path: &Path,
    target_name: &str,
    expected_path: &Path,
) -> Result<(), ProjectLoadError> {
    if expected_path.is_file() {
        Ok(())
    } else {
        Err(ProjectLoadError::MissingTargetRootFile {
            manifest_path: manifest_path.to_path_buf(),
            target_name: target_name.to_string(),
            expected_path: expected_path.to_path_buf(),
        })
    }
}

fn absolute_lexical_path(path: &Path) -> Result<PathBuf, ProjectLoadError> {
    if path.is_absolute() {
        return Ok(normalize_lexical_path(path));
    }

    let cwd = std::env::current_dir().map_err(|error| {
        ProjectLoadError::ManifestReadFailed {
            manifest_path: path.join(MANIFEST_FILE_NAME),
            message: error.to_string(),
        }
    })?;
    Ok(normalize_lexical_path(&cwd.join(path)))
}

fn join_and_normalize(base: &Path, child: impl AsRef<Path>) -> PathBuf {
    normalize_lexical_path(&base.join(child))
}

fn join_or_normalize(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize_lexical_path(path)
    } else {
        join_and_normalize(base, path)
    }
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                );
                if can_pop {
                    normalized.pop();
                } else if !normalized.is_absolute() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}
