use super::error::ProjectLoadError;
use super::graph::ProjectGraph;
use super::model::DependencyKind;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportRootKind {
    CurrentLibrary,
    LocalDependencyLibrary,
    UnloadedGitDependency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRoot {
    pub name: String,
    pub kind: ImportRootKind,
    pub project_dir: PathBuf,
    pub root_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetRoots {
    pub by_name: BTreeMap<String, ImportRoot>,
}

impl TargetRoots {
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ImportRoot> {
        self.by_name.get(name)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

/// Builds deterministic named import roots for a project graph.
///
/// # Errors
///
/// Returns `ProjectLoadError::DuplicateImportRootName` when multiple sources
/// produce the same import root name.
pub fn build_target_roots(
    graph: &ProjectGraph,
) -> Result<TargetRoots, ProjectLoadError> {
    let mut by_name = BTreeMap::new();

    if let Some(library) = &graph.root_project.manifest.library {
        let root = ImportRoot {
            name: library.name.clone(),
            kind: ImportRootKind::CurrentLibrary,
            project_dir: graph.root_project.project_dir.clone(),
            root_file: Some(library.root_file.clone()),
        };
        insert_root(
            &mut by_name,
            graph.root_project.manifest_path.as_path(),
            root,
        )?;
    }

    for dependency in &graph.local_dependencies {
        let Some(library) = dependency.project.manifest.library.as_ref() else {
            continue;
        };

        let root = ImportRoot {
            name: dependency.dependency_name.clone(),
            kind: ImportRootKind::LocalDependencyLibrary,
            project_dir: dependency.project.project_dir.clone(),
            root_file: Some(library.root_file.clone()),
        };
        insert_root(
            &mut by_name,
            graph.root_project.manifest_path.as_path(),
            root,
        )?;
    }

    for dependency in &graph.root_project.manifest.dependencies {
        let DependencyKind::Git { .. } = dependency.kind else {
            continue;
        };

        let root = ImportRoot {
            name: dependency.name.clone(),
            kind: ImportRootKind::UnloadedGitDependency,
            project_dir: PathBuf::new(),
            root_file: None,
        };
        insert_root(
            &mut by_name,
            graph.root_project.manifest_path.as_path(),
            root,
        )?;
    }

    Ok(TargetRoots { by_name })
}

fn insert_root(
    roots: &mut BTreeMap<String, ImportRoot>,
    project_manifest: &std::path::Path,
    root: ImportRoot,
) -> Result<(), ProjectLoadError> {
    if roots.contains_key(&root.name) {
        return Err(ProjectLoadError::DuplicateImportRootName {
            project_manifest: project_manifest.to_path_buf(),
            name: root.name,
        });
    }

    roots.insert(root.name.clone(), root);
    Ok(())
}
