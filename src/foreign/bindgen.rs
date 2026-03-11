use super::{ForeignLibraryManifest, ManifestError, TargetOs};
use serde_json::Value as JsonValue;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MANIFEST_FILE_NAME: &str = "corex.foreign.toml";

/// Inputs for generating `coreX` foreign bindings from a C header.
#[derive(Debug, Clone)]
pub struct BindgenOptions {
    /// Header path to inspect with Clang.
    pub header: PathBuf,
    /// Symbolic foreign library name used in generated extern block and manifest.
    pub library_name: String,
    /// Target OS key used when writing manifest path entries.
    pub target_os: TargetOs,
    /// Runtime shared-library path to store under the target OS in manifest.
    pub library_path: PathBuf,
    /// Output directory for generated `<library_name>.cx` and manifest.
    pub out_dir: PathBuf,
    /// Extra pass-through arguments for clang.
    pub clang_args: Vec<String>,
}

/// Paths to generated binding artifacts.
#[derive(Debug, Clone)]
pub struct BindgenOutput {
    /// Generated foreign source file path (`<library_name>.cx`).
    pub source_path: PathBuf,
    /// Generated or updated manifest path (`corex.foreign.toml`).
    pub manifest_path: PathBuf,
}

#[derive(Debug)]
struct ExtractedFunction {
    name: String,
    params: Vec<ExtractedParam>,
    ret: SourceEmitType,
}

#[derive(Debug)]
struct ExtractedParam {
    name: Option<String>,
    ty: SourceEmitType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceEmitType {
    Void,
    I32,
    Usize,
    PtrConstVoid,
    PtrMutVoid,
}

/// Errors produced during C-header bindgen generation.
#[derive(Debug)]
pub enum BindgenError {
    HeaderRead {
        path: PathBuf,
        source: std::io::Error,
    },
    ClangInvocation {
        message: String,
    },
    UnsupportedDeclaration {
        message: String,
    },
    UnsupportedType {
        message: String,
    },
    OutputWrite {
        path: PathBuf,
        source: std::io::Error,
    },
    ManifestLoad {
        path: PathBuf,
        source: ManifestError,
    },
}

impl Display for BindgenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HeaderRead { path, source } => {
                write!(f, "failed to read header {}: {source}", path.display())
            }
            Self::ClangInvocation { message } => {
                write!(f, "clang invocation failed: {message}")
            }
            Self::UnsupportedDeclaration { message } => {
                write!(f, "unsupported declaration: {message}")
            }
            Self::UnsupportedType { message } => {
                write!(f, "unsupported C type: {message}")
            }
            Self::OutputWrite { path, source } => {
                write!(
                    f,
                    "failed to write generated output {}: {source}",
                    path.display()
                )
            }
            Self::ManifestLoad { path, source } => {
                write!(
                    f,
                    "failed to load manifest {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for BindgenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::HeaderRead { source, .. } | Self::OutputWrite { source, .. } => {
                Some(source)
            }
            Self::ClangInvocation { .. }
            | Self::UnsupportedDeclaration { .. }
            | Self::UnsupportedType { .. } => None,
            Self::ManifestLoad { source, .. } => Some(source),
        }
    }
}

/// Generates `.cx` foreign source and `corex.foreign.toml` from a C header.
///
/// This function extracts a narrow C declaration subset via Clang, renders
/// `coreX` foreign source, and creates/updates the foreign library manifest.
/// It does not verify native ABI truth beyond the extracted declarations.
///
/// # Errors
/// Returns [`BindgenError`] when header reading, Clang extraction, supported
/// type mapping, or output writing fails.
pub fn generate_foreign_bindings(
    options: &BindgenOptions,
) -> Result<BindgenOutput, BindgenError> {
    let _header_content =
        fs::read_to_string(&options.header).map_err(|source| {
            BindgenError::HeaderRead {
                path: options.header.clone(),
                source,
            }
        })?;

    let extracted =
        extract_functions_with_clang(&options.header, &options.clang_args)?;
    let source = render_foreign_source(&options.library_name, &extracted);

    fs::create_dir_all(&options.out_dir).map_err(|source| {
        BindgenError::OutputWrite {
            path: options.out_dir.clone(),
            source,
        }
    })?;

    let source_path =
        options.out_dir.join(format!("{}.cx", options.library_name));
    fs::write(&source_path, source).map_err(|source| {
        BindgenError::OutputWrite {
            path: source_path.clone(),
            source,
        }
    })?;

    let manifest_path = options.out_dir.join(MANIFEST_FILE_NAME);
    update_foreign_manifest(
        &manifest_path,
        &options.library_name,
        options.target_os,
        &options.library_path,
    )?;

    Ok(BindgenOutput {
        source_path,
        manifest_path,
    })
}

