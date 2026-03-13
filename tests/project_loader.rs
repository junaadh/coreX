use core_x::frontend::{
    DependencyKind, ProjectLoadError, ProjectLoader, load_project_from_dir,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "corex_project_loader_{name}_{}_{}",
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

#[test]
fn load_minimal_project_with_implicit_lib_and_bin_defaults() {
    let root = unique_temp_dir("implicit_defaults");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    write_file(&root.join("src/root.cx"), "fn lib_root() {}\n");
    write_file(&root.join("src/main.cx"), "fn main() {}\n");

    let loaded = load_project_from_dir(&root).expect("load project");
    let manifest = loaded.manifest;

    let library = manifest.library.expect("library target");
    assert_eq!(library.name, "app");
    assert_eq!(library.root_file, root.join("src/root.cx"));
    assert_eq!(manifest.binaries.len(), 1);
    assert_eq!(manifest.binaries[0].name, "app");
    assert_eq!(manifest.binaries[0].root_file, root.join("src/main.cx"));
}

#[test]
fn load_project_with_explicit_lib_and_multiple_bins() {
    let root = unique_temp_dir("explicit_targets");
    write_file(
        &root.join("corex.toml"),
        r#"
[project]
name = "app"

[lib]
name = "corelib"

[[bin]]
name = "app"
path = "src/main.cx"

[[bin]]
name = "tool"
path = "src/bin/tool.cx"
"#,
    );
    write_file(&root.join("src/root.cx"), "fn root_lib() {}\n");
    write_file(&root.join("src/main.cx"), "fn main() {}\n");
    write_file(&root.join("src/bin/tool.cx"), "fn main() {}\n");

    let manifest = ProjectLoader::load_project_manifest(&root)
        .expect("load project manifest");

    let library = manifest.library.expect("library target");
    assert_eq!(library.name, "corelib");
    assert_eq!(library.root_file, root.join("src/root.cx"));
    assert_eq!(manifest.binaries.len(), 2);
    assert_eq!(manifest.binaries[0].name, "app");
    assert_eq!(manifest.binaries[0].root_file, root.join("src/main.cx"));
    assert_eq!(manifest.binaries[1].name, "tool");
    assert_eq!(manifest.binaries[1].root_file, root.join("src/bin/tool.cx"));
}

#[test]
fn load_project_without_root_or_main_has_no_default_targets() {
    let root = unique_temp_dir("no_defaults");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");

    let manifest = ProjectLoader::load_project_manifest(&root)
        .expect("load project manifest");

    assert!(manifest.library.is_none());
    assert!(manifest.binaries.is_empty());
}

#[test]
fn load_workspace_manifest_with_explicit_members() {
    let root = unique_temp_dir("workspace_members");
    write_file(
        &root.join("corex.toml"),
        r#"
[workspace]
name = "example_workspace"
members = ["projects/app", "projects/util"]
"#,
    );

    let manifest = ProjectLoader::load_workspace_manifest(&root)
        .expect("load workspace manifest");
    assert_eq!(manifest.name, "example_workspace");
    assert_eq!(
        manifest.members,
        vec![
            PathBuf::from("projects/app"),
            PathBuf::from("projects/util")
        ]
    );
}

#[test]
fn load_project_rejects_workspace_manifest() {
    let root = unique_temp_dir("project_rejects_workspace");
    write_file(
        &root.join("corex.toml"),
        "[workspace]\nname = \"workspace\"\n",
    );

    let error = ProjectLoader::load_project(&root).expect_err("should fail");
    assert!(matches!(
        error,
        ProjectLoadError::UnsupportedManifestShape { .. }
    ));
}

#[test]
fn load_workspace_rejects_project_manifest() {
    let root = unique_temp_dir("workspace_rejects_project");
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");

    let error =
        ProjectLoader::load_workspace_manifest(&root).expect_err("should fail");
    assert!(matches!(
        error,
        ProjectLoadError::UnsupportedManifestShape { .. }
    ));
}

#[test]
fn load_manifest_rejects_ambiguous_workspace_and_project_roles() {
    let root = unique_temp_dir("ambiguous_roles");
    write_file(
        &root.join("corex.toml"),
        r#"
[workspace]
name = "workspace"

[project]
name = "app"
"#,
    );

    let error = ProjectLoader::load_project(&root).expect_err("should fail");
    assert!(matches!(
        error,
        ProjectLoadError::AmbiguousManifestRole { .. }
    ));
}

#[test]
fn load_project_reports_missing_explicit_target_root() {
    let root = unique_temp_dir("missing_target_root");
    write_file(
        &root.join("corex.toml"),
        r#"
[project]
name = "app"

[[bin]]
name = "tool"
path = "src/bin/tool.cx"
"#,
    );

    let error =
        ProjectLoader::load_project_manifest(&root).expect_err("should fail");
    assert!(matches!(
        error,
        ProjectLoadError::MissingTargetRootFile {
            target_name,
            expected_path,
            ..
        } if target_name == "tool"
            && expected_path == root.join("src/bin/tool.cx")
    ));
}

#[test]
fn load_project_reports_duplicate_binary_target_names() {
    let root = unique_temp_dir("duplicate_bin_names");
    write_file(
        &root.join("corex.toml"),
        r#"
[project]
name = "app"

[[bin]]
name = "tool"
path = "src/bin/tool.cx"

[[bin]]
name = "tool"
path = "src/bin/tool2.cx"
"#,
    );
    write_file(&root.join("src/bin/tool.cx"), "fn main() {}\n");
    write_file(&root.join("src/bin/tool2.cx"), "fn main() {}\n");

    let error =
        ProjectLoader::load_project_manifest(&root).expect_err("should fail");
    assert!(matches!(
        error,
        ProjectLoadError::DuplicateBinaryTargetName { name, .. } if name == "tool"
    ));
}

#[test]
fn load_project_name_match_does_not_suppress_implicit_main_bin() {
    let root = unique_temp_dir("name_match_not_suppression");
    write_file(
        &root.join("corex.toml"),
        r#"
[project]
name = "app"

[[bin]]
name = "app"
path = "src/bin/app.cx"
"#,
    );
    write_file(&root.join("src/main.cx"), "fn main() {}\n");
    write_file(&root.join("src/bin/app.cx"), "fn main() {}\n");

    let error =
        ProjectLoader::load_project_manifest(&root).expect_err("should fail");
    assert!(matches!(
        error,
        ProjectLoadError::DuplicateBinaryTargetName { name, .. } if name == "app"
    ));
}

#[test]
fn load_project_reports_duplicate_target_root_paths() {
    let root = unique_temp_dir("duplicate_target_root");
    write_file(
        &root.join("corex.toml"),
        r#"
[project]
name = "app"

[lib]
name = "corelib"

[[bin]]
name = "tool"
path = "src/root.cx"
"#,
    );
    write_file(&root.join("src/root.cx"), "fn root_lib() {}\n");

    let error =
        ProjectLoader::load_project_manifest(&root).expect_err("should fail");
    assert!(matches!(
        error,
        ProjectLoadError::DuplicateTargetRoot { path, .. }
            if path == root.join("src/root.cx")
    ));
}

#[test]
fn load_project_parses_path_and_git_dependencies() {
    let root = unique_temp_dir("dependencies");
    write_file(
        &root.join("corex.toml"),
        r#"
[project]
name = "app"

[dependencies]
util = { path = "../util" }
http = { git = "https://github.com/example/http.git" }
"#,
    );

    let manifest = ProjectLoader::load_project_manifest(&root)
        .expect("load project manifest");

    assert_eq!(manifest.dependencies.len(), 2);
    assert_eq!(manifest.dependencies[0].name, "http");
    assert_eq!(manifest.dependencies[1].name, "util");

    assert!(matches!(
        manifest.dependencies[0].kind,
        DependencyKind::Git { ref git }
            if git == "https://github.com/example/http.git"
    ));
    assert!(matches!(
        manifest.dependencies[1].kind,
        DependencyKind::Path { ref path } if path == Path::new("../util")
    ));
}

#[test]
fn project_cli_loading_uses_manifest_targets_not_recursive_guessing() {
    let root = unique_temp_dir("cli_manifest_targets");
    write_file(
        &root.join("corex.toml"),
        r#"
[project]
name = "app"

[[bin]]
name = "tool"
path = "src/bin/tool.cx"
"#,
    );
    write_file(&root.join("src/bin/tool.cx"), "fn main() {}\n");
    write_file(&root.join("src/ignored.cx"), "fn ignored() {}\n");

    let output = run_cxc(&[
        "dump".to_string(),
        "scopes".to_string(),
        "--project".to_string(),
        arg(&root),
    ]);

    assert!(
        output.status.success(),
        "expected manifest-driven load to work"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== target: binary (src/bin/tool.cx) =="));
    assert!(!stdout.contains("src/main.cx"));
}
