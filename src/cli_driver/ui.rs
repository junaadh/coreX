use clap::builder::styling::{AnsiColor, Effects, Styles};
use std::io::IsTerminal;

pub fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::BrightBlue.on_default() | Effects::BOLD)
        .usage(AnsiColor::BrightBlue.on_default() | Effects::BOLD)
        .literal(AnsiColor::BrightCyan.on_default())
        .placeholder(AnsiColor::BrightGreen.on_default())
}

fn force_color_enabled() -> bool {
    std::env::var("CLICOLOR_FORCE")
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(false)
}

fn no_color_requested() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

pub fn ui_stdout_color_enabled() -> bool {
    if force_color_enabled() {
        return true;
    }
    if no_color_requested() {
        return false;
    }
    std::io::stdout().is_terminal()
}

fn ui_stderr_color_enabled() -> bool {
    if force_color_enabled() {
        return true;
    }
    if no_color_requested() {
        return false;
    }
    std::io::stderr().is_terminal()
}

pub fn ui_header(text: &str) -> String {
    if ui_stdout_color_enabled() {
        format!("\x1b[1;34m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn ui_section(text: &str) -> String {
    if ui_stdout_color_enabled() {
        format!("\x1b[1;36m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn ui_error(text: &str) -> String {
    if ui_stderr_color_enabled() {
        format!("\x1b[1;31m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}
