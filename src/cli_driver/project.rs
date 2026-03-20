use crate::cli_driver::DynError;
use core_x::frontend::parser::parse_source_file_with_recovery;
use core_x::frontend::resolver::ResolvedScopeKind;
use core_x::frontend::source::FileId;
use core_x::frontend::source::SourceDb;
use core_x::frontend::{
    DesugaredFile, FrontendAnalysis, FrontendContext, ImportRootKind,
    NamedImportRoot, ParseSessionError, ParsedFile, ProjectGraph,
    ProjectLoader, ProjectManifest, TargetRoots, analyze_project,
    build_target_roots, load_local_dependency_project_graph,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

type ParsedProjectFiles = (
    FrontendAnalysis,
    BTreeMap<PathBuf, FileId>,
    BTreeMap<FileId, PathBuf>,
);

pub struct ProjectContext {
    pub db: SourceDb,
    pub analysis: FrontendAnalysis,
    pub parsed_files: Vec<DesugaredFile>,
    pub ordered_file_ids: Vec<FileId>,
    pub path_by_file_id: BTreeMap<FileId, PathBuf>,
    pub library_target: Option<TargetSelection>,
    pub binary_targets: Vec<TargetSelection>,
}

#[derive(Clone)]
pub struct TargetSelection {
    pub kind: ResolvedScopeKind,
    pub label: &'static str,
    pub root_file_id: FileId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReachableScopeFileKind {
    RootLike,
    DirectoryBacked,
    FileBacked,
}

pub fn parse_single_file(
    path: &Path,
) -> Result<(SourceDb, DesugaredFile, FileId), DynError> {
    let source = fs::read_to_string(path)?;
    let mut context = FrontendContext::new();
    let file_id = context.add_file(path.to_path_buf(), source);
    let analysis = analyze_project(&mut context, &[file_id]).map_err(|error| {
        format_parse_session_error(
            &context,
            error,
            "failed to run frontend canonical analysis for",
        )
    })?;
    let desugared = analysis
        .desugared
        .iter()
        .find(|file| file.file_id == file_id)
        .cloned()
        .ok_or_else(|| {
            format!("missing desugared output for file {}", file_id.raw())
        })?;
    Ok((context.into_db(), desugared, file_id))
}

pub fn load_project_context(
    project_dir: &Path,
) -> Result<ProjectContext, DynError> {
    let loaded_project = ProjectLoader::load_project(project_dir)?;
    let project_graph =
        load_local_dependency_project_graph(loaded_project.clone())?;
    let target_roots = build_target_roots(&project_graph)?;
    let current_library_import_root =
        target_roots.by_name.iter().find_map(|(name, root)| {
            (root.kind == ImportRootKind::CurrentLibrary).then(|| name.clone())
        });

    let manifest = loaded_project.manifest.clone();
    let project_root = loaded_project.project_dir.clone();
    let project_files = collect_project_cx_files(&manifest)?;

    let mut frontend = FrontendContext::new();
    let mut file_id_by_path = BTreeMap::new();

    for absolute_path in project_files {
        let display_path =
            project_relative_or_absolute_path(&project_root, &absolute_path);
        let source = fs::read_to_string(&absolute_path)?;
        let file_id = frontend.add_file(display_path.clone(), source);

        file_id_by_path.insert(absolute_path, file_id);
    }

    let library_target = if let Some(target) = manifest.library.as_ref() {
        let file_id =
            file_id_by_path.get(&target.root_file).ok_or_else(|| {
                format!(
                    "missing target root in parsed project files: {}",
                    target.root_file.display()
                )
            })?;
        Some(TargetSelection {
            kind: ResolvedScopeKind::Root,
            label: "library",
            root_file_id: *file_id,
        })
    } else {
        None
    };

    let mut binary_targets = Vec::with_capacity(manifest.binaries.len());
    for target in &manifest.binaries {
        let file_id =
            file_id_by_path.get(&target.root_file).ok_or_else(|| {
                format!(
                    "missing target root in parsed project files: {}",
                    target.root_file.display()
                )
            })?;
        binary_targets.push(TargetSelection {
            kind: ResolvedScopeKind::BinaryRoot,
            label: "binary",
            root_file_id: *file_id,
        });
    }

    let dependency_named_roots =
        build_dependency_named_roots(&project_graph, &target_roots)?;

    for target in &binary_targets {
        frontend.set_root_kind(target.root_file_id, target.kind);
    }
    if let Some(target) = &library_target {
        frontend.set_root_kind(target.root_file_id, target.kind);
    }
    frontend.set_dependency_named_roots(dependency_named_roots.clone());
    frontend.configure_current_library_root(
        current_library_import_root.clone(),
        library_target.as_ref().map(|target| target.root_file_id),
    );

    let mut entry_file_ids = Vec::new();
    if let Some(target) = &library_target {
        entry_file_ids.push(target.root_file_id);
    }
    entry_file_ids.extend(binary_targets.iter().map(|target| target.root_file_id));
    if entry_file_ids.is_empty() {
        entry_file_ids.extend(frontend.ordered_file_ids().iter().copied());
    }

    let analysis = analyze_project(&mut frontend, &entry_file_ids).map_err(
        |error| {
            format_parse_session_error(
                &frontend,
                error,
                "failed to run frontend canonical analysis for project file",
            )
        },
    )?;
    let ordered_file_ids = frontend.ordered_file_ids().to_vec();
    let path_by_file_id = frontend.path_by_file_id().clone();
    let desugared_files = analysis.desugared.clone();

    Ok(ProjectContext {
        db: frontend.into_db(),
        analysis,
        parsed_files: desugared_files,
        ordered_file_ids,
        path_by_file_id,
        library_target,
        binary_targets,
    })
}

pub fn collect_project_cx_files(
    manifest: &ProjectManifest,
) -> Result<Vec<PathBuf>, DynError> {
    let mut files = BTreeSet::new();
    let mut queue = VecDeque::new();

    if let Some(library_target) = &manifest.library
        && files.insert(library_target.root_file.clone())
    {
        queue.push_back((
            library_target.root_file.clone(),
            ReachableScopeFileKind::RootLike,
        ));
    }
    for binary_target in &manifest.binaries {
        if files.insert(binary_target.root_file.clone()) {
            queue.push_back((
                binary_target.root_file.clone(),
                ReachableScopeFileKind::RootLike,
            ));
        }
    }

    while let Some((file_path, kind)) = queue.pop_front() {
        let source = fs::read_to_string(&file_path)?;
        let parsed = parse_source_file_with_recovery(&source).map_err(|error| {
            format!(
                "failed to initialize parser for manifest-reachable file {}: {error}",
                file_path.display()
            )
        })?;
        let child_base_dir = child_scope_base_dir(&file_path, kind)?;

        for declared_scope in collect_declared_scope_names(&parsed) {
            let file_candidate =
                child_base_dir.join(format!("{declared_scope}.cx"));
            let dir_candidate = child_base_dir
                .join(&declared_scope)
                .join(format!("{declared_scope}.cx"));

            let file_exists = file_candidate.is_file();
            let dir_exists = dir_candidate.is_file();

            if file_exists {
                let inserted = files.insert(file_candidate.clone());
                if inserted && !dir_exists {
                    queue.push_back((
                        file_candidate,
                        ReachableScopeFileKind::FileBacked,
                    ));
                }
            }
            if dir_exists {
                let inserted = files.insert(dir_candidate.clone());
                if inserted && !file_exists {
                    queue.push_back((
                        dir_candidate,
                        ReachableScopeFileKind::DirectoryBacked,
                    ));
                }
            }
        }
    }

    Ok(files.into_iter().collect())
}

fn child_scope_base_dir(
    scope_file: &Path,
    kind: ReachableScopeFileKind,
) -> Result<PathBuf, DynError> {
    match kind {
        ReachableScopeFileKind::RootLike
        | ReachableScopeFileKind::DirectoryBacked => {
            scope_file.parent().map(Path::to_path_buf).ok_or_else(|| {
                format!(
                    "scope file has no parent directory: {}",
                    scope_file.display()
                )
                .into()
            })
        }
        ReachableScopeFileKind::FileBacked => {
            let Some(parent) = scope_file.parent() else {
                return Err(format!(
                    "scope file has no parent directory: {}",
                    scope_file.display()
                )
                .into());
            };
            let Some(stem) = scope_file.file_stem() else {
                return Err(format!(
                    "scope file has no file stem: {}",
                    scope_file.display()
                )
                .into());
            };
            Ok(parent.join(stem))
        }
    }
}

fn collect_declared_scope_names(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .ast
        .items
        .iter()
        .filter_map(|item| match &item.node {
            core_x::frontend::ast::Item::Scope(scope_decl) => {
                Some(scope_decl.node.name.clone())
            }
            _ => None,
        })
        .collect()
}

fn project_relative_or_absolute_path(
    project_dir: &Path,
    absolute_path: &Path,
) -> PathBuf {
    absolute_path
        .strip_prefix(project_dir)
        .map_or_else(|_| absolute_path.to_path_buf(), Path::to_path_buf)
}

fn format_parse_session_error(
    context: &FrontendContext,
    error: ParseSessionError,
    message_prefix: &str,
) -> DynError {
    match error {
        ParseSessionError::MissingFile { file_id } => format!(
            "{} missing source file id {}",
            message_prefix,
            file_id.raw()
        )
        .into(),
        ParseSessionError::Parse(file_error) => {
            let path =
                context.path_for_file_id(file_error.file_id).map_or_else(
                    || format!("<unknown:{}>", file_error.file_id.raw()),
                    |path| path.display().to_string(),
                );
            format!("{} {}: {}", message_prefix, path, file_error.error).into()
        }
    }
}

pub fn build_dependency_named_roots(
    project_graph: &ProjectGraph,
    target_roots: &TargetRoots,
) -> Result<BTreeMap<String, NamedImportRoot>, DynError> {
    let mut named_roots = BTreeMap::new();

    for (name, root) in &target_roots.by_name {
        match root.kind {
            ImportRootKind::CurrentLibrary => {}
            ImportRootKind::UnloadedGitDependency => {
                named_roots
                    .insert(name.clone(), NamedImportRoot::UnloadedDependency);
            }
            ImportRootKind::LocalDependencyLibrary => {
                let dependency = project_graph
                    .local_dependencies
                    .iter()
                    .find(|dependency| dependency.dependency_name == *name)
                    .ok_or_else(|| {
                        format!(
                            "missing loaded local dependency project for root `{name}`"
                        )
                    })?;
                let library_target =
                    dependency.project.manifest.library.as_ref().ok_or_else(
                        || {
                            format!(
                                "dependency `{}` has no library target",
                                dependency.dependency_name
                            )
                        },
                    )?;
                let (analysis, file_id_by_path, path_by_file_id) =
                    parse_loaded_project_files(&dependency.project)?;
                let library_root_file_id = file_id_by_path
                    .get(&library_target.root_file)
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "missing dependency library root file {}",
                            library_target.root_file.display()
                        )
                    })?;
                let graph = analysis
                    .resolution_tables
                    .get(&library_root_file_id)
                    .map(|resolution| resolution.graph.clone())
                    .ok_or_else(|| {
                        format!(
                            "missing canonical resolution graph for dependency root {}",
                            library_target.root_file.display()
                        )
                    })?;
                named_roots.insert(
                    name.clone(),
                    NamedImportRoot::LoadedLibrary {
                        graph,
                        parsed_files: analysis.desugared.clone(),
                        path_by_file_id,
                    },
                );
            }
        }
    }

    Ok(named_roots)
}