fn extract_functions_with_clang(
    header: &Path,
    clang_args: &[String],
) -> Result<Vec<ExtractedFunction>, BindgenError> {
    let output = Command::new("clang")
        .arg("-Xclang")
        .arg("-ast-dump=json")
        .arg("-fsyntax-only")
        .args(clang_args)
        .arg(header)
        .output()
        .map_err(|err| BindgenError::ClangInvocation {
            message: err.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BindgenError::ClangInvocation {
            message: stderr.to_string(),
        });
    }

    let ast: JsonValue =
        serde_json::from_slice(&output.stdout).map_err(|err| {
            BindgenError::ClangInvocation {
                message: format!("failed to parse clang AST JSON: {err}"),
            }
        })?;

    let header_canon =
        header
            .canonicalize()
            .map_err(|source| BindgenError::HeaderRead {
                path: header.to_path_buf(),
                source,
            })?;
    let mut functions = Vec::new();
    let Some(inner) = ast.get("inner").and_then(JsonValue::as_array) else {
        return Err(BindgenError::ClangInvocation {
            message: "clang AST JSON missing root `inner` list".to_string(),
        });
    };

    for child in inner {
        if !is_decl_in_header(child, &header_canon) {
            continue;
        }
        let kind = child
            .get("kind")
            .and_then(JsonValue::as_str)
            .unwrap_or("<unknown>");
        match kind {
            "FunctionDecl" => functions.push(parse_function_decl(child)?),
            other => {
                return Err(BindgenError::UnsupportedDeclaration {
                    message: format!(
                        "unsupported top-level declaration kind `{other}` in {}",
                        header.display()
                    ),
                });
            }
        }
    }

    if functions.is_empty() {
        return Err(BindgenError::UnsupportedDeclaration {
            message: format!(
                "no supported top-level function declarations found in {}",
                header.display()
            ),
        });
    }
    Ok(functions)
}

fn is_decl_in_header(node: &JsonValue, header_canon: &Path) -> bool {
    if let Some(file) = node
        .get("loc")
        .and_then(|v| v.get("file"))
        .and_then(JsonValue::as_str)
    {
        if let Ok(canon) = Path::new(file).canonicalize() {
            return canon == header_canon;
        }
        return false;
    }

    node.get("loc")
        .and_then(|v| v.get("line"))
        .and_then(JsonValue::as_u64)
        .is_some()
}

fn parse_function_decl(
    node: &JsonValue,
) -> Result<ExtractedFunction, BindgenError> {
    let name = node
        .get("name")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| BindgenError::UnsupportedDeclaration {
            message: "function declaration missing name".to_string(),
        })?
        .to_string();

    let qual_type = node
        .get("type")
        .and_then(|v| v.get("qualType"))
        .and_then(JsonValue::as_str)
        .ok_or_else(|| BindgenError::UnsupportedDeclaration {
            message: format!("function `{name}` missing qualType"),
        })?;

    let ret_type = parse_function_return_type(qual_type).ok_or_else(|| {
        BindgenError::UnsupportedDeclaration {
            message: format!(
                "unsupported function type format for `{name}`: {qual_type}"
            ),
        }
    })?;
    let ret = map_c_type_to_source(ret_type).map_err(|message| {
        BindgenError::UnsupportedType {
            message: format!("function `{name}` return {message}"),
        }
    })?;

    let mut params = Vec::new();
    if let Some(inner) = node.get("inner").and_then(JsonValue::as_array) {
        for child in inner {
            if child.get("kind").and_then(JsonValue::as_str)
                != Some("ParmVarDecl")
            {
                continue;
            }

            let param_ty = child
                .get("type")
                .and_then(|v| v.get("qualType"))
                .and_then(JsonValue::as_str)
                .ok_or_else(|| BindgenError::UnsupportedDeclaration {
                    message: format!("parameter in `{name}` missing qualType"),
                })?;
            let ty = map_c_type_to_source(param_ty).map_err(|message| {
                BindgenError::UnsupportedType {
                    message: format!("function `{name}` parameter {message}"),
                }
            })?;

            let pname = child
                .get("name")
                .and_then(JsonValue::as_str)
                .map(ToOwned::to_owned)
                .filter(|v| !v.trim().is_empty());
            params.push(ExtractedParam { name: pname, ty });
        }
    }

    if params.len() == 1 && params[0].ty == SourceEmitType::Void {
        params.clear();
    } else if params.iter().any(|p| p.ty == SourceEmitType::Void) {
        return Err(BindgenError::UnsupportedType {
            message: format!(
                "function `{name}` has `void` in non-empty parameter list"
            ),
        });
    }

    Ok(ExtractedFunction { name, params, ret })
}

