use super::decl::{ForeignLibraryDecl, LoweringError};
use super::parse::{
    ParsedForeignFile, ParsedForeignLibraryDecl,
    lower_parsed_foreign_library_decl,
};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

/// Target operating system used for foreign library path resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    Macos,
    Linux,
    Windows,
}

impl Display for TargetOs {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Macos => write!(f, "macos"),
            Self::Linux => write!(f, "linux"),
            Self::Windows => write!(f, "windows"),
        }
    }
}

impl TargetOs {
    /// Returns the current host OS as a [`TargetOs`] when supported.
    #[must_use]
    pub fn current() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::Macos)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else {
            None
        }
    }
}

/// Platform-specific runtime library paths for a symbolic foreign library.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryPaths {
    pub macos: Option<PathBuf>,
    pub linux: Option<PathBuf>,
    pub windows: Option<PathBuf>,
}

impl LibraryPaths {
    #[must_use]
    pub fn for_target(&self, target: TargetOs) -> Option<&Path> {
        match target {
            TargetOs::Macos => self.macos.as_deref(),
            TargetOs::Linux => self.linux.as_deref(),
            TargetOs::Windows => self.windows.as_deref(),
        }
    }
}

/// Manifest mapping symbolic foreign library names to platform-specific paths.
///
/// This type is independent of source syntax and provides name-to-path
/// resolution needed by parsed-source lowering.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForeignLibraryManifest {
    libraries: BTreeMap<String, LibraryPaths>,
}

impl ForeignLibraryManifest {
    /// Parses a foreign-library manifest from TOML text.
    ///
    /// Expected schema:
    /// `[libraries.<name>]` with optional `macos`, `linux`, and `windows`
    /// string path keys.
    ///
    /// # Errors
    /// Returns [`ManifestError`] when TOML parsing fails or schema is invalid.
    pub fn from_toml_str(input: &str) -> Result<Self, ManifestError> {
        let value = input.parse::<toml::Value>().map_err(|err| {
            ManifestError::TomlParse {
                message: err.to_string(),
            }
        })?;
        let root =
            value
                .as_table()
                .ok_or_else(|| ManifestError::InvalidSchema {
                    message: "manifest root must be a TOML table".to_string(),
                })?;

        for key in root.keys() {
            if key != "libraries" {
                return Err(ManifestError::InvalidSchema {
                    message: format!("unknown top-level key: {key}"),
                });
            }
        }

        let mut libraries = BTreeMap::new();
        if let Some(libraries_value) = root.get("libraries") {
            let libraries_table =
                libraries_value.as_table().ok_or_else(|| {
                    ManifestError::InvalidSchema {
                        message: "`libraries` must be a table".to_string(),
                    }
                })?;

            for (library_name, paths_value) in libraries_table {
                if library_name.trim().is_empty() {
                    return Err(ManifestError::InvalidSchema {
                        message: "library name cannot be empty".to_string(),
                    });
                }

                let paths_table = paths_value.as_table().ok_or_else(|| {
                    ManifestError::InvalidSchema {
                        message: format!(
                            "library `{library_name}` entry must be a table"
                        ),
                    }
                })?;

                let mut paths = LibraryPaths::default();
                for (target_key, path_value) in paths_table {
                    let path =
                        parse_path_value(library_name, target_key, path_value)?;
                    match target_key.as_str() {
                        "macos" => paths.macos = Some(path),
                        "linux" => paths.linux = Some(path),
                        "windows" => paths.windows = Some(path),
                        other => {
                            return Err(ManifestError::InvalidSchema {
                                message: format!(
                                    "unknown key `{other}` for library `{library_name}`"
                                ),
                            });
                        }
                    }
                }

                validate_library_name(library_name)?;
                validate_paths(library_name, &paths)?;
                libraries.insert(library_name.clone(), paths);
            }
        }

        Ok(Self { libraries })
    }

    /// Resolves a symbolic library name to a concrete path for `target`.
    ///
    /// # Errors
    /// Returns [`ResolveError::UnknownLibrary`] when the library name is not in
    /// the manifest, or [`ResolveError::MissingPathForTarget`] when the library
    /// exists but has no path for the requested target.
    pub fn resolve(
        &self,
        library_name: &str,
        target: TargetOs,
    ) -> Result<&Path, ResolveError> {
        let paths = self.libraries.get(library_name).ok_or_else(|| {
            ResolveError::UnknownLibrary {
                name: library_name.to_string(),
            }
        })?;

        paths.for_target(target).ok_or_else(|| {
            ResolveError::MissingPathForTarget {
                name: library_name.to_string(),
                target,
            }
        })
    }

