use clap::{Parser, ValueEnum};
use core_x::foreign::{BindgenOptions, TargetOs, generate_foreign_bindings};
use std::path::PathBuf;

/// Generate `coreX` foreign bindings (`.cx` + `corex.foreign.toml`) from a C header.
#[derive(Debug, Parser)]
#[command(name = "corex-bindgen")]
#[command(
    about = "Generate coreX foreign source and manifest from a C header",
    long_about = "Extract supported C function declarations from a header using Clang, emit <library>.cx, and create/update corex.foreign.toml."
)]
struct Cli {
    /// Path to the C header file to process.
    #[arg(long)]
    header: PathBuf,

    /// Symbolic foreign library name used for generated extern block and manifest key.
    #[arg(long)]
    library_name: String,

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
    let cli = Cli::parse();
    let options = BindgenOptions {
        header: cli.header,
        library_name: cli.library_name,
        target_os: cli.target_os.into(),
        library_path: cli.library_path,
        out_dir: cli.out_dir,
        clang_args: cli.clang_arg,
    };

    let output = generate_foreign_bindings(&options)?;
    println!(
        "generated: {}\nmanifest: {}",
        output.source_path.display(),
        output.manifest_path.display()
    );
    Ok(())
}
