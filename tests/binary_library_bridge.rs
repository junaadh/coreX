use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    ImportResolveError, NamedImportRoot, ResolvedScopeKind,
    resolve_project_imports_with_named_roots, resolve_project_scopes,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    DesugaredFile, ImportBindingKind, ProjectLoader, SymbolKind,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct ParsedProject {
    db: SourceDb,
    parsed_files: Vec<DesugaredFile>,
    file_id_by_abs_path: BTreeMap<PathBuf, FileId>,
}

type BinaryImportResolution = (
    BTreeMap<FileId, core_x::frontend::ScopeSymbols>,
    BTreeMap<FileId, core_x::frontend::ResolvedImports>,
    ParsedProject,
);

fn unique_temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "corex_binary_library_bridge_{name}_{}_{}",
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
    entries.sort_by_key(std::fs::DirEntry::path);

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

fn parsed_to_desugared(
    parsed: core_x::frontend::ParsedFile,
) -> core_x::frontend::DesugaredFile {
    core_x::frontend::DesugaredFile {
        file_id: parsed.file_id,
        ast: parsed.ast,
        diagnostics: parsed.diagnostics,
        provenance_map: core_x::frontend::expansion::ProvenanceMap::new(
            parsed.file_id,
        ),
    }
}

fn parse_project(project_dir: &Path) -> ParsedProject {
    let mut files = BTreeSet::new();
    let src_dir = project_dir.join("src");
    if src_dir.is_dir() {
        collect_cx_files_recursive(&src_dir, &mut files);
    }

    let mut db = SourceDb::new();
    let mut parsed_files = Vec::new();
    let mut file_id_by_abs_path = BTreeMap::new();

    for abs_path in files {
        let display_path = abs_path
            .strip_prefix(project_dir)
            .map_or_else(|_| abs_path.clone(), Path::to_path_buf);
        let source = fs::read_to_string(&abs_path).expect("read source");
        let file_id = db.add_file(display_path, source);
        let file = db.file(file_id).expect("file should exist");
        let parsed = parse_source_file_from_source_file(file)
            .expect("strict parse should succeed");
        assert!(
            parsed.diagnostics.is_empty(),
            "strict parse should not emit diagnostics"
        );
        parsed_files.push(parsed_to_desugared(parsed));
        file_id_by_abs_path.insert(abs_path, file_id);
    }

    ParsedProject {
        db,
        parsed_files,
        file_id_by_abs_path,
    }
}

fn resolve_binary_imports_with_library_bridge(
    project_dir: &Path,
) -> Result<BinaryImportResolution, ImportResolveError> {
    let project =
        ProjectLoader::load_project(project_dir).expect("load project");
    let parsed = parse_project(project_dir);

    let main_path = project_dir.join("src/main.cx");
    let main_id = parsed
        .file_id_by_abs_path
        .get(&main_path)
        .copied()
        .expect("main file should be present");
    let binary_graph = resolve_project_scopes(
        &parsed.db,
        &parsed.parsed_files,
        main_id,
        ResolvedScopeKind::BinaryRoot,
    )
    .expect("binary graph should resolve");

    let mut named_roots = BTreeMap::new();
    if let Some(library) = &project.manifest.library {
        let library_id = parsed
            .file_id_by_abs_path
            .get(&library.root_file)
            .copied()
            .expect("library root should be present");
        let library_graph = resolve_project_scopes(
            &parsed.db,
            &parsed.parsed_files,
            library_id,
            ResolvedScopeKind::Root,
        )
        .expect("library graph should resolve");
        named_roots.insert(
            library.name.clone(),
            NamedImportRoot::LoadedLibrary {
                graph: library_graph,
                parsed_files: parsed.parsed_files.clone(),
                path_by_file_id: parsed
                    .file_id_by_abs_path
                    .iter()
                    .map(|(path, file_id)| (*file_id, path.clone()))
                    .collect(),
            },
        );
    }

    let result = resolve_project_imports_with_named_roots(
        &binary_graph,
        &parsed.parsed_files,
        &named_roots,
    );
    result.map(|(symbols, imports)| (symbols, imports, parsed))
}

#[test]
fn binary_can_import_current_library_by_library_target_name() {
    let root = unique_temp_dir("import_current_library");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/root.cx"), "scope net;\n");
    write_file(&root.join("src/net/net.cx"), "struct Client {}\n");
    write_file(&root.join("src/main.cx"), "use app::net::Client;\n");

    let (_, imports, parsed) =
        resolve_binary_imports_with_library_bridge(&root)
            .expect("binary imports should resolve");
    let main_id = *parsed
        .file_id_by_abs_path
        .get(&root.join("src/main.cx"))
        .expect("main file id");
    let binding = imports
        .get(&main_id)
        .and_then(|imports| imports.get("Client"))
        .expect("Client binding");

    assert_eq!(binding.kind, ImportBindingKind::Symbol(SymbolKind::Struct));
    assert_eq!(
        binding.target_path,
        vec!["net".to_string(), "Client".to_string()]
    );
}