fn parse_loaded_project_files(
    project: &core_x::frontend::LoadedProject,
) -> Result<ParsedProjectFiles, DynError> {
    let project_files = collect_project_cx_files(&project.manifest)?;
    let mut frontend = FrontendContext::new();
    let mut file_id_by_path = BTreeMap::new();

    for absolute_path in project_files {
        let source = fs::read_to_string(&absolute_path)?;
        let file_id = frontend.add_file(absolute_path.clone(), source);
        file_id_by_path.insert(absolute_path, file_id);
    }

    if let Some(library_target) = project.manifest.library.as_ref()
        && let Some(file_id) = file_id_by_path.get(&library_target.root_file)
    {
        frontend.set_root_kind(*file_id, ResolvedScopeKind::Root);
    }
    for binary_target in &project.manifest.binaries {
        if let Some(file_id) = file_id_by_path.get(&binary_target.root_file) {
            frontend.set_root_kind(*file_id, ResolvedScopeKind::BinaryRoot);
        }
    }

    let mut entry_file_ids = Vec::new();
    if let Some(library_target) = project.manifest.library.as_ref()
        && let Some(file_id) = file_id_by_path.get(&library_target.root_file)
    {
        entry_file_ids.push(*file_id);
    }
    for binary_target in &project.manifest.binaries {
        if let Some(file_id) = file_id_by_path.get(&binary_target.root_file) {
            entry_file_ids.push(*file_id);
        }
    }
    if entry_file_ids.is_empty() {
        entry_file_ids.extend(frontend.ordered_file_ids().iter().copied());
    }

    let analysis = analyze_project(&mut frontend, &entry_file_ids).map_err(
        |error| {
            format_parse_session_error(
                &frontend,
                error,
                "failed to run frontend canonical analysis for dependency file",
            )
        },
    )?;
    let path_by_file_id = frontend.path_by_file_id().clone();
    Ok((analysis, file_id_by_path, path_by_file_id))
}

