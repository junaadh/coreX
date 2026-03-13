mod cli_driver;

use clap::{Args, ColorChoice, Parser, Subcommand, ValueEnum};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "cxc")]
#[command(color = ColorChoice::Auto)]
#[command(styles = cli_driver::ui::cli_styles())]
#[command(
    about = "coreX compiler and build driver",
    long_about = "cxc is the coreX compiler entrypoint and build driver. It combines compile-facing operations with deterministic frontend introspection across lexing, parsing, scope resolution, import resolution, and foreign-interface generation."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate coreX foreign-interface bindings from C headers.
    #[command(
        about = "Generate `.cx` foreign declarations and `corex.foreign.toml` from C headers",
        long_about = "Runs the Clang-backed bindgen pipeline, emits source-level foreign declarations, and materializes target-path mappings in manifest form for runtime loading."
    )]
    Bindgen(cli_driver::bindgen::BindgenCliArgs),
    /// Dump deterministic frontend pipeline snapshots.
    #[command(
        about = "Dump frontend pipeline artifacts (lexer/parser/resolver)",
        long_about = "Produces deterministic frontend snapshots for a single file or project. This command hooks directly into concrete pipeline boundaries: lexer output (`tokens`), parser output (`ast`), parsed envelope output (`parsed`), scope-resolution output (`scopes`), import-resolution output (`imports`), and semantic-analysis output (`semantic`).",
        after_long_help = "Dump kinds:\n  tokens    Emit the full token stream from lexer output.\n  ast       Emit recursive AST structure from parser output.\n  parsed    Emit ParsedFile envelope (file id, AST, diagnostics).\n  scopes    Emit resolved project scope graph rooted at src/root.cx or src/main.cx.\n  imports   Emit resolved imports and collected scope symbols over a scope graph.\n  semantic  Emit semantic-analysis summaries and semantic diagnostics."
    )]
    Dump(DumpArgs),
}

#[derive(Args)]
pub(crate) struct DumpArgs {
    /// Dump kind to emit from the frontend pipeline.
    kind: DumpKind,
    /// Single source file path (mutually exclusive with `--project`).
    path: Option<std::path::PathBuf>,
    /// Project directory root (mutually exclusive with `<path>`).
    #[arg(long)]
    project: Option<std::path::PathBuf>,
    /// Output format for emitted dump payload.
    #[arg(long, value_enum, default_value_t = DumpFormat::Text)]
    format: DumpFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum DumpKind {
    Tokens,
    Ast,
    Scopes,
    Imports,
    Semantic,
    Parsed,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum DumpFormat {
    Text,
    Json,
}

fn main() -> ExitCode {
    match run_cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                cli_driver::ui::ui_error(&format!("error: {error}"))
            );
            ExitCode::FAILURE
        }
    }
}

fn run_cli() -> Result<(), cli_driver::DynError> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Bindgen(args) => cli_driver::bindgen::run_bindgen(args)?,
        Commands::Dump(args) => cli_driver::dump::run_dump(args)?,
    }
    Ok(())
}
