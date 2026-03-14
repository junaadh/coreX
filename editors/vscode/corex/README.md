# CoreX VS Code Extension

CoreX support for VS Code, aligned with the existing Zed setup:

- `.cx` language registration
- editor comment/bracket behavior from Zed language config
- Tree-sitter powered semantic highlighting via CoreX `highlights.scm`
- LSP integration using `cxc lsp`

## Features

### Tree-sitter highlighting

This extension loads:

- parser wasm: `syntaxes/tree-sitter-corex.wasm`
- query: `syntaxes/highlights.scm`

and maps Tree-sitter captures to VS Code semantic token types.

### LSP

The extension starts the CoreX language server over stdio.

Default launch command:

```text
cxc lsp
```

Configurable settings:

- `corex.languageServer.command` (default: `cxc`)
- `corex.languageServer.args` (default: `["lsp"]`)
- `corex.languageServer.cwd` (default: `workspace`)
- `corex.treeSitter.enabled` (default: `true`)

## Development

Install dependencies:

```bash
cd editors/vscode/corex
bun install
```

Quick syntax check:

```bash
bun run check
```

Launch extension dev host:

1. Open `editors/vscode/corex` in VS Code.
2. Press `F5` to run the extension in an Extension Development Host.
3. Open a `.cx` file.

## Notes

- This extension uses semantic tokens from Tree-sitter captures.
- Rich language features are provided by the CoreX LSP server.