    /// Inserts or replaces one symbolic library entry with validated paths.
    ///
    /// This enforces the same invariants as TOML parsing for library names and
    /// path values.
    ///
    /// # Errors
    /// Returns [`ManifestError`] if `library_name` is empty or any configured
    /// path is empty.
    pub fn insert(
        &mut self,
        library_name: impl Into<String>,
        paths: LibraryPaths,
    ) -> Result<(), ManifestError> {
        let library_name = library_name.into();
        validate_library_name(&library_name)?;
        validate_paths(&library_name, &paths)?;
        self.libraries.insert(library_name, paths);
        Ok(())
    }

    #[must_use]
    pub fn libraries(&self) -> &BTreeMap<String, LibraryPaths> {
        &self.libraries
    }
}

fn parse_path_value(
    library_name: &str,
    target_key: &str,
    value: &toml::Value,
) -> Result<PathBuf, ManifestError> {
    let Some(path) = value.as_str() else {
        return Err(ManifestError::InvalidSchema {
            message: format!(
                "path for `{library_name}.{target_key}` must be a string"
            ),
        });
    };

    if path.trim().is_empty() {
        return Err(ManifestError::InvalidSchema {
            message: format!(
                "path for `{library_name}.{target_key}` cannot be empty"
            ),
        });
    }

    Ok(PathBuf::from(path))
}

/// Manifest loading and schema-validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    TomlParse { message: String },
    InvalidSchema { message: String },
}

impl Display for ManifestError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TomlParse { message } => {
                write!(
                    f,
                    "failed to parse foreign library manifest TOML: {message}"
                )
            }
            Self::InvalidSchema { message } => {
                write!(f, "invalid foreign library manifest schema: {message}")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

/// Foreign library name resolution errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    UnknownLibrary { name: String },
    MissingPathForTarget { name: String, target: TargetOs },
}

impl Display for ResolveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLibrary { name } => {
                write!(f, "unknown foreign library `{name}`")
            }
            Self::MissingPathForTarget { name, target } => {
                write!(
                    f,
                    "foreign library `{name}` has no path for target `{target}`"
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Errors produced by manifest-based parsed-source lowering.
#[derive(Debug)]
pub enum ManifestLoweringError {
    Resolve {
        library_name: String,
        source: ResolveError,
    },
    Lowering {
        library_name: String,
        source: LoweringError,
    },
}

impl Display for ManifestLoweringError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolve {
                library_name,
                source,
            } => {
                write!(
                    f,
                    "failed to resolve foreign library `{library_name}`: {source}"
                )
            }
            Self::Lowering {
                library_name,
                source,
            } => write!(
                f,
                "failed to lower parsed foreign declaration for library `{library_name}` with manifest: {source}"
            ),
        }
    }
}

impl std::error::Error for ManifestLoweringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Resolve { source, .. } => Some(source),
            Self::Lowering { source, .. } => Some(source),
        }
    }
}

/// Resolves the parsed source library name through a manifest and lowers into
/// normalized foreign declaration IR.
///
/// This convenience API preserves layering by delegating to the explicit-path
/// parsed-source lowering entry point after manifest resolution.
///
/// # Errors
/// Returns [`ManifestLoweringError::Resolve`] when name resolution fails, or
/// [`ManifestLoweringError::Lowering`] when normalized IR lowering fails.
pub fn lower_parsed_foreign_library_decl_with_manifest(
    parsed: &ParsedForeignLibraryDecl,
    manifest: &ForeignLibraryManifest,
    target: TargetOs,
) -> Result<ForeignLibraryDecl, ManifestLoweringError> {
    let library_name = parsed.library_name().to_string();
    let resolved_path =
        manifest.resolve(&library_name, target).map_err(|source| {
            ManifestLoweringError::Resolve {
                library_name: library_name.clone(),
                source,
            }
        })?;

    lower_parsed_foreign_library_decl(parsed, resolved_path.to_path_buf())
        .map_err(|source| ManifestLoweringError::Lowering {
            library_name,
            source,
        })
}

/// File-level manifest-aware lowering errors.
#[derive(Debug)]
pub enum FileLoweringError {
    LowerLibrary {
        index: usize,
        library_name: String,
        source: Box<ManifestLoweringError>,
    },
}

impl Display for FileLoweringError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LowerLibrary {
                index,
                library_name,
                source,
            } => write!(
                f,
                "failed to lower foreign library block #{index} (`{library_name}`): {source}"
            ),
        }
    }
}

impl std::error::Error for FileLoweringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::LowerLibrary { source, .. } => Some(source.as_ref()),
        }
    }
}

