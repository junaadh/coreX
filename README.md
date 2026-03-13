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

## `cxc dump`

Supported dump kinds:

- `tokens`
- `ast`
- `parsed`
- `scopes`
- `imports`

Options:

- `--format text` (default)
- `--format json`

Single-file examples:

```bash
cargo run --bin cxc -- dump tokens examples/ffi.cx
cargo run --bin cxc -- dump ast examples/ffi.cx --format json
cargo run --bin cxc -- dump parsed examples/ffi.cx
```

Project examples:

```bash
cargo run --bin cxc -- dump ast --project .
cargo run --bin cxc -- dump scopes --project .
cargo run --bin cxc -- dump imports --project .
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

## Development

Useful local checks:

```bash
cargo fmt
cargo check
cargo test
```