#[test]
fn binary_root_still_refers_to_binary_scope_not_library_scope() {
    let root = unique_temp_dir("binary_root_stays_binary");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/root.cx"), "scope net;\n");
    write_file(&root.join("src/net/net.cx"), "struct Client {}\n");
    write_file(
        &root.join("src/main.cx"),
        "scope local; use root::local::Thing;\n",
    );
    write_file(&root.join("src/local.cx"), "struct Thing {}\n");

    let (_, imports, parsed) =
        resolve_binary_imports_with_library_bridge(&root)
            .expect("binary imports should resolve");
    let main_id = *parsed
        .file_id_by_abs_path
        .get(&root.join("src/main.cx"))
        .expect("main file id");
    let local_id = *parsed
        .file_id_by_abs_path
        .get(&root.join("src/local.cx"))
        .expect("local file id");
    let binding = imports
        .get(&main_id)
        .and_then(|imports| imports.get("Thing"))
        .expect("Thing binding");

    assert_eq!(binding.target_file_id, local_id);
    assert_eq!(
        binding.target_path,
        vec!["local".to_string(), "Thing".to_string()]
    );
}

#[test]
fn binary_does_not_see_library_without_explicit_import() {
    let root = unique_temp_dir("no_implicit_library_visibility");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/root.cx"), "scope net;\n");
    write_file(&root.join("src/net/net.cx"), "struct Client {}\n");
    write_file(&root.join("src/main.cx"), "fn main() {}\n");

    let (_, imports, parsed) =
        resolve_binary_imports_with_library_bridge(&root)
            .expect("binary imports should resolve");
    let main_id = *parsed
        .file_id_by_abs_path
        .get(&root.join("src/main.cx"))
        .expect("main file id");
    let main_imports = imports.get(&main_id).expect("main imports");
    assert!(main_imports.is_empty());
}

#[test]
fn binary_import_by_library_name_fails_when_project_has_no_library_target() {
    let root = unique_temp_dir("no_library_target");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/main.cx"), "use app::net::Client;\n");

    let error = resolve_binary_imports_with_library_bridge(&root)
        .expect_err("imports should fail");
    let parsed = parse_project(&root);
    let main_id = *parsed
        .file_id_by_abs_path
        .get(&root.join("src/main.cx"))
        .expect("main file id");

    assert!(matches!(
        error,
        ImportResolveError::UnknownRoot { from_file_id, root }
            if from_file_id == main_id && root == "app"
    ));
}

#[test]
fn binary_import_missing_library_path_reports_unresolved_path() {
    let root = unique_temp_dir("missing_library_path");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/root.cx"), "scope net;\n");
    write_file(&root.join("src/net/net.cx"), "struct Client {}\n");
    write_file(&root.join("src/main.cx"), "use app::missing::Thing;\n");

    let error = resolve_binary_imports_with_library_bridge(&root)
        .expect_err("imports should fail");
    let parsed = parse_project(&root);
    let main_id = *parsed
        .file_id_by_abs_path
        .get(&root.join("src/main.cx"))
        .expect("main file id");

    assert!(matches!(
        error,
        ImportResolveError::UnresolvedPath { from_file_id, path }
            if from_file_id == main_id
                && path == vec!["app".to_string(), "missing".to_string(), "Thing".to_string()]
    ));
}

#[test]
fn binary_target_name_is_not_an_import_root() {
    let root = unique_temp_dir("bin_name_not_root");
    write_file(
        &root.join("corex.toml"),
        r#"
[project]
name = "pkg"

[lib]
name = "app"

[[bin]]
name = "tool"
path = "src/main.cx"
"#,
    );
    write_file(&root.join("src/root.cx"), "fn lib_root() {}\n");
    write_file(&root.join("src/main.cx"), "use tool::Thing;\n");

    let error = resolve_binary_imports_with_library_bridge(&root)
        .expect_err("imports should fail");
    let parsed = parse_project(&root);
    let main_id = *parsed
        .file_id_by_abs_path
        .get(&root.join("src/main.cx"))
        .expect("main file id");

    assert!(matches!(
        error,
        ImportResolveError::UnknownRoot { from_file_id, root }
            if from_file_id == main_id && root == "tool"
    ));
}

#[test]
fn project_mode_cli_binary_import_resolution_uses_library_bridge() {
    let root = unique_temp_dir("cli_uses_library_bridge");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/root.cx"), "scope net;\n");
    write_file(&root.join("src/net/net.cx"), "struct Client {}\n");
    write_file(&root.join("src/main.cx"), "use app::net::Client;\n");

    let output = run_cxc(&[
        "dump".to_string(),
        "imports".to_string(),
        "--project".to_string(),
        arg(&root),
    ]);

    assert!(output.status.success(), "expected import dump to succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== target: binary (src/main.cx) =="));
    assert!(stdout.contains("local_name: \"Client\""));
}
