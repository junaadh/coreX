//! Macro expansion validation tests.
//!
//! These tests verify that supported macro forms work end-to-end
//! and unsupported forms fail with clear, actionable diagnostics.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestProject {
    main_file: PathBuf,
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "corex_macro_validation_{name}_{}_{}",
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

fn create_test_project(name: &str, source: &str) -> TestProject {
    let root = unique_temp_dir(name);
    write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
    let main_file = root.join("src/main.cx");
    write_file(&main_file, source);

    TestProject { main_file }
}

fn run_cxc_check(project: &TestProject) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cxc"))
        .args(["check", &project.main_file.to_string_lossy()])
        .output()
        .expect("run cxc check command")
}

fn assert_check_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "cxc check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_check_failure_with_message(output: &std::process::Output, expected_msg: &str) {
    assert!(
        !output.status.success(),
        "expected cxc check to fail, but it succeeded:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected_msg),
        "expected error message to contain '{}', but got:\n{}",
        expected_msg,
        stderr
    );
}

#[test]
fn test_rule_call_style_expr_supported() {
    // Verify that call-style expression macros work
    let project = create_test_project(
        "rule_call_style_expr",
        r#"
macro identity {
  rule(x: Expr) => { x };
}

fn main() {
  let y = @identity(42);
}
"#,
    );

    let output = run_cxc_check(&project);
    assert_check_success(&output);
}

#[test]
fn test_rule_block_style_tokens_supported() {
    // Verify that block-style token macros work
    let project = create_test_project(
        "rule_block_style_tokens",
        r#"
macro print_tokens {
  rule(tokens: Tokens) => {
    fn main() {
      print(tokens);
    }
  };
}

@print_tokens {
  hello world
}
"#,
    );

    let output = run_cxc_check(&project);
    assert_check_success(&output);
}

#[test]
fn test_reflect_attached_item_supported() {
    // Verify that attached item macros work
    let project = create_test_project(
        "reflect_attached_item",
        r#"
macro derive_debug {
  reflect(item: Item) => {
    item
    fn debug(self) -> str {
      "debug"
    }
  };
}

@derive_debug
struct Point {
  x: i32,
  y: i32,
}

fn main() {
  let p = Point { x: 1, y: 2 };
}
"#,
    );

    let output = run_cxc_check(&project);
    assert_check_success(&output);
}

#[test]
fn test_attached_rule_macro_rejected_with_clear_error() {
    // Verify that attached rule macros fail with clear error
    let project = create_test_project(
        "attached_rule_macro_error",
        r#"
macro bad {
  rule(item: Item) => { item };
}

@bad
struct Foo {}
"#,
    );

    let output = run_cxc_check(&project);
    assert_check_failure_with_message(&output, "attached rule macros are not supported");
    assert_check_failure_with_message(&output, "change this macro clause from `rule(...)` to `reflect(item: Item)`");
}

#[test]
fn test_expression_reflect_macro_rejected_with_clear_error() {
    // Verify that expression reflect macros fail with clear error
    let project = create_test_project(
        "expression_reflect_macro_error",
        r#"
macro bad {
  reflect(expr: Expr) => { expr };
}

fn main() {
  let x = @bad(42);
}
"#,
    );

    let output = run_cxc_check(&project);
    assert_check_failure_with_message(&output, "reflect clauses only support Item input");
    assert_check_failure_with_message(&output, "for expression macros, use a rule clause");
}

#[test]
fn test_unsupported_input_kind_rejected() {
    // Verify that unsupported input kinds (Stmt, Type, Pattern) fail
    let project = create_test_project(
        "unsupported_input_kind",
        r#"
macro bad_stmt {
  rule(s: Stmt) => { s };
}

fn main() {
  @bad_stmt { let x = 42; }
}
"#,
    );

    let output = run_cxc_check(&project);
    assert_check_failure_with_message(&output, "macro `bad_stmt` clause has unsupported input kind");
    assert_check_failure_with_message(&output, "input kind Stmt is");
}

#[test]
fn test_dispatch_validates_compatibility() {
    // Verify that dispatch validates clause/input compatibility
    let project = create_test_project(
        "dispatch_validation",
        r#"
macro wrong_combo {
  reflect(x: Expr) => { x };
}

fn main() {
  let y = @wrong_combo(42);
}
"#,
    );

    let output = run_cxc_check(&project);
    assert_check_failure_with_message(&output, "reflect clauses only support Item input");
}
