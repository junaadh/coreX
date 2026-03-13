# CoreX Zed Extension

This extension adds basic CoreX editor support in Zed for `.cx` files using the
project's existing Tree-sitter grammar.

## What this provides

- `.cx` file association
- Tree-sitter parsing (`source.corex`)
- syntax highlighting
- code folding
- indentation queries
- locals query support (where available from the grammar queries)
- textobject queries
- minimal doc-comment injections

## Current status

- syntax highlighting: yes
- folding: yes
- indents: yes
- locals: yes
- textobjects: yes
- injections: minimal (doc comments)
- semantic tooling: not yet

## Grammar wiring

This extension is wired to the local Tree-sitter grammar repository in this
project:

- grammar repo: `file:///Users/junaadh/Developer/rust/core_x/tree-sitter`
- grammar revision: `63c4661e2efdbcdc99f88eaa2522dac336114af3`
- grammar id: `corex`
- language scope: `source.corex`

Query files are placed under `languages/corex/` for Zed consumption:

- `highlights.scm`
- `folds.scm`
- `indents.scm`
- `locals.scm`
- `textobjects.scm`
- `injections.scm`

## Local development install (Zed)

1. Open Zed Command Palette.
2. Run `zed: install dev extension`.
3. Select this extension directory:
   `/Users/junaadh/Developer/rust/core_x/editors/zed/corex`
4. Open a `.cx` file and verify highlighting/folding.

## Notes

- This extension is syntax/editor focused only.
- Semantic features like completion, diagnostics, go-to-definition, and refactors
  require LSP support later.
