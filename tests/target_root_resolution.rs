use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    ImportResolveError, NamedImportRoot, ResolvedScopeKind,
    resolve_project_imports_with_named_roots, resolve_project_scopes,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    ImportRootKind, LoadedProject, ParsedFile, ProjectGraph, ProjectLoadError,
    ProjectLoader, SymbolKind, TargetRoots, build_target_roots,
    load_local_dependency_project_graph,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct ParsedProject {
    db: SourceDb,
    parsed_files: Vec<ParsedFile>,
    file_id_by_absolute_path: BTreeMap<PathBuf, FileId>,
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "corex_target_roots_{name}_{}_{}",
        std::process::id(),
        nonce
    ));
    fs::create_dir_all(&path).expect("create temp directory");
    path
}

fn write_file(path: &Path, source: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, source).expect("write file");
}

fn run_cxc(args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cxc"))
        .args(args)
        .output()
        .expect("run cxc command")
}

fn arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn collect_cx_files_recursive(dir: &Path, out: &mut BTreeSet<PathBuf>) {
    let mut entries = fs::read_dir(dir)
        .expect("read directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("read entries");
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().expect("file type");
        if file_type.is_dir() {
            collect_cx_files_recursive(&path, out);
            continue;
        }
        if file_type.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("cx")
        {
            out.insert(path);
        }
    }
}

fn collect_project_cx_files(project: &LoadedProject) -> Vec<PathBuf> {
    let mut files = BTreeSet::new();
    let src_dir = project.project_dir.join("src");
    if src_dir.is_dir() {
        collect_cx_files_recursive(&src_dir, &mut files);
    }

    if let Some(library) = &project.manifest.library {
        files.insert(library.root_file.clone());
    }
    for binary in &project.manifest.binaries {
        files.insert(binary.root_file.clone());
    }

    files.into_iter().collect()
}

fn parse_loaded_project(project: &LoadedProject) -> ParsedProject {
    let mut db = SourceDb::new();
    let mut parsed_files = Vec::new();
    let mut file_id_by_absolute_path = BTreeMap::new();

    for absolute_path in collect_project_cx_files(project) {
        let display_path = absolute_path
            .strip_prefix(&project.project_dir)
            .map_or_else(|_| absolute_path.to_path_buf(), Path::to_path_buf);
        let source = fs::read_to_string(&absolute_path).expect("read source");
        let file_id = db.add_file(display_path, source);
        let file = db.file(file_id).expect("source file should exist");
        let parsed = parse_source_file_from_source_file(file)
            .expect("strict parse should succeed");
        assert!(
            parsed.diagnostics.is_empty(),
            "strict parse should not emit diagnostics"
        );
        parsed_files.push(parsed);
        file_id_by_absolute_path.insert(absolute_path, file_id);
    }

    ParsedProject {
        db,
        parsed_files,
        file_id_by_absolute_path,
    }
}

fn resolve_graph_for_root(
    parsed: &ParsedProject,
    root_file: &Path,
    kind: ResolvedScopeKind,
) -> core_x::frontend::ScopeGraph {
    let root_file_id = parsed
        .file_id_by_absolute_path
        .get(root_file)
        .copied()
        .expect("target root file should be parsed");
    resolve_project_scopes(&parsed.db, &parsed.parsed_files, root_file_id, kind)
        .expect("scope graph should resolve")
}