pub fn classify_single_root_target(
    path: &Path,
) -> Result<(PathBuf, ResolvedScopeKind), DynError> {
    let canonical = fs::canonicalize(path)?;
    let file_name = canonical.file_name().and_then(|name| name.to_str());
    let root_kind = match file_name {
        Some("root.cx") => ResolvedScopeKind::Root,
        Some("main.cx") => ResolvedScopeKind::BinaryRoot,
        _ => {
            return Err(
                "single-file mode only supports src/root.cx or src/main.cx"
                    .into(),
            );
        }
    };

    let src_dir = canonical
        .parent()
        .ok_or("single-file mode path is missing parent directory")?;
    if src_dir.file_name().and_then(|name| name.to_str()) != Some("src") {
        return Err(
            "single-file mode only supports src/root.cx or src/main.cx".into(),
        );
    }

    let project_dir = src_dir
        .parent()
        .ok_or("single-file mode path is missing project root directory")?;
    Ok((project_dir.to_path_buf(), root_kind))
}

pub fn single_target_from_context(
    context: &ProjectContext,
    root_kind: ResolvedScopeKind,
) -> Result<TargetSelection, DynError> {
    match root_kind {
        ResolvedScopeKind::Root => context
            .library_target
            .clone()
            .ok_or_else(|| "project does not declare a library target".into()),
        ResolvedScopeKind::BinaryRoot => context
            .binary_targets
            .iter()
            .find(|target| {
                context
                    .path_by_file_id
                    .get(&target.root_file_id)
                    .is_some_and(|path| path == Path::new("src/main.cx"))
            })
            .cloned()
            .ok_or_else(|| {
                "project does not declare binary target rooted at src/main.cx"
                    .into()
            }),
        _ => Err("single target root must be library or binary".into()),
    }
}

pub fn targets_from_context(
    context: &ProjectContext,
) -> Result<Vec<TargetSelection>, DynError> {
    let mut targets = Vec::new();
    if let Some(library_target) = &context.library_target {
        targets.push(library_target.clone());
    }
    targets.extend(context.binary_targets.iter().cloned());

    if targets.is_empty() {
        return Err(
            "project manifest does not define any compilation targets".into()
        );
    }

    Ok(targets)
}

pub fn parsed_by_id(
    parsed_files: &[DesugaredFile],
) -> BTreeMap<FileId, &DesugaredFile> {
    parsed_files
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect()
}

pub fn path_for_file_id(context: &ProjectContext, file_id: FileId) -> String {
    context.path_by_file_id.get(&file_id).map_or_else(
        || format!("<unknown:{}>", file_id.raw()),
        |path| path.display().to_string(),
    )
}
