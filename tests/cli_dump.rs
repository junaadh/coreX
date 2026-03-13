use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestProject {
    root: PathBuf,
    root_file: PathBuf,
    net_file: PathBuf,
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "corex_cli_dump_{name}_{}_{}",
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

fn create_project_fixture(name: &str) -> TestProject {
    let root = unique_temp_dir(name);
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    let root_file = root.join("src/root.cx");
    let net_file = root.join("src/net.cx");
    let app_file = root.join("src/app.cx");
    let main_file = root.join("src/main.cx");

    write_file(
        &root_file,
        "scope net;\nscope app;\nfn top() {}\nstruct RootType {}\n",
    );
    write_file(
        &net_file,
        "scope http;\nstruct Client {}\nstruct Server {}\n",
    );
    write_file(&root.join("src/net/http.cx"), "fn serve() {}\n");
    write_file(
        &app_file,
        "use root::net::Client;\nuse root::net::{self, http};\n",
    );
    write_file(&main_file, "fn main() {}\n");

    TestProject {
        root,
        root_file,
        net_file,
    }
}

fn create_project_fixture_with_parse_error(name: &str) -> TestProject {
    let project = create_project_fixture(name);
    write_file(
        &project.root.join("src/root.cx"),
        "scope net;\nscope app;\nscope broken;\nfn top() {}\nstruct RootType {}\n",
    );
    write_file(&project.root.join("src/broken.cx"), "fn bad( { return; }\n");
    project
}

fn create_project_fixture_with_semantic_error(name: &str) -> TestProject {
    let project = create_project_fixture(name);
    write_file(
        &project.root.join("src/root.cx"),
        "scope net;\nscope app;\nfn top() {}\nstruct RootType {}\nfn broken() -> i32 { true }\n",
    );
    project
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
fn dump_tokens_single_file() {
    let project = create_project_fixture("tokens_single_file");
    let output = run_cxc(&[
        "dump".to_string(),
        "tokens".to_string(),
        arg(&project.root_file),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== file:"));
    assert!(stdout.contains("KwScope"));
}

#[test]
fn dump_ast_single_file() {
    let project = create_project_fixture("ast_single_file");
    let output = run_cxc(&[
        "dump".to_string(),
        "ast".to_string(),
        arg(&project.root_file),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== file:"));
    assert!(stdout.contains("item_count:"));
    assert!(stdout.contains("Scope"));
}

#[test]
fn dump_parsed_single_file() {
    let project = create_project_fixture("parsed_single_file");
    let output = run_cxc(&[
        "dump".to_string(),
        "parsed".to_string(),
        arg(&project.root_file),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("file_id:"));
    assert!(stdout.contains("item_count:"));
    assert!(stdout.contains("diagnostics_count:"));
}

#[test]
fn dump_scopes_on_src_root() {
    let project = create_project_fixture("scopes_root");
    let output = run_cxc(&[
        "dump".to_string(),
        "scopes".to_string(),
        arg(&project.root_file),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== target: library (src/root.cx) =="));
    assert!(stdout.contains("scope_path: ["));
    assert!(stdout.contains("\"net\""));
}

#[test]
fn dump_imports_on_src_root() {
    let project = create_project_fixture("imports_root");
    let output = run_cxc(&[
        "dump".to_string(),
        "imports".to_string(),
        arg(&project.root_file),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== target: library (src/root.cx) =="));
    assert!(stdout.contains("resolved_imports:"));
    assert!(stdout.contains("local_name: \"Client\""));
}

#[test]
fn dump_scopes_rejects_non_root_single_file() {
    let project = create_project_fixture("scopes_reject_non_root");
    let output = run_cxc(&[
        "dump".to_string(),
        "scopes".to_string(),
        arg(&project.net_file),
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "single-file mode only supports src/root.cx or src/main.cx"
        )
    );
}

#[test]
fn dump_project_wide_ast() {
    let project = create_project_fixture("project_wide_ast");
    let output = run_cxc(&[
        "dump".to_string(),
        "ast".to_string(),
        "--project".to_string(),
        arg(&project.root),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== file: src/root.cx =="));
    assert!(stdout.contains("== file: src/net.cx =="));
    assert!(stdout.contains("== file: src/main.cx =="));
}

#[test]
fn dump_project_wide_scopes() {
    let project = create_project_fixture("project_wide_scopes");
    let output = run_cxc(&[
        "dump".to_string(),
        "scopes".to_string(),
        "--project".to_string(),
        arg(&project.root),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== target: library (src/root.cx) =="));
    assert!(stdout.contains("== target: binary (src/main.cx) =="));
}

#[test]
fn dump_project_wide_imports() {
    let project = create_project_fixture("project_wide_imports");
    let output = run_cxc(&[
        "dump".to_string(),
        "imports".to_string(),
        "--project".to_string(),
        arg(&project.root),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("== target: library (src/root.cx) =="));
    assert!(stdout.contains("scope_symbols:"));
    assert!(stdout.contains("resolved_imports:"));
}

#[test]
fn text_output_is_deterministic() {
    let project = create_project_fixture("deterministic_text");
    let args = vec![
        "dump".to_string(),
        "parsed".to_string(),
        "--project".to_string(),
        arg(&project.root),
    ];

    let first = run_cxc(&args);
    let second = run_cxc(&args);
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn json_output_is_valid_and_contains_expected_top_level_fields() {
    let project = create_project_fixture("json_output");
    let output = run_cxc(&[
        "dump".to_string(),
        "parsed".to_string(),
        "--project".to_string(),
        arg(&project.root),
        "--format".to_string(),
        "json".to_string(),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(&stdout).expect("valid json");

    assert_eq!(value.get("kind").and_then(Value::as_str), Some("parsed"));
    assert_eq!(value.get("mode").and_then(Value::as_str), Some("project"));
    let files = value.get("files").and_then(Value::as_array);
    assert!(files.is_some(), "expected files array in json output");
    let first = &files.expect("files present")[0];
    assert!(first.get("parsed").is_some(), "expected parsed object");
    assert!(first["parsed"].get("ast").is_some(), "expected parsed.ast");
    assert!(
        first["parsed"]
            .get("diagnostics")
            .and_then(Value::as_array)
            .is_some(),
        "expected parsed.diagnostics array"
    );
    assert!(
        first.get("parsed_debug").is_none(),
        "parsed JSON should not be an escaped debug string"
    );
}

#[test]
fn dump_ast_json_single_file_has_structured_items() {
    let project = create_project_fixture("ast_json_single_file");
    let output = run_cxc(&[
        "dump".to_string(),
        "ast".to_string(),
        arg(&project.root_file),
        "--format".to_string(),
        "json".to_string(),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(&stdout).expect("valid json");

    assert_eq!(value.get("kind").and_then(Value::as_str), Some("ast"));
    assert_eq!(value.get("mode").and_then(Value::as_str), Some("file"));
    let files = value
        .get("files")
        .and_then(Value::as_array)
        .expect("files array");
    assert_eq!(files.len(), 1);
    let first = &files[0];
    assert!(first.get("ast").is_some());
    assert!(first.get("ast_debug").is_none());
}

#[test]
fn dump_ast_json_includes_item_attributes() {
    let temp_dir = unique_temp_dir("ast_json_attributes");
    let file = temp_dir.join("ffi.cx");
    write_file(
        &file,
        "@call(.C) extern libc { fn malloc(size: usize) -> *mut void; }\n",
    );

    let output = run_cxc(&[
        "dump".to_string(),
        "ast".to_string(),
        arg(&file),
        "--format".to_string(),
        "json".to_string(),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(&stdout).expect("valid json");

    let items = value["files"][0]["ast"]["items"]
        .as_array()
        .expect("ast items array");
    let extern_item = items
        .iter()
        .find(|item| item["node"]["ExternBlock"].is_object())
        .expect("extern item present");
    let attrs = extern_item["node"]["ExternBlock"]["node"]["attributes"]
        .as_array()
        .expect("attributes array");
    assert!(!attrs.is_empty());
    assert_eq!(attrs[0]["node"]["name"].as_str(), Some("call"));
    assert_eq!(
        attrs[0]["node"]["args"]["Paren"]["raw"].as_str(),
        Some(".C")
    );
}

#[test]
fn dump_single_file_tokens_renders_recovery_diagnostics() {
    let temp_dir = unique_temp_dir("tokens_single_file_diagnostics");
    let file = temp_dir.join("broken.cx");
    write_file(&file, "fn bad( { return; }\n");

    let output =
        run_cxc(&["dump".to_string(), "tokens".to_string(), arg(&file)]);

    assert!(
        output.status.success(),
        "expected recovery-mode dump to succeed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"));
    assert!(stderr.contains("-->"));
}

#[test]
fn dump_project_kinds_render_recovery_diagnostics_and_continue() {
    let project =
        create_project_fixture_with_parse_error("project_diagnostics");

    for kind in ["tokens", "ast", "parsed", "scopes", "imports", "semantic"] {
        let output = run_cxc(&[
            "dump".to_string(),
            kind.to_string(),
            "--project".to_string(),
            arg(&project.root),
        ]);

        assert!(
            output.status.success(),
            "expected dump kind `{kind}` to succeed with recovery diagnostics"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("error:"),
            "expected rendered diagnostics on stderr for `{kind}`"
        );
        assert!(
            stderr.contains("-->"),
            "expected rendered location output on stderr for `{kind}`"
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.is_empty(),
            "expected dump output to continue for `{kind}`"
        );
    }
}

#[test]
fn dump_semantic_project_renders_semantic_diagnostics() {
    let project =
        create_project_fixture_with_semantic_error("project_semantic_error");
    let output = run_cxc(&[
        "dump".to_string(),
        "semantic".to_string(),
        "--project".to_string(),
        arg(&project.root),
    ]);

    assert!(
        output.status.success(),
        "expected dump semantic to continue after semantic diagnostics"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("type mismatch")
            || stderr.contains("invalid return type"),
        "expected semantic diagnostics from the analysis driver"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "expected semantic dump output");
}

#[test]
fn dump_imports_project_does_not_render_semantic_diagnostics() {
    let project =
        create_project_fixture_with_semantic_error("imports_no_semantic_diag");
    let output = run_cxc(&[
        "dump".to_string(),
        "imports".to_string(),
        "--project".to_string(),
        arg(&project.root),
    ]);

    assert!(
        output.status.success(),
        "expected dump imports to succeed without semantic diagnostics"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("type mismatch")
            && !stderr.contains("invalid return type"),
        "dump imports should stop at import diagnostics"
    );
}

#[test]
fn dump_parsed_project_does_not_render_semantic_diagnostics() {
    let project =
        create_project_fixture_with_semantic_error("parsed_no_semantic_diag");
    let output = run_cxc(&[
        "dump".to_string(),
        "parsed".to_string(),
        "--project".to_string(),
        arg(&project.root),
    ]);

    assert!(output.status.success(), "expected dump parsed to succeed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("type mismatch")
            && !stderr.contains("invalid return type"),
        "dump parsed should stop at parse diagnostics"
    );
}
