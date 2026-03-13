use core_x::foreign::BindgenOutput;

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
