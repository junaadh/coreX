use super::error::ProjectLoadError;
use super::loader::ProjectLoader;
use super::model::{DependencyKind, LoadedProject};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedDependencyProject {
    pub dependency_name: String,
    pub project: LoadedProject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectGraph {
    pub root_project: LoadedProject,
    pub local_dependencies: Vec<LoadedDependencyProject>,
}

/// Loads a one-level local dependency graph for a root project.
///
/// # Errors
///
/// Returns `ProjectLoadError` if any immediate path dependency cannot be
/// loaded as a valid project.
pub fn load_local_dependency_project_graph(
    root_project: LoadedProject,
) -> Result<ProjectGraph, ProjectLoadError> {
    let mut local_dependencies = Vec::new();

    for dependency in &root_project.manifest.dependencies {
        let DependencyKind::Path { path } = &dependency.kind else {
            continue;
        };

        let dependency_dir = if path.is_absolute() {
            normalize_lexical_path(path)
        } else {
            normalize_lexical_path(&root_project.project_dir.join(path))
        };
        let dependency_project = ProjectLoader::load_project(&dependency_dir)?;

        local_dependencies.push(LoadedDependencyProject {
            dependency_name: dependency.name.clone(),
            project: dependency_project,
        });
    }

    Ok(ProjectGraph {
        root_project,
        local_dependencies,
    })
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