fn build_named_roots_for_target(
    project_graph: &ProjectGraph,
    target_roots: &TargetRoots,
    root_parsed: &ParsedProject,
    include_current_library: bool,
) -> BTreeMap<String, NamedImportRoot> {
    let mut named_roots = BTreeMap::new();

    for (name, root) in &target_roots.by_name {
        match root.kind {
            ImportRootKind::CurrentLibrary => {
                if !include_current_library {
                    continue;
                }
                let library = project_graph
                    .root_project
                    .manifest
                    .library
                    .as_ref()
                    .expect("current library should exist");
                let graph = resolve_graph_for_root(
                    root_parsed,
                    &library.root_file,
                    ResolvedScopeKind::Root,
                );
                named_roots.insert(
                    name.clone(),
                    NamedImportRoot::LoadedLibrary {
                        graph,
                        parsed_files: root_parsed.parsed_files.clone(),
                    },
                );
            }
            ImportRootKind::LocalDependencyLibrary => {
                let dependency = project_graph
                    .local_dependencies
                    .iter()
                    .find(|dependency| dependency.dependency_name == *name)
                    .expect("dependency should be loaded");
                let dependency_parsed =
                    parse_loaded_project(&dependency.project);
                let library = dependency
                    .project
                    .manifest
                    .library
                    .as_ref()
                    .expect("dependency library should exist");
                let graph = resolve_graph_for_root(
                    &dependency_parsed,
                    &library.root_file,
                    ResolvedScopeKind::Root,
                );
                named_roots.insert(
                    name.clone(),
                    NamedImportRoot::LoadedLibrary {
                        graph,
                        parsed_files: dependency_parsed.parsed_files,
                    },
                );
            }
            ImportRootKind::UnloadedGitDependency => {
                named_roots
                    .insert(name.clone(), NamedImportRoot::UnloadedDependency);
            }
        }
    }

    named_roots
}

#[test]
fn build_target_roots_includes_current_library_target() {
    let root = unique_temp_dir("roots_current_library");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/root.cx"), "fn root_entry() {}\n");

    let project = ProjectLoader::load_project(&root).expect("load project");
    let graph = load_local_dependency_project_graph(project)
        .expect("load project graph");
    let roots = build_target_roots(&graph).expect("build target roots");

    let current = roots.get("app").expect("current library root");
    assert_eq!(current.kind, ImportRootKind::CurrentLibrary);
    assert_eq!(current.project_dir, root);
    assert_eq!(current.root_file, Some(root.join("src/root.cx")));
}

#[test]
fn build_target_roots_includes_local_path_dependency_library() {
    let root = unique_temp_dir("roots_local_dependency");
    let util = root.join("util");
    let app = root.join("app");

    write_file(
        &app.join("corex.toml"),
        "[project]\nname = \"app\"\n[dependencies]\nutil = { path = \"../util\" }\n",
    );
    write_file(&util.join("corex.toml"), "[project]\nname = \"util\"\n");
    write_file(&util.join("src/root.cx"), "fn util_entry() {}\n");

    let project = ProjectLoader::load_project(&app).expect("load project");
    let graph = load_local_dependency_project_graph(project)
        .expect("load project graph");
    let roots = build_target_roots(&graph).expect("build target roots");

    let util_root = roots.get("util").expect("util root");
    assert_eq!(util_root.kind, ImportRootKind::LocalDependencyLibrary);
    assert_eq!(util_root.project_dir, util);
    assert_eq!(
        util_root.root_file,
        Some(util_root.project_dir.join("src/root.cx"))
    );
}

#[test]
fn build_target_roots_marks_git_dependency_as_unloaded() {
    let root = unique_temp_dir("roots_git_dependency");
    write_file(
        &root.join("corex.toml"),
        "[project]\nname = \"app\"\n[dependencies]\nhttp = { git = \"https://github.com/example/http.git\" }\n",
    );

    let project = ProjectLoader::load_project(&root).expect("load project");
    let graph = load_local_dependency_project_graph(project)
        .expect("load project graph");
    let roots = build_target_roots(&graph).expect("build target roots");

    let http = roots.get("http").expect("http root");
    assert_eq!(http.kind, ImportRootKind::UnloadedGitDependency);
    assert!(http.root_file.is_none());
}

#[test]
fn build_target_roots_rejects_duplicate_import_root_names() {
    let root = unique_temp_dir("roots_duplicate_name");
    write_file(
        &root.join("corex.toml"),
        r#"
[project]
name = "pkg"

[lib]
name = "app"

[dependencies]
app = { git = "https://github.com/example/app.git" }
"#,
    );
    write_file(&root.join("src/root.cx"), "fn root_entry() {}\n");

    let project = ProjectLoader::load_project(&root).expect("load project");
    let graph = load_local_dependency_project_graph(project)
        .expect("load project graph");
    let error = build_target_roots(&graph).expect_err("should fail");

    assert!(matches!(
        error,
        ProjectLoadError::DuplicateImportRootName { name, .. } if name == "app"
    ));
}

