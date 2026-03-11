use std::path::{Path, PathBuf};
use std::process::Command;

pub struct BindgenFixturePaths {
    pub header: PathBuf,
    pub dylib: PathBuf,
    pub out_dir: PathBuf,
}

pub fn build_bindgen_fixture() -> BindgenFixturePaths {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = repo_root.join("tests/fixtures/bindgen");
    let script = fixture_dir.join("build_fixture.sh");
    let header = fixture_dir.join("example.h");

    let output = Command::new("bash")
        .arg(&script)
        .output()
        .expect("failed to invoke fixture build script");
    assert!(
        output.status.success(),
        "fixture script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let dylib_path = String::from_utf8(output.stdout)
        .expect("script stdout is not utf8")
        .trim()
        .to_string();
    let dylib = PathBuf::from(dylib_path);

    assert!(
        header.exists(),
        "header fixture missing: {}",
        header.display()
    );
    assert!(dylib.exists(), "fixture dylib missing: {}", dylib.display());

    let out_dir = repo_root.join("target/test-bindgen/generated");
    std::fs::create_dir_all(&out_dir).expect("create generated output dir");

    BindgenFixturePaths {
        header,
        dylib,
        out_dir,
    }
}

pub fn fixture_target_os() -> core_x::foreign::TargetOs {
    if cfg!(target_os = "macos") {
        core_x::foreign::TargetOs::Macos
    } else if cfg!(target_os = "linux") {
        core_x::foreign::TargetOs::Linux
    } else {
        panic!("fixture helper only supports macOS/Linux hosts");
    }
}

pub fn fixture_dylib_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|v| v.to_str())
        .expect("fixture dylib path must end with utf8 file name")
}