fn parse_function_return_type(qual_type: &str) -> Option<&str> {
    qual_type.find(" (").map(|idx| qual_type[..idx].trim())
}

fn map_c_type_to_source(c_type: &str) -> Result<SourceEmitType, String> {
    let normalized = normalize_c_type(c_type);
    match normalized.as_str() {
        "void" => Ok(SourceEmitType::Void),
        "int" => Ok(SourceEmitType::I32),
        "size_t" | "unsigned long" | "unsigned long int" => {
            Ok(SourceEmitType::Usize)
        }
        "void *" | "char *" => Ok(SourceEmitType::PtrMutVoid),
        "const void *" | "const char *" => Ok(SourceEmitType::PtrConstVoid),
        other => Err(format!("`{other}` is not in the supported subset")),
    }
}

fn normalize_c_type(c_type: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for ch in c_type.trim().chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else if ch == '*' {
            if out.ends_with(' ') {
                out.pop();
            }
            out.push(' ');
            out.push('*');
            prev_space = false;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn render_type(ty: SourceEmitType) -> &'static str {
    match ty {
        SourceEmitType::Void => "void",
        SourceEmitType::I32 => "i32",
        SourceEmitType::Usize => "usize",
        SourceEmitType::PtrConstVoid => "*const void",
        SourceEmitType::PtrMutVoid => "*mut void",
    }
}

fn render_foreign_source(
    library_name: &str,
    functions: &[ExtractedFunction],
) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    out.push_str("@call(.C)\n");
    let _ = writeln!(&mut out, "extern {library_name} {{");
    for function in functions {
        out.push_str("    fn ");
        out.push_str(&function.name);
        out.push('(');
        let mut first = true;
        for param in &function.params {
            if !first {
                out.push_str(", ");
            }
            first = false;
            if let Some(name) = &param.name {
                out.push_str(name);
                out.push_str(": ");
                out.push_str(render_type(param.ty));
            } else {
                out.push_str("_: ");
                out.push_str(render_type(param.ty));
            }
        }
        out.push_str(") -> ");
        out.push_str(render_type(function.ret));
        out.push_str(";\n");
    }
    out.push_str("}\n");
    out
}

fn update_foreign_manifest(
    manifest_path: &Path,
    library_name: &str,
    target_os: TargetOs,
    library_path: &Path,
) -> Result<(), BindgenError> {
    let mut manifest = if manifest_path.exists() {
        let raw = fs::read_to_string(manifest_path).map_err(|source| {
            BindgenError::HeaderRead {
                path: manifest_path.to_path_buf(),
                source,
            }
        })?;
        ForeignLibraryManifest::from_toml_str(&raw).map_err(|source| {
            BindgenError::ManifestLoad {
                path: manifest_path.to_path_buf(),
                source,
            }
        })?
    } else {
        ForeignLibraryManifest::default()
    };

    let mut paths = manifest
        .libraries()
        .get(library_name)
        .cloned()
        .unwrap_or_default();
    match target_os {
        TargetOs::Macos => paths.macos = Some(library_path.to_path_buf()),
        TargetOs::Linux => paths.linux = Some(library_path.to_path_buf()),
        TargetOs::Windows => paths.windows = Some(library_path.to_path_buf()),
    }
    manifest
        .insert(library_name.to_string(), paths)
        .map_err(|source| BindgenError::ManifestLoad {
            path: manifest_path.to_path_buf(),
            source,
        })?;

    let rendered = render_manifest_toml(&manifest);
    fs::write(manifest_path, rendered).map_err(|source| {
        BindgenError::OutputWrite {
            path: manifest_path.to_path_buf(),
            source,
        }
    })
}

