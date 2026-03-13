use clap::{Args, ValueEnum};
use std::io;
use std::path::{Path, PathBuf};

use super::{
    BindgenOptions, BindgenOutput, TargetOs, generate_foreign_bindings,
};

/// CLI target-os value surface for bindgen commands.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BindgenTargetOs {
    Macos,
    Linux,
    Windows,
}

impl From<BindgenTargetOs> for TargetOs {
    fn from(value: BindgenTargetOs) -> Self {
        match value {
            BindgenTargetOs::Macos => Self::Macos,
            BindgenTargetOs::Linux => Self::Linux,
            BindgenTargetOs::Windows => Self::Windows,
        }
    }
}

/// Shared bindgen CLI flags for `cxc bindgen` and compatibility wrappers.
#[derive(Debug, Clone, Args)]
pub struct BindgenCliArgs {
    /// Path to the C header file to process.
    #[arg(long)]
    pub header: PathBuf,

    /// Symbolic foreign library name used for generated extern block and manifest key.
    ///
    /// If omitted, this defaults to the file name stem from `--library-path`.
    #[arg(long)]
    pub library_name: Option<String>,

    /// Target operating system key for manifest path mapping.
    #[arg(long, value_enum)]
    pub target_os: BindgenTargetOs,

    /// Runtime shared-library path to store in generated manifest for target OS.
    #[arg(long)]
    pub library_path: PathBuf,

    /// Output directory for generated `<library_name>.cx` and `corex.foreign.toml`.
    #[arg(long)]
    pub out_dir: PathBuf,

    /// Extra argument passed through to clang. Repeat this flag for multiple args.
    #[arg(long)]
    pub clang_arg: Vec<String>,
}

impl BindgenCliArgs {
    /// Converts CLI flags into bindgen engine options.
    pub fn into_bindgen_options(
        self,
    ) -> Result<BindgenOptions, Box<dyn std::error::Error>> {
        let library_name =
            resolve_library_name(self.library_name, &self.library_path)?;
        Ok(BindgenOptions {
            header: self.header,
            library_name,
            target_os: self.target_os.into(),
            library_path: self.library_path,
            out_dir: self.out_dir,
            clang_args: self.clang_arg,
        })
    }
}

/// Runs bindgen using shared CLI arguments.
pub fn run_bindgen_from_args(
    args: BindgenCliArgs,
) -> Result<BindgenOutput, Box<dyn std::error::Error>> {
    let options = args.into_bindgen_options()?;
    let output = generate_foreign_bindings(&options)?;
    Ok(output)
}

/// Stable success message printed by bindgen CLIs.
#[must_use]
pub fn bindgen_success_message(output: &BindgenOutput, color: bool) -> String {
    let generated_label = if color {
        "\x1b[1;32mgenerated:\x1b[0m"
    } else {
        "generated:"
    };
    let manifest_label = if color {
        "\x1b[1;36mmanifest:\x1b[0m"
    } else {
        "manifest:"
    };
    format!(
        "{generated_label} {}\n{manifest_label} {}",
        output.source_path.display(),
        output.manifest_path.display()
    )
}

fn resolve_library_name(
    explicit: Option<String>,
    library_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(name) = explicit {
        if name.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--library-name cannot be empty",
            )
            .into());
        }
        return Ok(name);
    }

    let stem = library_path
        .file_stem()
        .and_then(|v| v.to_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "cannot infer --library-name from library path {}",
                    library_path.display()
                ),
            )
        })?;
    if stem.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "inferred library name is empty",
        )
        .into());
    }
    Ok(stem.to_string())
}
