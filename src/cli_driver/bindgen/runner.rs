use crate::cli_driver::DynError;
use crate::cli_driver::bindgen::args::BindgenCliArgs;
use crate::cli_driver::bindgen::render::bindgen_success_message;
use crate::cli_driver::ui::ui_stdout_color_enabled;
use core_x::foreign::{BindgenOutput, generate_foreign_bindings};

/// Runs bindgen using shared CLI arguments.
pub fn run_bindgen_from_args(
    args: BindgenCliArgs,
) -> Result<BindgenOutput, Box<dyn std::error::Error>> {
    let options = args.into_bindgen_options()?;
    let output = generate_foreign_bindings(&options)?;
    Ok(output)
}

pub fn run_bindgen(args: BindgenCliArgs) -> Result<(), DynError> {
    let output = run_bindgen_from_args(args)?;
    println!(
        "{}",
        bindgen_success_message(&output, ui_stdout_color_enabled())
    );
    Ok(())
}