/// Lowers a parsed foreign file using manifest-based library path resolution.
///
/// Each extern block is lowered independently in source order. Blocks with the
/// same library name remain separate and are not merged.
///
/// # Errors
/// Returns [`FileLoweringError`] with block index and library name context when
/// any block fails to lower.
pub fn lower_parsed_foreign_file_with_manifest(
    parsed: &ParsedForeignFile,
    manifest: &ForeignLibraryManifest,
    target: TargetOs,
) -> Result<Vec<ForeignLibraryDecl>, FileLoweringError> {
    parsed
        .libraries()
        .iter()
        .enumerate()
        .map(|(index, library)| {
            lower_parsed_foreign_library_decl_with_manifest(
                library, manifest, target,
            )
            .map_err(|source| FileLoweringError::LowerLibrary {
                index,
                library_name: library.library_name().to_string(),
                source: Box::new(source),
            })
        })
        .collect()
}

fn validate_library_name(name: &str) -> Result<(), ManifestError> {
    if name.trim().is_empty() {
        return Err(ManifestError::InvalidSchema {
            message: "library name cannot be empty".to_string(),
        });
    }
    Ok(())
}

fn validate_paths(
    library_name: &str,
    paths: &LibraryPaths,
) -> Result<(), ManifestError> {
    for (target_key, value) in [
        ("macos", &paths.macos),
        ("linux", &paths.linux),
        ("windows", &paths.windows),
    ] {
        if let Some(path) = value
            && path.as_os_str().is_empty()
        {
            return Err(ManifestError::InvalidSchema {
                message: format!(
                    "path for `{library_name}.{target_key}` cannot be empty"
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::{ForeignCallConv, Value};
    use crate::foreign::{
        lower_foreign_library_decl, parse_foreign_file,
        parse_foreign_library_decl,
    };
    use std::ffi::CString;
    use std::path::{Path, PathBuf};

    #[test]
    fn parse_manifest_with_single_library() {
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let resolved = manifest
            .resolve("libSystem", TargetOs::Macos)
            .expect("resolve macos path");
        assert_eq!(resolved, Path::new("/usr/lib/libSystem.B.dylib"));
    }

    #[test]
    fn parse_manifest_with_multiple_platforms() {
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.sqlite3]
macos = "/usr/lib/libsqlite3.dylib"
linux = "libsqlite3.so"
windows = "sqlite3.dll"
"#,
        )
        .expect("manifest should parse");

        assert_eq!(
            manifest.resolve("sqlite3", TargetOs::Macos),
            Ok(Path::new("/usr/lib/libsqlite3.dylib"))
        );
        assert_eq!(
            manifest.resolve("sqlite3", TargetOs::Linux),
            Ok(Path::new("libsqlite3.so"))
        );
        assert_eq!(
            manifest.resolve("sqlite3", TargetOs::Windows),
            Ok(Path::new("sqlite3.dll"))
        );
    }

    #[test]
    fn resolve_unknown_library_fails() {
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let err = manifest
            .resolve("missing", TargetOs::Macos)
            .expect_err("unknown library should fail");
        assert!(matches!(err, ResolveError::UnknownLibrary { .. }));
    }

    #[test]
    fn resolve_missing_target_path_fails() {
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let err = manifest
            .resolve("libSystem", TargetOs::Linux)
            .expect_err("missing linux path should fail");
        assert!(matches!(err, ResolveError::MissingPathForTarget { .. }));
    }

    #[test]
    fn parse_manifest_rejects_empty_path() {
        let err = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = ""
"#,
        )
        .expect_err("empty path should fail");
        assert!(matches!(err, ManifestError::InvalidSchema { .. }));
    }

    #[test]
    fn parse_then_lower_with_manifest() {
        let parsed = parse_foreign_library_decl(
            r"
extern libSystem {
    fn strlen(s: *const void) -> usize;
    fn pid = getpid() -> i32;
}
",
        )
        .expect("source should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let decl = lower_parsed_foreign_library_decl_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect("manifest lowering should succeed");

        assert_eq!(decl.library_name(), "libSystem");
        assert_eq!(
            decl.library_path(),
            Path::new("/usr/lib/libSystem.B.dylib")
        );
        assert_eq!(decl.functions().len(), 2);
        assert_eq!(decl.functions()[0].local_name(), "strlen");
        assert_eq!(decl.functions()[0].symbol_name(), "strlen");
        assert_eq!(decl.functions()[1].local_name(), "pid");
        assert_eq!(decl.functions()[1].symbol_name(), "getpid");
    }

    #[test]
    fn manifest_lowering_wraps_unknown_library() {
        let parsed = parse_foreign_library_decl(
            r"
extern missingLib {
    fn getpid() -> i32;
}
",
        )
        .expect("source should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let err = lower_parsed_foreign_library_decl_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect_err("unknown library should fail");

        match err {
            ManifestLoweringError::Resolve { library_name, .. } => {
                assert_eq!(library_name, "missingLib");
            }
            other @ ManifestLoweringError::Lowering { .. } => {
                panic!("expected resolve error, got {other:?}");
            }
        }
    }

    #[test]
    fn manifest_lowering_preserves_ir_validation_failures() {
        let parsed = parse_foreign_library_decl(
            r"
extern libSystem {
    fn pid = getpid() -> i32;
    fn pid = getpid() -> i32;
}
",
        )
        .expect("source should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let err = lower_parsed_foreign_library_decl_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect_err("duplicate local name should fail");
        assert!(matches!(
            err,
            ManifestLoweringError::Lowering {
                library_name: _,
                source: LoweringError::DuplicateLocalName { .. }
            }
        ));
    }

    #[test]
    fn insert_rejects_empty_library_name() {
        let mut manifest = ForeignLibraryManifest::default();
        let err = manifest
            .insert("   ", LibraryPaths::default())
            .expect_err("empty name should fail");
        assert!(matches!(err, ManifestError::InvalidSchema { .. }));
    }

    #[test]
    fn insert_rejects_empty_path_value() {
        let mut manifest = ForeignLibraryManifest::default();
        let paths = LibraryPaths {
            macos: Some(PathBuf::new()),
            linux: None,
            windows: None,
        };
        let err = manifest
            .insert("libSystem", paths)
            .expect_err("empty path should fail");
        assert!(matches!(err, ManifestError::InvalidSchema { .. }));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parse_manifest_lower_runtime_call_integration() {
        let parsed = parse_foreign_library_decl(
            r"
extern libSystem {
    fn strlen(s: *const void) -> usize;
    fn pid = getpid() -> i32;
}
",
        )
        .expect("source should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let decl = lower_parsed_foreign_library_decl_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect("manifest lowering should succeed");
        let runtime = lower_foreign_library_decl(&decl)
            .expect("runtime lowering should succeed");

        let strlen = runtime.function("strlen").expect("lookup strlen");
        let pid = runtime.function("pid").expect("lookup pid");

        let input = CString::new("hello").expect("literal contains no NUL");
        let strlen_result = strlen
            .call(&[Value::from_c_string(&input)])
            .expect("call strlen");
        let pid_result = pid.call(&[]).expect("call pid");

        match strlen_result {
            Value::USize(length) => assert_eq!(length, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
        match pid_result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn lower_file_with_manifest_multiple_blocks() {
        let parsed = parse_foreign_file(
            r"
extern libSystem {
    fn strlen(s: *const void) -> usize;
    fn pid = getpid() -> i32;
}

extern sqlite3 {
    fn sqlite3_close(db: *mut void) -> i32;
}
",
        )
        .expect("file should parse");

        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"

[libraries.sqlite3]
macos = "/usr/lib/libsqlite3.dylib"
"#,
        )
        .expect("manifest should parse");

        let lowered = lower_parsed_foreign_file_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect("file lowering should succeed");

        assert_eq!(lowered.len(), 2);
        assert_eq!(lowered[0].library_name(), "libSystem");
        assert_eq!(
            lowered[0].library_path(),
            Path::new("/usr/lib/libSystem.B.dylib")
        );
        assert_eq!(lowered[1].library_name(), "sqlite3");
        assert_eq!(
            lowered[1].library_path(),
            Path::new("/usr/lib/libsqlite3.dylib")
        );
    }

    #[test]
    fn lower_file_wraps_failing_block_index_and_name() {
        let parsed = parse_foreign_file(
            r"
extern libSystem {
    fn getpid() -> i32;
}

extern missingLib {
    fn foo() -> i32;
}
",
        )
        .expect("file should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let err = lower_parsed_foreign_file_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect_err("missing library should fail");

        match err {
            FileLoweringError::LowerLibrary {
                index,
                library_name,
                source: _,
            } => {
                assert_eq!(index, 1);
                assert_eq!(library_name, "missingLib");
            }
        }
    }

    #[test]
    fn lower_file_preserves_source_order() {
        let parsed = parse_foreign_file(
            r"
extern one {
    fn a() -> i32;
}

extern two {
    fn b() -> i32;
}

extern three {
    fn c() -> i32;
}
",
        )
        .expect("file should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.one]
macos = "/tmp/one.dylib"

[libraries.two]
macos = "/tmp/two.dylib"

[libraries.three]
macos = "/tmp/three.dylib"
"#,
        )
        .expect("manifest should parse");

        let lowered = lower_parsed_foreign_file_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect("file lowering should succeed");

        assert_eq!(lowered.len(), 3);
        assert_eq!(lowered[0].library_name(), "one");
        assert_eq!(lowered[1].library_name(), "two");
        assert_eq!(lowered[2].library_name(), "three");
    }

    #[test]
    fn lower_file_allows_duplicate_library_names_as_separate_blocks() {
        let parsed = parse_foreign_file(
            r"
extern libSystem {
    fn getpid() -> i32;
}

extern libSystem {
    fn strlen(s: *const void) -> usize;
}
",
        )
        .expect("file should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let lowered = lower_parsed_foreign_file_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect("file lowering should succeed");

        assert_eq!(lowered.len(), 2);
        assert_eq!(lowered[0].library_name(), "libSystem");
        assert_eq!(lowered[1].library_name(), "libSystem");
    }

    #[test]
    fn parse_file_manifest_lower_preserves_call_conv_per_block() {
        let parsed = parse_foreign_file(
            r"
@call(.C)
extern libSystem {
    fn getpid() -> i32;
}

extern libSystem {
    fn strlen(s: *const void) -> usize;
}
",
        )
        .expect("file should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let lowered = lower_parsed_foreign_file_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect("file lowering should succeed");

        assert_eq!(lowered.len(), 2);
        assert_eq!(lowered[0].functions()[0].call_conv(), ForeignCallConv::C);
        assert_eq!(lowered[1].functions()[0].call_conv(), ForeignCallConv::C);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parse_file_lower_runtime_integration_for_multiple_blocks() {
        let parsed = parse_foreign_file(
            r"
extern libSystem {
    fn strlen(s: *const void) -> usize;
    fn pid = getpid() -> i32;
}

extern libSystem {
    fn puts(_: *const void) -> i32;
}
",
        )
        .expect("file should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let lowered = lower_parsed_foreign_file_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect("file lowering should succeed");
        assert_eq!(lowered.len(), 2);

        let runtime_a = lower_foreign_library_decl(&lowered[0])
            .expect("runtime lowering for first block");
        let runtime_b = lower_foreign_library_decl(&lowered[1])
            .expect("runtime lowering for second block");

        let strlen = runtime_a.function("strlen").expect("lookup strlen");
        let pid = runtime_a.function("pid").expect("lookup pid");
        let puts = runtime_b.function("puts").expect("lookup puts");

        let input = CString::new("hello").expect("literal contains no NUL");
        let strlen_result = strlen
            .call(&[Value::from_c_string(&input)])
            .expect("call strlen");
        let pid_result = pid.call(&[]).expect("call pid");
        let puts_result = puts
            .call(&[Value::from_c_string(&input)])
            .expect("call puts");

        match strlen_result {
            Value::USize(length) => assert_eq!(length, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
        match pid_result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
        match puts_result {
            Value::I32(rc) => assert!(rc >= 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn runtime_integration_still_works_with_explicit_call_conv_metadata() {
        let parsed = parse_foreign_library_decl(
            r"
@call(.C)
extern libSystem {
    fn strlen(s: *const void) -> usize;
    fn pid = getpid() -> i32;
}
",
        )
        .expect("source should parse");
        let manifest = ForeignLibraryManifest::from_toml_str(
            r#"
[libraries.libSystem]
macos = "/usr/lib/libSystem.B.dylib"
"#,
        )
        .expect("manifest should parse");

        let decl = lower_parsed_foreign_library_decl_with_manifest(
            &parsed,
            &manifest,
            TargetOs::Macos,
        )
        .expect("manifest lowering should succeed");

        assert_eq!(decl.functions()[0].call_conv(), ForeignCallConv::C);
        assert_eq!(decl.functions()[1].call_conv(), ForeignCallConv::C);

        let runtime = lower_foreign_library_decl(&decl)
            .expect("runtime lowering should succeed");
        let strlen = runtime.function("strlen").expect("lookup strlen");
        let pid = runtime.function("pid").expect("lookup pid");

        let input = CString::new("hello").expect("literal contains no NUL");
        let strlen_result = strlen
            .call(&[Value::from_c_string(&input)])
            .expect("call strlen");
        let pid_result = pid.call(&[]).expect("call pid");

        match strlen_result {
            Value::USize(length) => assert_eq!(length, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
        match pid_result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }
}
