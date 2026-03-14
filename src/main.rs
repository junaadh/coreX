mod cli_driver;
mod lsp;
use clap::Parser;
use cli_driver::{Cli, Commands};
use std::process::ExitCode;

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
        Commands::Lsp => lsp::run_lsp()
            .map_err(|error| -> cli_driver::DynError { error.into() })?,
    }
    Ok(())
}
