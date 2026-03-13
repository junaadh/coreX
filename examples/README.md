# CoreX Examples

This directory contains small `.cx` programs used as:

- syntax highlighting demos
- parser smoke tests
- documentation snippets
- future compiler integration test inputs

These files should open with syntax support in both Zed and Helix when the CoreX Tree-sitter setup is installed.
They are also covered by parser smoke tests in `tests/examples_parse.rs`.

## Files

- `hello_world.cx` - minimal `main`, string literal, and function call
- `structs.cx` - struct declaration, fields, struct literal init, field access
- `enums.cx` - enum declaration, payload variant, match-based destructuring
- `patterns.cx` - tuple/struct/variant/wildcard patterns and destructuring
- `attributes.cx` - attributes on functions, struct fields, and enum variants
- `ffi.cx` - `extern` block with pointer types (`*void`, `*mut void`)
- `doc_comments.cx` - outer/block doc comments vs normal comments
- `control_flow.cx` - `if`, `while`, ternary `?:`, `??`, and optional chaining `?.`

## Manifest Projects

Manifest-driven examples for the project loader are in `examples/projects/`:

- `bin_app` - binary-only project (`src/main.cx`)
- `lib_core` - library-only project (`src/root.cx`)
- `lib_and_bin` - implicit library + binary defaults
- `multi_bin_lib` - explicit one-library + two-binary manifest

## Workspace Examples

Workspace manifest examples are in `examples/workspaces/`:

- `basic_workspace` - two-member workspace (app + util)
- `full_stack_workspace` - mixed runtime/cli/tooling member targets
- `tools_workspace` - tools-focused workspace with explicit member list