#[test]
fn binary_target_can_import_current_library_by_library_target_name() {
    let root = unique_temp_dir("binary_imports_current_library");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/root.cx"), "scope net;\n");
    write_file(&root.join("src/net.cx"), "struct Client {}\n");
    write_file(&root.join("src/main.cx"), "use app::net::Client;\n");

    let project = ProjectLoader::load_project(&root).expect("load project");
    let project_graph = load_local_dependency_project_graph(project.clone())
        .expect("load project graph");
    let target_roots =
        build_target_roots(&project_graph).expect("target roots");
    let parsed = parse_loaded_project(&project);
    let binary_graph = resolve_graph_for_root(
        &parsed,
        &root.join("src/main.cx"),
        ResolvedScopeKind::BinaryRoot,
    );
    let named_roots = build_named_roots_for_target(
        &project_graph,
        &target_roots,
        &parsed,
        true,
    );

    let (_, imports) = resolve_project_imports_with_named_roots(
        &binary_graph,
        &parsed.parsed_files,
        &named_roots,
    )
    .expect("resolve imports");
    let main_file_id = *parsed
        .file_id_by_absolute_path
        .get(&root.join("src/main.cx"))
        .expect("main file id");
    let binding = imports
        .get(&main_file_id)
        .and_then(|imports| imports.get("Client"))
        .expect("Client binding");
    assert_eq!(
        binding.kind,
        core_x::frontend::ImportBindingKind::Symbol(SymbolKind::Struct)
    );
    assert_eq!(
        binding.target_path,
        vec!["net".to_string(), "Client".to_string()]
    );
}

#[test]
fn local_dependency_import_root_resolves_into_dependency_scope_graph() {
    let root = unique_temp_dir("local_dependency_resolve");
    let util = root.join("util");
    let app = root.join("app");

    write_file(
        &app.join("corex.toml"),
        "[project]\nname = \"app\"\n[dependencies]\nutil = { path = \"../util\" }\n",
    );
    write_file(&app.join("src/main.cx"), "use util::fmt::Writer;\n");
    write_file(&util.join("corex.toml"), "[project]\nname = \"util\"\n");
    write_file(&util.join("src/root.cx"), "scope fmt;\n");
    write_file(&util.join("src/fmt.cx"), "struct Writer {}\n");

    let project = ProjectLoader::load_project(&app).expect("load project");
    let project_graph = load_local_dependency_project_graph(project.clone())
        .expect("load project graph");
    let target_roots =
        build_target_roots(&project_graph).expect("target roots");
    let parsed = parse_loaded_project(&project);
    let binary_graph = resolve_graph_for_root(
        &parsed,
        &app.join("src/main.cx"),
        ResolvedScopeKind::BinaryRoot,
    );
    let named_roots = build_named_roots_for_target(
        &project_graph,
        &target_roots,
        &parsed,
        false,
    );

    let (_, imports) = resolve_project_imports_with_named_roots(
        &binary_graph,
        &parsed.parsed_files,
        &named_roots,
    )
    .expect("resolve imports");
    let main_file_id = *parsed
        .file_id_by_absolute_path
        .get(&app.join("src/main.cx"))
        .expect("main file id");
    let binding = imports
        .get(&main_file_id)
        .and_then(|imports| imports.get("Writer"))
        .expect("Writer binding");
    assert_eq!(
        binding.kind,
        core_x::frontend::ImportBindingKind::Symbol(SymbolKind::Struct)
    );
    assert_eq!(
        binding.target_path,
        vec!["fmt".to_string(), "Writer".to_string()]
    );
}

