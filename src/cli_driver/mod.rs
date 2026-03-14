use clap::{ColorChoice, Parser, Subcommand};

pub mod bindgen;
pub mod diagnostics;
pub mod dump;
pub mod project;
pub mod ui;

pub type DynError = Box<dyn std::error::Error>;

#[derive(Parser)]
#[command(name = "cxc")]
#[command(color = ColorChoice::Auto)]
#[command(styles = ui::cli_styles())]
#[command(
    about = "coreX compiler and build driver",
    long_about = "cxc is the coreX compiler entrypoint and build driver. It combines compile-facing operations with deterministic frontend introspection across lexing, parsing, scope resolution, import resolution, and foreign-interface generation."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Generate coreX foreign-interface bindings from C headers.
    #[command(
        about = "Generate `.cx` foreign declarations and `corex.foreign.toml` from C headers",
        long_about = "Runs the Clang-backed bindgen pipeline, emits source-level foreign declarations, and materializes target-path mappings in manifest form for runtime loading."
    )]
    Bindgen(bindgen::BindgenCliArgs),
    /// Dump deterministic frontend pipeline snapshots.
    #[command(
        about = "Dump frontend pipeline artifacts (lexer/parser/resolver)",
        long_about = "Produces deterministic frontend snapshots for a single file or project. This command hooks directly into concrete pipeline boundaries: lexer output (`tokens`), parser output (`ast`), parsed envelope output (`parsed`), scope-resolution output (`scopes`), import-resolution output (`imports`), and semantic-analysis output (`semantic`).",
        after_long_help = "Dump kinds:\n  tokens    Emit the full token stream from lexer output.\n  ast       Emit recursive AST structure from parser output.\n  parsed    Emit ParsedFile envelope (file id, AST, diagnostics).\n  scopes    Emit resolved project scope graph rooted at src/root.cx or src/main.cx.\n  imports   Emit resolved imports and collected scope symbols over a scope graph.\n  semantic  Emit semantic-analysis summaries and semantic diagnostics."
    )]
    Dump(dump::DumpArgs),
    /// Run the CoreX language server over stdio.
    #[command(
        about = "Run CoreX Language Server Protocol (LSP) server over stdio",
        long_about = "Launches a thin stdio-based Language Server Protocol endpoint backed by the existing CoreX frontend analysis pipeline."
    )]
    Lsp,
}
