use core_x::ffi::Value;
use core_x::foreign::{
    BindgenOptions, ForeignLibrary, ForeignLibraryManifest, TargetOs,
    generate_foreign_bindings, lower_foreign_library_decl,
    lower_parsed_foreign_file_with_manifest, parse_foreign_file,
};
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_os = TargetOs::current()
        .ok_or("unsupported host OS for bindgen demo target mapping")?;

    let header = repo_root.join("tests/fixtures/bindgen/example.h");
    let fixture_script =
        repo_root.join("tests/fixtures/bindgen/build_fixture.sh");
    build_fixture_library(&repo_root, &fixture_script)?;

    let library_path = fixture_library_path(&repo_root, target_os);
    if !library_path.exists() {
        return Err(format!(
            "fixture library not found at {}",
            library_path.display()
        )
        .into());
    }

    let out_dir = repo_root.join("target/demo-bindgen-main");
    let bindgen_output = generate_foreign_bindings(&BindgenOptions {
        header: header.clone(),
        library_name: "example_bindgen".to_string(),
        target_os,
        library_path: library_path.clone(),
        out_dir: out_dir.clone(),
        clang_args: Vec::new(),
    })?;

    println!("header: {}", header.display());
    println!("library: {}", library_path.display());
    println!("generated cx: {}", bindgen_output.source_path.display());
    println!(
        "generated manifest: {}",
        bindgen_output.manifest_path.display()
    );

    let source = fs::read_to_string(&bindgen_output.source_path)?;
    let parsed_file = parse_foreign_file(&source)?;
    let manifest_text = fs::read_to_string(&bindgen_output.manifest_path)?;
    let manifest = ForeignLibraryManifest::from_toml_str(&manifest_text)?;
    let normalized = lower_parsed_foreign_file_with_manifest(
        &parsed_file,
        &manifest,
        target_os,
    )?;

    let c_string =
        CString::new("hello").expect("literal contains no interior NUL");

    for library_decl in normalized {
        let runtime = lower_foreign_library_decl(&library_decl)?;
        run_each_demo_function(&runtime, &c_string)?;
    }

    Ok(())
}

fn build_fixture_library(
    repo_root: &Path,
    script_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if !script_path.exists() {
        return Err(format!(
            "fixture build script not found at {}",
            script_path.display()
        )
        .into());
    }

    let status = Command::new("bash")
        .arg(script_path)
        .current_dir(repo_root)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err("fixture build script failed".into())
    }
}

fn fixture_library_path(repo_root: &Path, target_os: TargetOs) -> PathBuf {
    let file_name = match target_os {
        TargetOs::Macos => "libexample_bindgen.dylib",
        TargetOs::Linux => "libexample_bindgen.so",
        TargetOs::Windows => "example_bindgen.dll",
    };
    repo_root.join("target/test-bindgen").join(file_name)
}

fn run_each_demo_function(
    library: &ForeignLibrary,
    c_string: &CString,
) -> Result<(), Box<dyn std::error::Error>> {
    for (name, args) in [
        ("add_i32", vec![Value::I32(2), Value::I32(3)]),
        ("returns_42", vec![]),
        ("strlen_like", vec![Value::from_c_string(c_string)]),
    ] {
        let function = library
            .function(name)
            .ok_or_else(|| format!("missing generated function `{name}`"))?;
        let result = function.call(&args)?;
        match (name, result) {
            ("add_i32", Value::I32(sum)) => {
                println!("add_i32(2, 3) = {sum}");
            }
            ("returns_42", Value::I32(value)) => {
                println!("returns_42() = {value}");
            }
            ("strlen_like", Value::USize(len)) => {
                println!("strlen_like(\"hello\") = {len}");
            }
            (fn_name, other) => {
                eprintln!("unexpected result from {fn_name}: {other:?}");
            }
        }
    }

    Ok(())
}
