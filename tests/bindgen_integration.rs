mod helpers;

use core_x::ffi::Value;
use core_x::foreign::{
    BindgenOptions, ForeignLibraryManifest, generate_foreign_bindings,
    lower_foreign_library_decl, lower_parsed_foreign_file_with_manifest,
    parse_foreign_file,
};
use helpers::bindgen_fixture::{
    build_bindgen_fixture, fixture_dylib_name, fixture_target_os,
};
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn unique_out_dir(base: &Path, name: &str) -> PathBuf {
    let dir = base.join(format!(
        "{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("create unique out dir");
    dir
}

#[test]
fn fixture_build_script_produces_dylib() {
    let fixture = build_bindgen_fixture();
    assert!(fixture.header.exists());
    assert!(fixture.dylib.exists());
}

#[test]
fn bindgen_generates_cx_and_manifest_for_fixture() {
    let fixture = build_bindgen_fixture();
    let out_dir = unique_out_dir(&fixture.out_dir, "generate");

    let result = generate_foreign_bindings(&BindgenOptions {
        header: fixture.header.clone(),
        library_name: "example_bindgen".to_string(),
        target_os: fixture_target_os(),
        library_path: fixture.dylib.clone(),
        out_dir: out_dir.clone(),
        clang_args: Vec::new(),
    })
    .expect("generate bindings");

    assert!(result.source_path.exists());
    assert!(result.manifest_path.exists());

    let source = fs::read_to_string(result.source_path).expect("read source");
    assert!(source.contains("fn add_i32"));
    assert!(source.contains("fn returns_42"));
    assert!(source.contains("fn strlen_like"));
}

#[test]
fn generated_outputs_parse_and_lower() {
    let fixture = build_bindgen_fixture();
    let out_dir = unique_out_dir(&fixture.out_dir, "parse-lower");

    let result = generate_foreign_bindings(&BindgenOptions {
        header: fixture.header.clone(),
        library_name: "example_bindgen".to_string(),
        target_os: fixture_target_os(),
        library_path: fixture.dylib.clone(),
        out_dir,
        clang_args: Vec::new(),
    })
    .expect("generate bindings");

    let source = fs::read_to_string(&result.source_path).expect("read source");
    let parsed = parse_foreign_file(&source).expect("parse generated source");

    let manifest_raw =
        fs::read_to_string(&result.manifest_path).expect("read manifest");
    let manifest = ForeignLibraryManifest::from_toml_str(&manifest_raw)
        .expect("parse manifest");

    let lowered = lower_parsed_foreign_file_with_manifest(
        &parsed,
        &manifest,
        fixture_target_os(),
    )
    .expect("lower generated parsed source");

    assert_eq!(lowered.len(), 1);
    assert_eq!(lowered[0].library_name(), "example_bindgen");
}

#[cfg(target_os = "macos")]
#[test]
fn generated_outputs_runtime_call_integration() {
    let fixture = build_bindgen_fixture();
    let out_dir = unique_out_dir(&fixture.out_dir, "runtime");

    let result = generate_foreign_bindings(&BindgenOptions {
        header: fixture.header.clone(),
        library_name: "example_bindgen".to_string(),
        target_os: fixture_target_os(),
        library_path: fixture.dylib.clone(),
        out_dir,
        clang_args: Vec::new(),
    })
    .expect("generate bindings");

    let source = fs::read_to_string(&result.source_path).expect("read source");
    let parsed = parse_foreign_file(&source).expect("parse generated source");
    let manifest = ForeignLibraryManifest::from_toml_str(
        &fs::read_to_string(&result.manifest_path).expect("read manifest"),
    )
    .expect("parse manifest");
    let lowered = lower_parsed_foreign_file_with_manifest(
        &parsed,
        &manifest,
        fixture_target_os(),
    )
    .expect("lower generated source");
    let runtime =
        lower_foreign_library_decl(&lowered[0]).expect("lower to runtime");

    let add = runtime.function("add_i32").expect("lookup add_i32");
    let returns_42 = runtime.function("returns_42").expect("lookup returns_42");
    let strlen_like =
        runtime.function("strlen_like").expect("lookup strlen_like");

    let add_result = add
        .call(&[Value::I32(2), Value::I32(3)])
        .expect("call add_i32");
    let returns_result = returns_42.call(&[]).expect("call returns_42");
    let input = CString::new("hello").expect("cstring");
    let strlen_result = strlen_like
        .call(&[Value::from_c_string(&input)])
        .expect("call strlen_like");

    match add_result {
        Value::I32(v) => assert_eq!(v, 5),
        other => panic!("expected Value::I32, got {other:?}"),
    }
    match returns_result {
        Value::I32(v) => assert_eq!(v, 42),
        other => panic!("expected Value::I32, got {other:?}"),
    }
    match strlen_result {
        Value::USize(v) => assert_eq!(v, 5),
        other => panic!("expected Value::USize, got {other:?}"),
    }
}

#[test]
fn cli_invocation_generates_expected_outputs() {
    let fixture = build_bindgen_fixture();
    let out_dir = unique_out_dir(&fixture.out_dir, "cli");

    let target = match fixture_target_os() {
        core_x::foreign::TargetOs::Macos => "macos",
        core_x::foreign::TargetOs::Linux => "linux",
        core_x::foreign::TargetOs::Windows => "windows",
    };

    let status = Command::new(env!("CARGO_BIN_EXE_corex-bindgen"))
        .arg("--header")
        .arg(&fixture.header)
        .arg("--library-name")
        .arg("example_bindgen")
        .arg("--target-os")
        .arg(target)
        .arg("--library-path")
        .arg(&fixture.dylib)
        .arg("--out-dir")
        .arg(&out_dir)
        .status()
        .expect("spawn cli");

    assert!(status.success(), "cli exited with non-zero status");

    let source_path = out_dir.join("example_bindgen.cx");
    let manifest_path = out_dir.join("corex.foreign.toml");
    assert!(source_path.exists());
    assert!(manifest_path.exists());

    let source =
        fs::read_to_string(&source_path).expect("read generated source");
    let parsed = parse_foreign_file(&source).expect("parse generated source");
    let manifest = ForeignLibraryManifest::from_toml_str(
        &fs::read_to_string(&manifest_path).expect("read generated manifest"),
    )
    .expect("parse generated manifest");
    let lowered = lower_parsed_foreign_file_with_manifest(
        &parsed,
        &manifest,
        fixture_target_os(),
    )
    .expect("lower parsed generated source");
    assert_eq!(lowered.len(), 1);
    assert_eq!(
        fixture_dylib_name(&fixture.dylib),
        lowered[0]
            .library_path()
            .file_name()
            .and_then(|v| v.to_str())
            .expect("utf8 filename")
    );
}

#[test]
fn cli_invocation_infers_library_name_from_library_path_stem() {
    let fixture = build_bindgen_fixture();
    let out_dir = unique_out_dir(&fixture.out_dir, "cli-infer-name");

    let target = match fixture_target_os() {
        core_x::foreign::TargetOs::Macos => "macos",
        core_x::foreign::TargetOs::Linux => "linux",
        core_x::foreign::TargetOs::Windows => "windows",
    };

    let status = Command::new(env!("CARGO_BIN_EXE_corex-bindgen"))
        .arg("--header")
        .arg(&fixture.header)
        .arg("--target-os")
        .arg(target)
        .arg("--library-path")
        .arg(&fixture.dylib)
        .arg("--out-dir")
        .arg(&out_dir)
        .status()
        .expect("spawn cli");

    assert!(status.success(), "cli exited with non-zero status");

    let inferred_name = fixture
        .dylib
        .file_stem()
        .and_then(|v| v.to_str())
        .expect("fixture dylib stem should be utf8")
        .to_string();
    let source_path = out_dir.join(format!("{inferred_name}.cx"));
    let manifest_path = out_dir.join("corex.foreign.toml");
    assert!(source_path.exists());
    assert!(manifest_path.exists());

    let source =
        fs::read_to_string(&source_path).expect("read generated source");
    let parsed = parse_foreign_file(&source).expect("parse generated source");
    assert_eq!(parsed.libraries().len(), 1);
    assert_eq!(parsed.libraries()[0].library_name(), inferred_name);
}