fn render_manifest_toml(manifest: &ForeignLibraryManifest) -> String {
    let mut out = String::new();
    let mut first = true;
    for (name, paths) in manifest.libraries() {
        if !first {
            out.push('\n');
        }
        first = false;

        out.push_str("[libraries.");
        out.push_str(&toml::Value::String(name.clone()).to_string());
        out.push_str("]\n");

        if let Some(path) = &paths.macos {
            out.push_str("macos = ");
            out.push_str(
                &toml::Value::String(path.to_string_lossy().to_string())
                    .to_string(),
            );
            out.push('\n');
        }
        if let Some(path) = &paths.linux {
            out.push_str("linux = ");
            out.push_str(
                &toml::Value::String(path.to_string_lossy().to_string())
                    .to_string(),
            );
            out.push('\n');
        }
        if let Some(path) = &paths.windows {
            out.push_str("windows = ");
            out.push_str(
                &toml::Value::String(path.to_string_lossy().to_string())
                    .to_string(),
            );
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "corex-bindgen-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock before unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn map_supported_c_types_to_source_types() {
        assert_eq!(map_c_type_to_source("void"), Ok(SourceEmitType::Void));
        assert_eq!(map_c_type_to_source("int"), Ok(SourceEmitType::I32));
        assert_eq!(map_c_type_to_source("size_t"), Ok(SourceEmitType::Usize));
        assert_eq!(
            map_c_type_to_source("void *"),
            Ok(SourceEmitType::PtrMutVoid)
        );
        assert_eq!(
            map_c_type_to_source("const void *"),
            Ok(SourceEmitType::PtrConstVoid)
        );
        assert_eq!(
            map_c_type_to_source("char *"),
            Ok(SourceEmitType::PtrMutVoid)
        );
        assert_eq!(
            map_c_type_to_source("const char *"),
            Ok(SourceEmitType::PtrConstVoid)
        );
    }

    #[test]
    fn render_generated_cx_source_is_stable() {
        let functions = vec![
            ExtractedFunction {
                name: "add_i32".to_string(),
                params: vec![
                    ExtractedParam {
                        name: Some("a".to_string()),
                        ty: SourceEmitType::I32,
                    },
                    ExtractedParam {
                        name: Some("b".to_string()),
                        ty: SourceEmitType::I32,
                    },
                ],
                ret: SourceEmitType::I32,
            },
            ExtractedFunction {
                name: "returns_42".to_string(),
                params: vec![],
                ret: SourceEmitType::I32,
            },
        ];

        let rendered = render_foreign_source("example_bindgen", &functions);
        let expected = "@call(.C)\nextern example_bindgen {\n    fn add_i32(a: i32, b: i32) -> i32;\n    fn returns_42() -> i32;\n}\n";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn generate_manifest_creates_new_file() {
        let out = unique_temp_dir("manifest-create");
        let manifest_path = out.join("corex.foreign.toml");

        update_foreign_manifest(
            &manifest_path,
            "example_bindgen",
            TargetOs::Macos,
            Path::new("/tmp/libexample_bindgen.dylib"),
        )
        .expect("update manifest");

        let content =
            fs::read_to_string(&manifest_path).expect("read manifest file");
        assert!(content.contains("libraries"));
        assert!(content.contains("example_bindgen"));
        assert!(content.contains("/tmp/libexample_bindgen.dylib"));
    }

    #[test]
    fn generate_manifest_updates_existing_file_without_wiping_other_entries() {
        let out = unique_temp_dir("manifest-update");
        let manifest_path = out.join("corex.foreign.toml");
        fs::write(
            &manifest_path,
            r#"[libraries.other]
macos = "/tmp/libother.dylib"
"#,
        )
        .expect("write initial manifest");

        update_foreign_manifest(
            &manifest_path,
            "example_bindgen",
            TargetOs::Macos,
            Path::new("/tmp/libexample_bindgen.dylib"),
        )
        .expect("update manifest");

        let content =
            fs::read_to_string(&manifest_path).expect("read manifest file");
        assert!(content.contains("libraries"));
        assert!(content.contains("other"));
        assert!(content.contains("/tmp/libother.dylib"));
        assert!(content.contains("example_bindgen"));
        assert!(content.contains("/tmp/libexample_bindgen.dylib"));
    }

    #[test]
    fn loc_without_file_is_treated_as_header_declaration() {
        let out = unique_temp_dir("loc-fallback");
        let header = out.join("example.h");
        fs::write(&header, "int f(void);").expect("write header");
        let header_canon = header.canonicalize().expect("canonicalize header");

        let node = json!({
            "kind": "FunctionDecl",
            "loc": { "line": 2, "col": 1 }
        });
        assert!(is_decl_in_header(&node, &header_canon));
    }
}