#[test]
fn git_dependency_import_root_reports_unloaded_dependency_root() {
    let root = unique_temp_dir("git_dependency_unloaded");
    write_file(
        &root.join("corex.toml"),
        "[project]\nname = \"app\"\n[dependencies]\nhttp = { git = \"https://github.com/example/http.git\" }\n",
    );
    write_file(&root.join("src/main.cx"), "use http::Client;\n");

    let project = ProjectLoader::load_project(&root).expect("load project");
    let project_graph = load_local_dependency_project_graph(project.clone())
        .expect("load project graph");
    let target_roots =
        build_target_roots(&project_graph).expect("target roots");
    let parsed = parse_loaded_project(&project);
    let binary_graph = resolve_graph_for_root(
        &parsed,
        &root.join("src/main.cx"),
        ResolvedScopeKind::BinaryRoot,
    );
    let named_roots = build_named_roots_for_target(
        &project_graph,
        &target_roots,
        &parsed,
        false,
    );

    let error = resolve_project_imports_with_named_roots(
        &binary_graph,
        &parsed.parsed_files,
        &named_roots,
    )
    .expect_err("resolution should fail");
    let main_file_id = *parsed
        .file_id_by_absolute_path
        .get(&root.join("src/main.cx"))
        .expect("main file id");
    assert!(matches!(
        error,
        ImportResolveError::UnloadedDependencyRoot { from_file_id, root }
            if from_file_id == main_file_id && root == "http"
    ));
}

#[test]
fn nonexistent_named_root_reports_unknown_root() {
    let root = unique_temp_dir("unknown_root");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/main.cx"), "use missing::Thing;\n");

    let project = ProjectLoader::load_project(&root).expect("load project");
    let parsed = parse_loaded_project(&project);
    let binary_graph = resolve_graph_for_root(
        &parsed,
        &root.join("src/main.cx"),
        ResolvedScopeKind::BinaryRoot,
    );
    let named_roots = BTreeMap::new();

    let error = resolve_project_imports_with_named_roots(
        &binary_graph,
        &parsed.parsed_files,
        &named_roots,
    )
    .expect_err("resolution should fail");
    let main_file_id = *parsed
        .file_id_by_absolute_path
        .get(&root.join("src/main.cx"))
        .expect("main file id");
    assert!(matches!(
        error,
        ImportResolveError::UnknownRoot { from_file_id, root }
            if from_file_id == main_file_id && root == "missing"
    ));
}

#[test]
fn project_graph_loads_immediate_local_path_dependencies_only() {
    let root = unique_temp_dir("immediate_only");
    let helper = root.join("helper");
    let util = root.join("util");
    let app = root.join("app");

    write_file(
        &app.join("corex.toml"),
        "[project]\nname = \"app\"\n[dependencies]\nutil = { path = \"../util\" }\n",
    );
    write_file(
        &util.join("corex.toml"),
        "[project]\nname = \"util\"\n[dependencies]\nhelper = { path = \"../helper\" }\n",
    );
    write_file(&util.join("src/root.cx"), "fn util_entry() {}\n");
    write_file(&helper.join("corex.toml"), "[project]\nname = \"helper\"\n");
    write_file(&helper.join("src/root.cx"), "fn helper_entry() {}\n");

    let project = ProjectLoader::load_project(&app).expect("load project");
    let graph = load_local_dependency_project_graph(project)
        .expect("load project graph");

    assert_eq!(graph.local_dependencies.len(), 1);
    assert_eq!(graph.local_dependencies[0].dependency_name, "util");
    assert!(
        graph
            .local_dependencies
            .iter()
            .all(|dependency| dependency.dependency_name != "helper")
    );
}

#[test]
fn project_mode_cli_import_resolution_uses_manifest_target_roots() {
    let root = unique_temp_dir("cli_named_roots");
    let util = root.join("util");
    let app = root.join("app");

    write_file(
        &app.join("corex.toml"),
        "[project]\nname = \"app\"\n[dependencies]\nutil = { path = \"../util\" }\n",
    );
    write_file(&app.join("src/main.cx"), "use util::fmt::Writer;\n");
    write_file(&util.join("corex.toml"), "[project]\nname = \"util\"\n");
    write_file(&util.join("src/root.cx"), "scope fmt;\n");
    write_file(&util.join("src/fmt.cx"), "struct Writer {}\n");

    let output = run_cxc(&[
        "dump".to_string(),
        "imports".to_string(),
        "--project".to_string(),
        arg(&app),
    ]);

    assert!(output.status.success(), "expected import dump to succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== target: binary (src/main.cx) =="));
    assert!(stdout.contains("local_name: \"Writer\""));
}
