# CoreX

CoreX compiler repository.

## Overview

This README documents CLI usage for `cxc`.

## Repository Layout

- `src/main.rs` - `cxc` CLI entrypoint
- `examples/` - sample `.cx` files for CLI dumps
- `docs/REFERENCE.md` - language reference
- `tests/` - CLI and compiler integration tests

## CLI

Build and run:

```bash
cargo run --bin cxc -- --help
```

Top-level commands:

- `cxc dump <kind> <path>`
- `cxc dump <kind> --project <dir>`
- `cxc bindgen ...`
- `cxc lsp`

## `cxc dump`

Supported dump kinds:

- `tokens`
- `ast`
- `parsed`
- `expanded`
- `desugared`
- `hir`
- `resolved`
- `typed` / `inferred`
- `pipeline`
- `scopes`
- `imports`
- `semantic`

Options:

- `--format text` (default)
- `--format json`
- `--stages <comma-separated-stages>` (for combined canonical stage dumps)

Single-file examples:

```bash
cargo run --bin cxc -- dump tokens examples/ffi.cx
cargo run --bin cxc -- dump ast examples/ffi.cx --format json
cargo run --bin cxc -- dump parsed examples/ffi.cx
cargo run --bin cxc -- dump expanded examples/ffi.cx
```

Project examples:

```bash
cargo run --bin cxc -- dump ast --project .
cargo run --bin cxc -- dump hir --project .
cargo run --bin cxc -- dump typed --project .
cargo run --bin cxc -- dump --stages expanded,desugared,hir --project .
cargo run --bin cxc -- dump pipeline --project .
cargo run --bin cxc -- dump scopes --project .
cargo run --bin cxc -- dump imports --project .
cargo run --bin cxc -- dump semantic --project .
```

Notes:

- `text` output is human-readable and deterministic.
- `json` output is machine-readable and stable for tooling/tests.
- Diagnostics are rendered on stderr when present.

## `cxc bindgen`

Generate CoreX foreign declarations and manifest entries from a C header:

```bash
cargo run --bin cxc -- bindgen \
  --header path/to/header.h \
  --target-os macos \
  --library-path /path/to/libexample.dylib \
  --out-dir ./generated
```

## `cxc lsp`

Start the CoreX language server over stdio:

```bash
cargo run --bin cxc -- lsp
```

Current v0 LSP support:

- lifecycle: `initialize`, `initialized`, `shutdown`, `exit`
- text sync: `textDocument/didOpen`, `textDocument/didChange`, `textDocument/didClose`
- diagnostics: `textDocument/publishDiagnostics`
- language features:
  - `textDocument/documentSymbol`
  - `textDocument/hover`
  - `textDocument/definition`
  - `textDocument/completion`
  - `textDocument/inlayHint`

Analysis behavior:

- files inside a CoreX project are analyzed in project context
- standalone files use fallback single-file analysis

## Development

Useful local checks:

```bash
cargo fmt
cargo check
cargo test
```
