use core_x::frontend::parser::parse_source_file;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn example_cx_files() -> Vec<PathBuf> {
    let dir = examples_dir();
    let entries = fs::read_dir(&dir).unwrap_or_else(|err| {
        panic!("failed to read examples dir {}: {err}", dir.display())
    });

    let mut files = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("cx"))
        .collect::<Vec<_>>();

    files.sort();
    files
}

#[test]
fn all_examples_parse() {
    let files = example_cx_files();
    assert!(
        !files.is_empty(),
        "no .cx files found in examples directory {}",
        examples_dir().display()
    );

    for path in files {
        let source = fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("failed to read example {}: {err}", path.display())
        });

        let parsed = parse_source_file(&source).unwrap_or_else(|err| {
            panic!("failed to parse example {}: {err}", path.display())
        });
        assert!(
            parsed.diagnostics.is_empty(),
            "strict parse returned diagnostics for {}",
            path.display()
        );
    }
}

#[test]
fn examples_directory_contains_expected_files() {
    let present = example_cx_files()
        .into_iter()
        .filter_map(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(std::borrow::ToOwned::to_owned)
        })
        .collect::<BTreeSet<_>>();

    let expected = [
        "hello_world.cx",
        "structs.cx",
        "enums.cx",
        "patterns.cx",
        "attributes.cx",
        "ffi.cx",
        "doc_comments.cx",
        "control_flow.cx",
    ];

    for name in expected {
        assert!(
            present.contains(name),
            "missing expected example file {} in {}",
            name,
            examples_dir().display()
        );
    }
}
