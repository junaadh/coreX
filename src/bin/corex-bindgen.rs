use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{ColorChoice, CommandFactory, FromArgMatches, Parser, ValueEnum};
use core_x::foreign::{BindgenOptions, TargetOs, generate_foreign_bindings};
use std::io;
use std::path::{Path, PathBuf};

/// Generate `coreX` foreign bindings (`.cx` + `corex.foreign.toml`) from a C header.
#[derive(Debug, Parser)]
#[command(name = "corex-bindgen")]
#[command(color = ColorChoice::Always)]
#[command(
    about = "Generate coreX foreign source and manifest from a C header",
    long_about = "Extract supported C function declarations from a header using Clang, emit <library>.cx, and create/update corex.foreign.toml."
)]
struct Cli {
    /// Path to the C header file to process.
    #[arg(long)]
    header: PathBuf,

    /// Symbolic foreign library name used for generated extern block and manifest key.
    ///
    /// If omitted, this defaults to the file name stem from `--library-path`.
    #[arg(long)]
    library_name: Option<String>,

    /// Target operating system key for manifest path mapping.
    #[arg(long, value_enum)]
    target_os: CliTargetOs,

    /// Runtime shared-library path to store in generated manifest for target OS.
    #[arg(long)]
    library_path: PathBuf,

    /// Output directory for generated `<library_name>.cx` and `corex.foreign.toml`.
    #[arg(long)]
    out_dir: PathBuf,

    /// Extra argument passed through to clang. Repeat this flag for multiple args.
    #[arg(long)]
    clang_arg: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliTargetOs {
    Macos,
    Linux,
    Windows,
}

impl From<CliTargetOs> for TargetOs {
    fn from(value: CliTargetOs) -> Self {
        match value {
            CliTargetOs::Macos => Self::Macos,
            CliTargetOs::Linux => Self::Linux,
            CliTargetOs::Windows => Self::Windows,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Cli::command().styles(cli_styles()).get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|err| err.exit());
    let library_name =
        resolve_library_name(cli.library_name, &cli.library_path)?;
    let options = BindgenOptions {
        header: cli.header,
        library_name,
        target_os: cli.target_os.into(),
        library_path: cli.library_path,
        out_dir: cli.out_dir,
        clang_args: cli.clang_arg,
    };

    let output = generate_foreign_bindings(&options)?;
    let generated_label = "\x1b[1;32mgenerated:\x1b[0m";
    let manifest_label = "\x1b[1;36mmanifest:\x1b[0m";
    println!(
        "{generated_label} {}\n{manifest_label} {}",
        output.source_path.display(),
        output.manifest_path.display()
    );
    Ok(())
}

fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::BrightBlue.on_default() | Effects::BOLD)
        .usage(AnsiColor::BrightBlue.on_default() | Effects::BOLD)
        .literal(AnsiColor::BrightCyan.on_default())
        .placeholder(AnsiColor::BrightGreen.on_default())
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
