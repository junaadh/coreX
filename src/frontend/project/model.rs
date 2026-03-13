use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyKind {
    Path { path: PathBuf },
    Git { git: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencySpec {
    pub name: String,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Library,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryTarget {
    pub name: String,
    pub root_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryTarget {
    pub name: String,
    pub root_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceManifest {
    pub name: String,
    pub members: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifest {
    pub name: String,
    pub library: Option<LibraryTarget>,
    pub binaries: Vec<BinaryTarget>,
    pub dependencies: Vec<DependencySpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedProject {
    pub project_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: ProjectManifest,
}
