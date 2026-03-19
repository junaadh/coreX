# CoreX Language Reference

This document defines the current CoreX syntax and project model in EBNF form.
It is organized by numbered sections and keeps semantic notes separate from syntax.

## 1. Terminology

CoreX uses the following terms:

| Term | Definition |
|---|---|
| workspace | Top-level container for one or more projects and a shared build environment |
| project | Single buildable/importable unit with its own `corex.toml` and dependency root |
| scope | Source namespace unit defined by files/directories |

Hierarchy:

```text
workspace
  └ project
       └ scopes
```

The term `package` is not used in this specification.

## 2. Project and Workspace Layout

### 2.1 Project root layout

Project root files:

| File | Meaning |
|---|---|
| `corex.toml` | project manifest |
| `build.cx` | project build script (not loaded by current frontend analysis pipeline) |
| `src/root.cx` | root scope of the project |
| `src/main.cx` | optional executable entry |

Example:

```text
src/
  root.cx
  net/
    net.cx
    http.cx
  util.cx
```

### Root scope file

`src/root.cx` defines the root scope of a project.

Nested scopes do not use `root.cx`.

### 2.2 Workspace and project manifests

A workspace `corex.toml` defines workspace-level configuration and explicit
workspace members.

A project `corex.toml` defines one project's metadata, dependencies, and build
configuration.

These are distinct manifests with different roles, even though they use the
same filename.

A manifest must declare exactly one role section:

- `[project]` (project manifest), or
- `[workspace]` (workspace manifest)

Invalid manifest shapes:

- both `[project]` and `[workspace]` present
- neither `[project]` nor `[workspace]` present

Example workspace manifest:

```toml
# workspace corex.toml
[workspace]
name = "example_workspace"
members = ["projects/app", "projects/util"]
```

Example project manifest:

```toml
# project corex.toml
[project]
name = "app"

[lib]
name = "app"

[[bin]]
name = "app"
path = "src/main.cx"

[[bin]]
name = "tool"
path = "src/bin/tool.cx"
```

These examples lock target naming concepts and target roles. They do not define
the complete manifest grammar.

Workspace members are explicit:

```toml
[workspace]
name = "example_workspace"

members = [
  "projects/app",
  "projects/util"
]
```

No automatic filesystem scanning is performed for workspace membership.

### 2.3 Dependencies

Dependencies are explicit per project:

Local dependency:

```toml
[dependencies]
util = { path = "../util" }
```

Git dependency:

```toml
[dependencies]
http = { git = "https://github.com/example/http.git" }
```

Dependency source and graph behavior are specified in Section 2.6.

Rules:

- Workspace members are explicit.
- Dependencies are explicit.
- No automatic filesystem discovery of projects.
- Being a workspace member does not automatically make a project a dependency.
- Being a dependency does not automatically make a project a workspace member.

### 2.4 Target roles

Project name, library target name, and binary target names are distinct.

Library target:

- A project may define at most one library target.
- If `src/root.cx` exists, the project has a library target rooted at
  `src/root.cx`.
- If no explicit `[lib]` target is declared, the library target name defaults
  to the project name.

Binary targets:

- A project may define one or more binary targets.
- If `src/main.cx` exists, the project has a default binary target rooted at
  `src/main.cx`.
- If no explicit `[[bin]]` target uses `path = "src/main.cx"`, the default
  binary target name is the project name.
- Additional `[[bin]]` targets may coexist with the default binary target.

Target validation rules:

- Explicit `[lib]` and `[[bin]]` targets must point to existing root files.
- Duplicate binary target names are rejected.
- Duplicate target root file paths are rejected.
- At most one library target is permitted.
- An implicit library target from `src/root.cx` coexists with explicit bins.
- The implicit default bin from `src/main.cx` is suppressed only when an
  explicit `[[bin]]` already owns `src/main.cx`.

Import roots:

- Dependency import roots come from dependency binding names declared in the
  current project's `[dependencies]` table.
- Library code is imported using the library target name.
- Binary targets do not create dependency import roots unless the language
  defines that explicitly.
- There is no implicit `lib<name>` prefix rule.

### 2.5 Binary and library scope separation

- `main.cx` does not automatically see names from `root.cx`.
- Binary code imports library code explicitly using the library target name.

Example:

```text
use mylib::net::http;
```

In a binary target, `root::` refers to that binary target's own root scope, not
the library target.

### 2.6 Project Dependencies

Projects may depend on other CoreX projects through the `[dependencies]`
section of `corex.toml`.

Dependencies are resolved at the project level and form a deduplicated package
graph.

#### 2.6.1 Dependency declaration

Dependencies are declared in `corex.toml` using a dependency key and a source
specification.

Example:

```toml
[dependencies]
serde = { git = "https://github.com/serde-cx/serde.git" }
util  = { path = "../util" }
core  = { path = "/Users/home/core" }
```

The dependency key (`serde`, `util`, `core`) is the import root used within the
project.

Example:

```text
use serde::json::Value;
use util::math::Vector;
```

The dependency key does not need to match the upstream project name.

#### 2.6.2 Supported dependency sources

CoreX currently supports two dependency source kinds:

| Source | Description |
|---|---|
| `path` | Local filesystem project |
| `git` | Remote Git repository |

Examples:

```toml
[dependencies]
serde = { git = "https://github.com/serde-cx/serde.git" }

serde = { git = "https://github.com/serde-cx/serde.git", rev = "abc123" }

serde = { git = "https://github.com/serde-cx/serde.git", tag = "v1.0.0" }

serde = { git = "https://github.com/serde-cx/serde.git", branch = "main" }
```

Revision selection precedence is:

`rev > tag > branch > repository default branch`

#### 2.6.3 Dependency resolution

Dependency resolution proceeds as follows:

1. The root project's manifest is loaded.
2. Each dependency is resolved to a package instance.
3. Each dependency's `corex.toml` is loaded.
4. The dependency graph is expanded recursively.
5. Package instances are deduplicated using their resolved source identity.

Two dependencies referring to the same source and revision resolve to a single
package instance in the dependency graph.

This prevents exponential dependency expansion.

#### 2.6.4 Dependency import roots

Within source code, dependency names act as import roots.

Example:

```text
use serde::json::Value;
```

Resolution of this path begins at the library target root of the `serde`
dependency.

Dependency keys defined in `[dependencies]` determine the import root visible
to the consuming project.

#### 2.6.5 Global dependency source cache

Git dependencies are fetched into a global source cache.

Typical location:

```text
~/.cache/corex/src/
```

The cache stores:

- bare repository mirrors
- revision checkouts used by projects

Example structure:

```text
~/.cache/corex/src/git/
  db/
    <repo-hash>.git
  checkouts/
    <repo-hash>/
      <revision>/
```

This allows:

- deduplicated downloads
- reuse of dependency source across projects
- stable filesystem paths for tools such as LSP

#### 2.6.6 Dependency source for language tooling

Language tooling such as LSP servers resolves dependency source code directly
from the global source cache.

This allows features such as:

- jump-to-definition
- hover documentation
- symbol navigation

across dependency boundaries.

#### 2.6.7 Build artifacts

Build artifacts are stored within the project repository.

Typical location:

```text
build/
```

This directory contains:

- compiled dependency artifacts used by the project
- intermediate object files
- final binaries and libraries
- build metadata

Example structure:

```text
build/
  debug/
    deps/
    objects/
    lib/
    bin/
  release/
    deps/
    objects/
    lib/
    bin/
```

Build artifacts are not stored in the global dependency cache.

This allows:

- simple project cleanup
- predictable artifact locations
- straightforward inspection of build outputs

#### 2.6.8 Cleaning build artifacts

The command:

```text
cxc clean
```

removes the project's local `build/` directory.

The global dependency source cache is not removed by this command.

Global dependency cache maintenance may be handled by separate tooling.

```text
cxc clean --cache
```

## 3. Scope Paths and Visibility

### 3.1 Path roots

CoreX path resolution roots:

| Root | Meaning |
|---|---|
| `root::` | current target root |
| `super::` | parent scope |
| `name::` | dependency import root, or the current project's library target name when analyzing a binary target |

Examples:

```text
use root::net::http;
use serde::json;
```

For `name::...` imports, `name` is resolved as either:

- a dependency binding name from the current project's `[dependencies]` table,
  or
- the current project's library target name during binary-target analysis.

### 3.2 Scope declarations

Scopes are declared using:

```text
scope foo;
```

Grammar:

```ebnf
scope_decl = [ visibility ], "scope", identifier, ";" ;
```

Resolution rules:

A scope declared with:

```text
scope foo;
```

is resolved by locating either:

- `foo.cx`
- `foo/foo.cx`

No implicit file discovery is performed for scopes.

If `foo/foo.cx` is used, that file defines the scope `foo`, and other files
under `foo/` are candidate child scopes of `foo`.

A child scope exists in the scope graph only if declared by its parent using
`scope <name>;`.

A file under a scope directory that is not declared by the parent scope is not
part of compilation.

If neither `foo.cx` nor `foo/foo.cx` exists, it is a scope resolution error.

If both `foo.cx` and `foo/foo.cx` exist in the same parent scope, the
declaration is ambiguous and must be rejected.

### 3.4 Scope resolution errors

- Missing declared scope (`scope foo;` with no `foo.cx` and no `foo/foo.cx`) is
  a scope resolution error.
- Ambiguous declared scope (`foo.cx` and `foo/foo.cx` both present) is a scope
  resolution error.
- Cyclic scope declarations are invalid and reported as scope resolution
  errors, not grammar errors.

### 3.3 Visibility

Visibility forms:

- `private` (default)
- `pub(super)`
- `pub(project)`
- `pub`

| Visibility | Meaning |
|---|---|
| `private` | visible only in current scope |
| `pub(super)` | visible to parent scope and scopes inside that parent |
| `pub(project)` | visible anywhere in the same project |
| `pub` | exported outside the project |

Scope declarations follow the same visibility rules:

```text
scope net;
pub(project) scope util;
pub scope api;
```

## 4. Lexical Conventions

### 4.1 Identifiers

```ebnf
identifier      = ident_start, { ident_continue } ;
ident_start     = "_" | letter ;
ident_continue  = "_" | letter | digit ;
```

Current frontend implementation note:

- Identifiers are ASCII-only (`_`, `A-Z`, `a-z`, `0-9`).

### 4.2 Types

CoreX distinguishes between native references, raw pointers, and foreign pointers.

| Type form | Syntax | Meaning |
|---|---|---|
| Shared borrow | `&T` | Borrowed shared reference to `T` |
| Exclusive borrow | `&mut T` | Borrowed exclusive mutable reference to `T` |
| Raw const pointer | `*T` | Raw const pointer to `T` |
| Raw mutable pointer | `*mut T` | Raw mutable pointer to `T` |

Rules:
- `&T` and `&mut T` are native borrow semantics with lifetime guarantees.
- `*T` and `*mut T` are raw pointers without lifetime guarantees.
- Raw pointer dereference requires `unsafe`.

### 4.3 Literals

```ebnf
literal            = integer_literal
                   | float_literal
                   | char_literal
                   | string_literal
                   | boolean_literal
                   ;
```

Literal notes:

- Numeric literals allow `_` separators in integer and float spellings.
- Supported integer examples: `123`, `0x7A`, `0o65`, `044`, `87u8`, `87_u8`.
- Supported float examples: `1.25`, `1e9`, `1.0e-3`, `2E+10`.
- A dot starts the fractional part of a float only when followed by a digit; this avoids conflict with range operators.
- `.5` and `1.` are not part of this literal surface.
- `string` values are UTF-8 throughout the language.

### 4.3 Whitespace and Comments

```ebnf
whitespace              = { " " | "\t" | "\r" | "\n" } ;

line_comment            = "//", { non_newline_char }, [ "\n" ] ;
doc_line_comment        = "///", { non_newline_char }, [ "\n" ] ;
inner_doc_line_comment  = "//!", { non_newline_char }, [ "\n" ] ;

block_comment           = "/*", { block_comment_char }, "*/" ;
doc_block_comment       = "/**", { block_comment_char }, "*/" ;
inner_doc_block_comment = "/*!", { block_comment_char }, "*/" ;

comment                 = line_comment
                        | doc_line_comment
                        | inner_doc_line_comment
                        | block_comment
                        | doc_block_comment
                        | inner_doc_block_comment
                        ;
```

Comments are lexical trivia.
Doc comments (`///`, `//!`, `/** */`, `/*! */`) are distinct comment forms, not general attributes.
Block comments always close with `*/`.
Outer doc comments (`///`, `/** */`) may attach to the following declaration.
Inner doc comments (`//!`, `/*! */`) are classified distinctly but are not yet
fully attached semantically in this frontend stage.

### 4.4 Statement Termination

```ebnf
terminated_stmt = stmt, ";" ;
```

Simple statements require `;`.
Blocks may also end with a tail expression (final expression without `;`).

## 5. File Structure

```ebnf
file = { item } ;

item = use_item
     | scope_decl
     | struct_decl
     | enum_decl
     | impl_decl
     | protocol_decl
     | fn_decl
     | extern_block
     ;
```

## 6. Use Items

```ebnf
visibility = "pub"
           | "pub", "(", "super", ")"
           | "pub", "(", "project", ")"
           ;

use_item = [ visibility ], "use", use_tree, ";" ;

use_tree = use_path
         | use_path, "as", identifier
         | use_path, "::", "*"
         | use_path, "::", use_group
         ;

use_group = "{", use_group_item, { ",", use_group_item }, [ "," ], "}" ;

use_group_item = use_tree
               | "self"
               | "self", "as", identifier
               ;

use_path = use_root, { "::", use_path_segment } ;

use_path_segment = identifier
                 | "scope"
                 ;

use_root = "root"
         | "super"
         | identifier
         | "scope"
         ;
```

Supported forms:

```text
use root::scope::Thing;
use scope::Thing;
use super::Thing;
use depname::Thing;

use root::scope::*;
use scope::*;

use root::scope::{A, B, C};
use root::scope::{scope::*, scope::{self, SomeThing}};
use root::net::{self as net_root, http};

use root::scope::scope as SomethingElse;

pub use root::api::Client;
pub(project) use root::internal::helper;
pub use root::fmt::Writer as OutWriter;

use root::scope::{self, SomeThing};
use root::scope::{a::b, c::d as E, f::*};
pub(project) use root::internal::{helpers::*, model::{self, Id}};
```

Grouped `self` rules:

- `self` and `self as <alias>` are valid inside grouped imports.
- grouped `self` refers to the current group base path.
- `self` is not a general `use_path` segment outside grouped entries.
- empty groups such as `use root::foo::{};` are invalid.

## 7. Modifiers

```ebnf
modifier      = "unsafe"
            | "async"
            ;
modifier_list = { modifier } ;
```

### 7.1 Unsafe Modifier

`unsafe` marks boundaries where compiler guarantees are suspended.

`unsafe` placement:
- Functions: `unsafe fn` - function body requires unsafe operations
- Initializers: `unsafe init` - initializer may bypass safety checks
- Impl blocks: `unsafe impl` - implementation uses unsafe operations
- Closures: `unsafe { ... }` - closure body requires unsafe operations
- Blocks: `unsafe { ... }` - unsafe block within safe code

`unsafe` semantics:
- Compiler does not enforce memory safety or borrowing rules inside `unsafe`.
- Raw pointer dereference is only permitted within `unsafe`.
- Direct calls to foreign functions without safe wrappers are permitted.
- `unsafe` on a function definition does not make callers unsafe.
- `unsafe` on a function signature means the implementation is trusted, not that calling it is unsafe.

Visibility is a separate declaration prefix category and is not part of
`modifier_list`.

## 8. Functions and Initializers

### 8.1 Function Declarations

```ebnf
fn_decl      = [ visibility ], modifier_list, "fn", identifier, [ generic_params ],
               "(", [ param_list ], ")", [ return_type ],
               [ where_clause ], block ;
return_type  = "->", type ;
```

`unsafe fn` declares a function whose body may use unsafe operations. This does not make calling the function unsafe.

### 8.2 Initializer Declarations

```ebnf
init_decl    = modifier_list, "init", "(", [ param_list ], ")", block ;
```

Current parser behavior:
- `init` declarations do not accept visibility prefixes in struct/enum/impl
  member contexts.

### 8.3 Parameter Forms

```ebnf
param_list      = param, { ",", param } ;

param           = receiver_param | labeled_param ;

receiver_param  = "self"
                | "&", "self"
                | "&", "mut", "self"
                ;

labeled_param   = identifier, ":", type
                | "_", identifier, ":", type
                | identifier, identifier, ":", type
                ;
```

Examples:

```text
x: i32
_ value: i32
from str: string
&self
&mut self
```

### 8.4 Generics and Where Clauses

```ebnf
generic_params      = "<", generic_param_list, ">" ;
generic_param_list  = generic_param, { ",", generic_param } ;
generic_param       = identifier ;

where_clause        = "where", where_predicate_list ;
where_predicate_list = where_predicate, { ",", where_predicate } ;
where_predicate     = type, ":", type_bound_list ;
type_bound_list     = type, { "+", type } ;
```

## 9. Type Declarations

### 9.1 Structs

```ebnf
struct_decl   = [ visibility ], modifier_list, "struct", identifier, [ generic_params ],
                struct_body ;

struct_body   = "{", { struct_member }, "}" ;

struct_member = field_decl
              | init_decl
              | fn_decl
              ;

field_decl    = identifier, ":", type, "," ;
```

### 9.2 Enums

```ebnf
enum_decl           = [ visibility ], modifier_list, "enum", identifier, [ generic_params ],
                      enum_body ;

enum_body           = "{", { enum_member }, "}" ;

enum_member         = enum_case_decl
                    | init_decl
                    | fn_decl
                    ;

enum_case_decl      = identifier, [ enum_case_payload ], "," ;
enum_case_payload   = "(", [ enum_case_param_list ], ")" ;
enum_case_param_list = enum_case_param, { ",", enum_case_param } ;

enum_case_param     = type
                    | identifier, ":", type
                    ;
```

### 9.3 Impl Blocks

```ebnf
impl_decl             = "impl", type, [ protocol_conformance ], impl_body ;
protocol_conformance  = "for", type ;
impl_body             = "{", { impl_member }, "}" ;

impl_member           = init_decl | fn_decl ;

Unsafe impl blocks:
`unsafe impl` marks that the implementation uses unsafe operations. This does not make using the impl unsafe.
```

### 9.4 Builtin primitive type names

Builtin primitive type names are recognized semantically as predefined types
while remaining ordinary identifier-shaped names in source:

- `u8`, `u16`, `u32`, `u64`, `usize`
- `i8`, `i16`, `i32`, `i64`, `isize`
- `f32`, `f64`
- `bool`, `char`, `string`, `void`

## 10. Foreign Declarations

### 10.1 Foreign Domains

CoreX identifies foreign domains through call convention attributes.

Foreign domain identification:
- Call convention attributes (`@call(.C)`, `@call(.ObjC)`) specify the FFI domain.
- The domain is determined by the supported calling convention.
- FFI domains are first-class when the corresponding call convention is implemented.

Supported foreign domains:

| Domain | Call Convention | Description |
|---|---|---|
| C | `@call(.C)` | C calling convention and ABI |
| Objective-C | `@call(.ObjC)` | Objective-C calling convention and ABI |

Foreign domain rules:
- Foreign values do not automatically become native owned CoreX values.
- Foreign pointers remain raw handles unless explicitly wrapped.
- Native ownership/borrowing and foreign residency are separate semantic axes.
- Safe wrappers can be built on top of raw foreign interfaces.
- Domains without supported call convention implementation are not first-class FFI.

Note: Foreign domains are identified through call convention attributes. The `extern` block library name is symbolic and does not encode domain.

Example:

```text
@call(.C)
extern libSystem {
    fn strlen(s: *void) -> usize;
}

@call(.ObjC)
extern libObjC {
    fn objc_msgSend(obj: *mut void, selector: *const char) -> *mut void;
}
```

### 10.2 Extern Block

Foreign blocks use call convention attributes to identify the FFI domain.

```ebnf
extern_block       = { attribute }, "extern", identifier, "{",
                     { extern_member },
                     "}" ;
```

Calling convention attributes:
- `@call(.C)` - Identifies C calling convention and FFI domain
- `@call(.ObjC)` - Identifies Objective-C calling convention and FFI domain
- Function-level `@call(...)` overrides block-level attribute

Example:

```text
@call(.C)
extern libSystem {
    fn strlen(s: *void) -> usize;
    fn pid = getpid() -> i32;
}

@call(.ObjC)
extern libObjC {
    fn objc_msgSend(obj: *mut void, selector: *const char) -> *mut void;
}
```

### 10.3 Foreign Function Declaration

```ebnf
extern_member      = { attribute },
                     "fn", identifier, [ "=", identifier ],
                     "(", [ extern_param_list ], ")",
                     [ return_type ],
                     ";" ;
```

### 10.4 Foreign Parameter Forms

```ebnf
extern_param_list  = extern_param, { ",", extern_param } ;

extern_param       = labeled_param ;
```

### 10.5 Supported Foreign Type Surface (Current Parser)

Foreign declarations currently reuse the frontend `type` parser surface. The
common FFI subset used by examples includes:

```ebnf
ffi_common_type = "void"
                | "i32"
                | "usize"
                | pointer_type
                ;

pointer_type = "*", [ "mut" ], "void" ;
```

Examples:

```text
@call(.C)
extern libSystem {
    fn strlen(_ s: *void) -> usize;
    fn pid = getpid() -> i32;
}
```

Semantic notes:

1. The extern library name is symbolic and does not encode a file path.
2. Concrete target-specific library paths are resolved through `corex.foreign.toml`.
3. Function-level `@call(...)` overrides block-level `@call(...)`.
4. If no explicit call convention is provided, the default foreign calling convention is `C`.
5. `fn local = symbol(...) -> T;` declares a local imported name distinct from the native symbol name.

## 11. Unified Declarative Macro System

CoreX macros use one unified compile-time syntax expansion model.

### 11.1 Design goals

CoreX macros are designed to cover:

- simple declarative syntax rewriting
- derive-style item expansion
- structured compile-time AST reflection
- hygienic syntax generation

Core principles:

- macros expand before HIR lowering
- macros operate on syntax, not semantic/type information
- macros are hygienic by default
- macros emit syntax, not direct semantic IR
- macros use a declarative surface syntax
- structured AST reflection is allowed, but read-only

This model is intentionally aimed at covering most practical `macro_rules!` and many proc-macro-style use cases without requiring an immediate heavyweight procedural macro plugin system.

### 11.2 Compiler pipeline placement

Frontend pipeline:

```text
AST
→ Expanded AST
→ HIR / desugar
→ semantic analysis
→ later typed / borrow-checked lowering
```

Macro expansion runs between parsed AST and HIR lowering.

Macros consume AST fragments and emit expanded AST fragments.

Expansion must preserve source provenance so diagnostics, hover, inlay hints, and tooling can map expanded syntax back to original source locations.

### 11.3 Invocation syntax

Macros are invoked with `@`.

```ebnf
attribute       = "@", identifier, [ macro_args ] ;
macro_expr      = "@", identifier, [ macro_args ] ;
macro_expr_stmt = macro_expr, ";" ;

macro_args      = "(", [ argument_list ], ")"
                | block
                ;
```

Supported invocation families:

A. Attached/item-position invocation

```text
@derive
struct Foo { ... }

@derive(Debug, Clone)
enum Color { ... }
```

B. Call-style invocation

```text
@unless(x > 0, {
    print("non-positive");
})
```

C. Block-style invocation

```text
@sql {
    select * from users
}
```

The `@` prefix keeps macro invocation visually distinct from runtime calls.

### 11.4 Macro definition syntax

Macros are declared with `macro`.

```ebnf
macro_decl   = "macro", identifier, "{", { macro_clause }, "}" ;
macro_clause = rule_clause | reflect_clause ;

rule_clause    = "rule", "(", macro_param_list, ")", "=>", block, ";" ;
reflect_clause = "reflect", "(", macro_param_list, ")", "=>", block, ";" ;

macro_param_list = [ macro_param, { ",", macro_param } ] ;
macro_param      = identifier, ":", macro_input_kind ;
```

Unified form:

```text
macro name {
    rule(input: Tokens) => {
        ...
    };

    reflect(item: Item) => {
        ...
    };

    reflect(item: Item, args: MacroArgs) => {
        ...
    };
}
```

Clause order is significant only if dispatch uses first-match behavior for overlap.

### 11.5 Clause kinds

#### 11.5.1 `rule`

`rule` is for ordinary declarative syntax rewriting.

```text
macro unless {
    rule(cond: Expr, body: Block) => {
        if !cond body
    };
}
```

Typical use:

- lightweight syntax sugar
- expression/block rewrites
- token/block wrappers
- small local syntax DSLs

#### 11.5.2 `reflect`

`reflect` is for declarative expansion requiring structured AST inspection.

```text
macro derive_debug {
    reflect(item: Item) => {
        if item.kind == .Enum {
            ...
        } else if item.kind == .Struct {
            ...
        } else {
            error("derive_debug only supports enum or struct");
        }
    };
}
```

Typical use:

- derive-like expansion
- enum/struct/function shape inspection
- boilerplate generation
- compile-time branching on syntax structure

`reflect` remains syntax-oriented and declarative; it is not a general compiler plugin API.

### 11.5.1 Supported Macro Forms

The CoreX macro system currently supports a well-defined subset of macro forms:

**Expression Macros (Rule Clauses)**:
- Call-style: `@macro(arg1, arg2)` with `rule(...: Expr)`
- Block-style: `@macro { tokens }` with `rule(...: Tokens)`

**Item Macros (Reflect Clauses)**:
- Attached: `@macro struct Foo { ... }` with `reflect(...: Item)`

**Not Yet Supported**:
- Attached rule macros: `rule(...: Item)` - use `reflect(...: Item)` instead
- Expression reflection: `reflect(...: Expr)` - use `rule(...: Expr)` instead
- Block reflection: `reflect(...: Tokens)` - use `rule(...: Tokens)` instead
- Statement/Type/Pattern inputs: Declared but not implemented
- MacroArgs parameter: Declared but not implemented

Attempting to use unsupported forms will result in clear, actionable error messages that guide you to the correct alternative.

### 11.6 Input kinds

Supported macro input classes:

#### 11.6.1 Structured syntax inputs

- `Item`
- `Expr`
- `Stmt`
- `Block`
- `Type`
- `Pattern`

These are available in `reflect(...)`, and may later be available for richer `rule(...)` matching.

#### 11.6.2 Raw-ish syntax inputs

- `Tokens`
- `MacroArgs`

`Tokens` is a raw syntax bundle for catch-all rewriting.

`MacroArgs` models parsed macro argument lists (for example `@derive(Debug, Clone)`), initially as a structured sequence of simple arguments, extensible over time.

### 11.7 Suggested dispatch model

Dispatch is by invocation form and compatible clause signature.

Examples:

- `@derive struct Foo { ... }` -> compatible `reflect(item: Item)`
- `@derive(Debug, Clone) struct Foo { ... }` -> compatible `reflect(item: Item, args: MacroArgs)`
- `@unless(...)` or `@sql { ... }` -> compatible `rule(...)`

When multiple clauses are compatible, first applicable clause may be selected; otherwise expansion fails with a macro expansion error.

### 11.8 Reflection model

`reflect(...)` receives a read-only syntax reflection view.

Must NOT expose:

- inferred types
- resolved imports
- semantic item ids
- borrow information
- dataflow/ownership facts

May expose:

- syntactic kind
- name
- visibility
- attributes
- generics
- parameters
- fields
- variants
- bodies (where allowed)
- spans/provenance metadata

Conceptual API:

```text
item.kind
item.name
item.visibility
item.attrs

item.as_struct().fields
field.name
field.ty

item.as_enum().variants
variant.name
variant.payload

item.as_function().params
item.as_function().return_type
```

### 11.9 Syntax generation model

Macro clauses emit syntax fragments appropriate to invocation position.

Examples:

- item-position invocation emits items/item fragments
- expression-position invocation emits expressions
- statement/block-position invocation emits statements/blocks

Expanded output re-enters the standard pipeline as Expanded AST.

### 11.10 Hygiene model

Macros are hygienic by default.

#### 11.10.1 Macro-introduced bindings

Macro-introduced names are freshened to avoid collisions.

```text
let tmp = ...
```

may expand to an internal fresh binding conceptually like:

```text
let tmp#generated123 = ...
```

#### 11.10.2 Reflected/input names

Names originating from macro input preserve call-site identity.

#### 11.10.3 Literal names in macro definitions

Literal names written in macro definitions resolve in macro-definition scope by default.

This yields a predictable split:

- generated internals are hygienically fresh
- user-provided names preserve user identity
- literal references in macro definitions are definition-scoped

### 11.11 Relationship to future procedural macros

This unified declarative+reflection model is intended to absorb many proc-macro-like use cases:

- derive-like item generation
- syntax introspection of enum/struct/function items
- boilerplate generation
- lightweight syntax DSLs
- surface syntax sugar expansion

Full procedural macros may be added later, but are not required for initial macro architecture.

### 11.12 Examples

#### 11.12.1 Simple rule macro

```text
macro unless {
    rule(cond: Expr, body: Block) => {
        if !cond body
    };
}
```

Use:

```text
@unless(x == 0, {
    print("nonzero");
});
```

#### 11.12.2 Attached derive-style macro without args

```text
macro derive_debug {
    reflect(item: Item) => {
        if item.kind == .Enum {
            ...
        } else if item.kind == .Struct {
            ...
        } else {
            error("derive_debug only supports enum or struct");
        }
    };
}
```

Use:

```text
@derive_debug
enum Color { Red, Green, Blue }
```

#### 11.12.3 Attached derive-style macro with args

```text
macro derive {
    reflect(item: Item, args: MacroArgs) => {
        for arg in args {
            ...
        }
    };
}
```

Use:

```text
@derive(Debug, Clone, Eq)
struct Point { x: i32, y: i32 }
```

#### 11.12.4 Token/block-style macro

```text
macro sql {
    rule(input: Tokens) => {
        ...
    };
}
```

Use:

```text
@sql {
    select * from users
}
```

### 11.13 Source provenance requirements

Expanded AST nodes should retain origin metadata such as:

- direct source node
- expanded-from source node
- synthetic/generated for source node

This preserves usability for diagnostics and tooling.

### 11.14 Desugar interaction

Macro expansion runs before desugar.

Desugar consumes Expanded AST.

Responsibility split:

- macro expansion: syntax generation
- desugar: surface normalization
- semantic analysis: meaning

### 11.15 Non-goals for first version

Not included initially:

- full semantic reflection
- inferred type queries inside macros
- borrow-checker data inside macros
- arbitrary compiler plugin execution
- full Rust-style token-tree complexity
- advanced hygiene control surface
- full procedural macro ABI

### 11.16 Summary

CoreX uses one unified macro model:

```text
macro name {
    rule(...) => ...;
    reflect(...) => ...;
}
```

Invocation forms:

- `@name`
- `@name(...)`
- `@name { ... }`

The system provides declarative rewriting, structured compile-time AST reflection, hygienic expansion, and syntax output into Expanded AST.

## 12. Patterns

```ebnf
pattern                 = wildcard_pattern
                        | identifier_pattern
                        | literal_pattern
                        | tuple_pattern
                        | variant_pattern
                        | struct_pattern
                        | array_pattern
                        ;

wildcard_pattern        = "_" ;
identifier_pattern      = identifier ;

literal_pattern         = integer_literal
                        | boolean_literal
                        | char_literal
                        | string_literal
                        ;

tuple_pattern           = "(", pattern, ",", [ pattern_list ], [ "," ], ")" ;
pattern_list            = pattern, { ",", pattern } ;

variant_pattern         = [ "." ], identifier, [ variant_pattern_payload ] ;
variant_pattern_payload = "(", [ variant_pattern_arg_list ], ")" ;
variant_pattern_arg_list = variant_pattern_arg, { ",", variant_pattern_arg } ;
variant_pattern_arg     = pattern | ".." ;

struct_pattern          = identifier, "{", [ struct_pattern_field_list ], [ "," ], "}" ;
struct_pattern_field_list = struct_pattern_field, { ",", struct_pattern_field } ;
struct_pattern_field    = identifier
                        | identifier, ":", pattern
                        | ".."
                        ;

array_pattern           = "[", [ array_pattern_entry_list ], "]" ;
array_pattern_entry_list = array_pattern_entry, { ",", array_pattern_entry } ;
array_pattern_entry     = pattern
                        | ".."
                        | "..", identifier
                        ;
```

Pattern notes:

- `_ => value` ignores matched input.
- `x => x` binds the matched input to `x`.
- Variant shorthand forms like `.none`, `.some(x)`, and `.some(..)` are part of source pattern surface.
- Tuple patterns require at least one comma; `(x)` is not tuple-pattern syntax.
- Array/variant/struct rest marker `..` is allowed at most once and must be final.

#### Match Exhaustiveness

`match` expressions must be exhaustive: all possible values must be handled.

```text
enum Color {
    red,
    green,
    blue,
}

let c = Color.red;
// Error: non-exhaustive match
let name = match c {
    .red => "red",
    .green => "green",
};
```

The `_` wildcard pattern can cover unmatched cases:

## 13. Blocks and Statements

```ebnf
block             = "{", { stmt }, [ tail_expr ], "}" ;
tail_expr         = expr ;

stmt              = let_stmt
                  | var_stmt
                  | if_stmt
                  | guard_stmt
                  | while_stmt
                  | for_stmt
                  | return_stmt
                  | break_stmt
                  | continue_stmt
                  | expr_stmt
                  ;

let_stmt          = "let", pattern, [ ":", type ], [ "=", expr ], ";" ;
var_stmt          = "var", pattern, [ ":", type ], [ "=", expr ], ";" ;
expr_stmt         = expr, ";" ;
return_stmt       = "return", [ expr ], ";" ;
break_stmt        = "break", ";" ;
continue_stmt     = "continue", ";" ;
```

Block/statement notes:

- A block may end with a tail expression (final expression without trailing `;`).
- If an expression is followed by `;`, it is an expression statement, not a tail expression.
- Clause bindings (`if let`, `guard let`, `while let`) still require `= expr`.

## 14. Clause Lists and Control Statements

```ebnf
clause_list       = clause, { ";", clause } ;
clause            = expr
                  | "let", pattern, [ ":", type ], "=", expr
                  | "var", pattern, [ ":", type ], "=", expr
                  ;

if_stmt           = "if", clause_list, block, [ if_stmt_else ] ;
if_stmt_else      = "else", ( block | if_stmt ) ;

guard_stmt        = "guard", clause_list, "else", block ;
while_stmt        = "while", clause_list, block ;
for_stmt          = "for", pattern, "in", expr, block ;
```

Statement-form notes:

- Statement-form `if` does not require `else`.
- `else if` chaining is supported through recursive `if_stmt_else`.
- `guard` is statement-only and always requires `else` block.

## 15. Expressions

### 15.1 Expression precedence overview

From lowest precedence to highest precedence:

| Level | Category | Associativity |
|---|---|---|
| 1 | Assignment (`=`, compound assignment forms) | right |
| 2 | Ternary (`?:`) | right |
| 3 | Range (`..`, `..=`) | single-op conservative |
| 4 | Null coalescing (`??`) | right |
| 5 | Logical OR (`||`) | left |
| 6 | Logical AND (`&&`) | left |
| 7 | Bitwise OR (`|`) | left |
| 8 | Bitwise XOR (`^`) | left |
| 9 | Bitwise AND (`&`) | left |
| 10 | Equality (`==`, `!=`) | left |
| 11 | Comparison (`<`, `<=`, `>`, `>=`) | left |
| 12 | Shift (`<<`, `>>`) | left |
| 13 | Additive (`+`, `-`) | left |
| 14 | Multiplicative (`*`, `/`, `%`) | left |
| 15 | Cast (`as`, `as?`) | left |
| 16 | Prefix (`try`, `!`, `-`) | right |
| 17 | Postfix (`()`, `[]`, `?[]`, `.`, `?.`, `::`, postfix `!`) | left |
| 18 | Primary/control atoms | n/a |

This table is the parser contract for the currently implemented expression surface.

### 15.2 Operators currently assumed by the grammar

```ebnf
assignment_op       = "="
                    | "+="
                    | "-="
                    | "*="
                    | "/="
                    | "%="
                    | "^="
                    | "|="
                    | "&="
                    | "<<="
                    | ">>="
                    ;
ternary_op          = "?" , ":" ;
range_op            = ".." | "..=" ;
null_coalescing_op  = "??" ;
logical_or_op       = "||" ;
logical_and_op      = "&&" ;
bitwise_or_op       = "|" ;
bitwise_xor_op      = "^" ;
bitwise_and_op      = "&" ;
equality_op         = "==" | "!=" ;
comparison_op       = "<" | "<=" | ">" | ">=" ;
shift_op            = "<<" | ">>" ;
additive_op         = "+" | "-" ;
multiplicative_op   = "*" | "/" | "%" ;
cast_op             = "as" | "as?" ;
prefix_op           = "!" | "-" | "try" ;
```

Token and parser operator surface are aligned with the list above.

### 15.3 Grammar by precedence level

```ebnf
expr                = assignment_expr ;

assignment_expr     = ternary_expr
                    | ternary_expr, assignment_op, assignment_expr
                    ;

ternary_expr        = range_expr
                    | range_expr, "?", expr, ":", ternary_expr
                    ;

range_expr          = null_coalescing_expr
                    | null_coalescing_expr, "..", [ null_coalescing_expr ]
                    | null_coalescing_expr, "..=", null_coalescing_expr
                    | "..", null_coalescing_expr
                    | "..=", null_coalescing_expr
                    ;

null_coalescing_expr = logical_or_expr
                     | logical_or_expr, null_coalescing_op, null_coalescing_expr
                     ;

logical_or_expr     = logical_and_expr, { logical_or_op, logical_and_expr } ;

logical_and_expr    = bitwise_or_expr, { logical_and_op, bitwise_or_expr } ;

bitwise_or_expr     = bitwise_xor_expr, { bitwise_or_op, bitwise_xor_expr } ;

bitwise_xor_expr    = bitwise_and_expr, { bitwise_xor_op, bitwise_and_expr } ;

bitwise_and_expr    = equality_expr, { bitwise_and_op, equality_expr } ;

equality_expr       = comparison_expr, { equality_op, comparison_expr } ;

comparison_expr     = shift_expr, { comparison_op, shift_expr } ;

shift_expr          = additive_expr, { shift_op, additive_expr } ;

additive_expr       = multiplicative_expr, { additive_op, multiplicative_expr } ;

multiplicative_expr = cast_expr, { multiplicative_op, cast_expr } ;

cast_expr           = prefix_expr, { "as", [ "?" ], type } ;

prefix_expr         = "try", prefix_expr
                    | "!", prefix_expr
                    | "-", prefix_expr
                    | postfix_expr
                    ;
```

`try` is unified propagation for both `Result` and `Option` and is resolved semantically using the enclosing return type.

Range examples:

- `1..3`
- `1..=2`
- `1..`
- `..10`
- `1.0..2.0`
- `1.0..=2.0`

Range operands are expressions, so both integer and float endpoints are valid.

### 15.4 Postfix expressions

```ebnf
postfix_expr            = primary_expr, { postfix_suffix } ;

postfix_suffix          = member_suffix
                        | optional_member_suffix
                        | namespace_suffix
                        | call_suffix
                        | index_suffix
                        | optional_index_suffix
                        | force_unwrap_suffix
                        ;

member_suffix           = ".", identifier ;
optional_member_suffix  = "?.", identifier ;
namespace_suffix        = "::", identifier, [ turbofish_suffix ] ;
call_suffix             = "(", [ argument_list ], ")" ;
index_suffix            = "[", expr, "]" ;
optional_index_suffix   = "?", "[", expr, "]" ;
force_unwrap_suffix     = "!" ;

argument_list           = argument, { ",", argument } ;
argument                = expr
                        | identifier, ":", expr
                        ;

turbofish_suffix        = "<", type_list, ">" ;
```

Postfix notes:

- `?.` and `?[]` are optional-chaining postfix forms.
- postfix `!` is force unwrap and is distinct from prefix logical-not.
- `::` binds namespace/type/static lookup, while `.`/`?.` bind value/member lookup.

Examples:

- `foo()`
- `foo(x: 1, y: 2)`
- `xs[0]`
- `value?.member`
- `value?[index]`
- `value!.member`
- `usize::from(x)`
- `value.method()`
- `Type::make::<T>(x)`

### 15.5 Primary expressions

```ebnf
primary_expr              = literal
                          | identifier
                          | shorthand_member_expr
                          | qualified_enum_case_expr
                          | self_expr
                          | grouped_expr
                          | array_literal
                          | struct_literal
                          | if_expr
                          | match_expr
                          | closure_expr
                          | macro_expr
                          ;

self_expr                 = "self" | "Self" ;
shorthand_member_expr     = ".", identifier ;
qualified_enum_case_expr  = identifier, ".", identifier ;
grouped_expr              = "(", expr, ")" ;
```

Notes:

- `.variant` is parsed uniformly as a shorthand member expression and resolved semantically when type context exists
- `Type.variant` is valid enum-case qualification syntax
- `Type::name(...)` is for static/module namespace access

### 15.6 Spread expressions (literal contexts only)

```ebnf
spread_expr = "..", expr ;
```

`spread_expr` is currently modeled in AST but not part of the parser's array/struct literal entry grammar in this parser stage.

### 15.7 Array literals

```ebnf
array_literal      = "[", [ array_element_list ], "]" ;
array_element_list = expr, { ",", expr } ;
```

Examples:

- `[]`
- `[1, 2, 3]`

### 15.8 Struct literals

```ebnf
struct_literal         = type_expr, "{", [ struct_field_init_list ], [ "," ], "}" ;

type_expr              = identifier | "Self" ;

struct_field_init_list = struct_field_init, { ",", struct_field_init } ;
struct_field_init      = identifier
                       | identifier, ":", expr
                       ;
```

Examples:

- `Self { inner }`
- `Type { inner: 1, other }`

Shorthand form means field name and local variable name match.

Parser note:
- Struct literal parsing is intentionally conservative and starts only from
  type-like heads accepted by parser heuristics.

### 15.9 If and match expressions

```ebnf
if_expr          = "if", clause_list, block, "else", if_expr_else ;
if_expr_else     = if_expr | block | expr ;

match_expr       = "match", expr, "{", [ match_arm_list ], [ "," ], "}" ;
match_arm_list   = match_arm, { ",", match_arm } ;
match_arm        = pattern, "=>", match_arm_body ;
match_arm_body   = expr | block ;
```

Expression-form notes:

- Expression-form `if` requires `else`.
- `else if` chaining is supported via recursive `if_expr`.
- `else { ... }` is represented as an actual else block expression branch.

### 15.10 Closures

```ebnf
closure_expr       = [ "unsafe" ], "{", [ closure_signature ], closure_body, "}" ;

closure_signature  = closure_param_list, "in" ;
closure_param_list = closure_param, { ",", closure_param } ;
closure_param      = identifier
                   | identifier, ":", type
                   ;

closure_body       = { stmt }, [ expr ] ;
```

`unsafe { ... }` marks a closure body requiring unsafe operations. This is distinct from `unsafe fn` which marks the function implementation as trusted.

Examples:

- `{ print($0) }`
- `{ p1 in print(p1) }`
- `{ p1: string in print(p1) }`

Semantic rule:

- `$0`, `$1`, etc. are only valid when there is no explicit closure parameter list
- In expression position, `{ ... }` parses as closure syntax. Generic block
  expressions are only produced in explicit grammar contexts (for example
  `if ... else { ... }` branches).

## 16. Protocol declarations

Protocols combine trait-like requirements with protocol-oriented surface syntax.

```ebnf
protocol_decl        = [ visibility ], modifier_list, "protocol", identifier, [ generic_params ],
                       [ protocol_inheritance ], protocol_body ;

protocol_inheritance = ":", type_list ;

protocol_body        = "{", { protocol_member }, "}" ;

protocol_member      = protocol_fn_req
                     | protocol_init_req
                     | protocol_assoc_type
                     | protocol_property_req
                     ;

protocol_fn_req      = modifier_list, "fn", identifier, [ generic_params ],
                       "(", [ protocol_param_list ], ")",
                       [ return_type ], [ where_clause ],
                       ( ";" | block ) ;

protocol_init_req    = modifier_list, "init", "(", [ protocol_param_list ], ")",
                        ( ";" | block ) ;

protocol_assoc_type  = "type", identifier, [ ":", type_bound_list ], ";" ;

protocol_property_req = modifier_list, ( "let" | "var" ), identifier, ":", type,
                        "{", protocol_accessor_list, "}" ;

protocol_accessor_list = protocol_accessor, { protocol_accessor } ;
protocol_accessor      = "get" | "set" ;

protocol_param_list  = protocol_param, { ",", protocol_param } ;
protocol_param       = receiver_param | labeled_param ;
```

Examples:

```text
protocol Observable {
    fn observe(&self) -> string;
}

protocol Collection: Observable {
    type Element;
    fn len(&self) -> usize;
    fn map<T>(_ f: fn(Element) -> T) -> [T];
}
```

Semantic notes:

1. A protocol member ending with `;` is a requirement only.
2. A protocol member with a block is a default implementation.
3. Receiver syntax in protocol methods follows the same rules as ordinary methods.
4. `impl Protocol for Type { ... }` is the conformance form.
5. Property requirements are declaration-only contracts; storage is not part of protocol syntax.

## 17. Option and Result Types

CoreX provides generic types `Option<T>` and `Result<T, E>` for optional and fallible initialization. These are ordinary generic types, not special built-in types.

### 17.1 Option Type

The `Option<T>` type represents optional values.

```text
enum Option<T> {
    Some(T),
    None,
}
```

Semantic behavior:
- `Option<T>` is a generic type wrapping any type `T`.
- `None` represents absence of value.
- `Some(T)` represents presence of value.
- Pattern matching on `Option<T>` extracts values safely.

### 17.2 Result Type

The `Result<T, E>` type represents fallible operations.

```text
enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

Semantic behavior:
- `Result<T, E>` wraps success with `Ok(T)` and error with `Err(E)`.
- `Ok(T)` represents successful computation with value `T`.
- `Err(E)` represents failed computation with error `E`.
- Pattern matching on `Result<T, E>` handles success and error cases.
- Commonly used for operations that may fail.

### 17.3 Initializer Signatures

Initializers have different result types based on their initialization mode.

Infallible initializer:
```text
init() -> Self {
    Self { }
}
```

Optional initializer:
```text
init() -> Option<Self> {
    if condition { Self { } else { None }
}
```

Fallible initializer:
```text
init() -> Result<Self, E> {
    if condition { Self { } } else { Err(error) }
}
```

#### Init Sugar

When the return type is `Self`, the `-> Self` may be omitted:

```text
init() {     // desugars to: init() -> Self {
    Self { }
}
```

This sugar applies only to initializers returning `Self`.

## 18. Operators for Optionals and Results

### 18.1 `?` Operator (Error to Option)

The `?` operator propagates errors from `Result<T, E>` to `Option<T>`.

```text
// Converts Result to Option
fn get_user(id: i32) -> Result<User, Error> {
    ...
}

let user = get_user(id)?;  // Result<User, Error> -> Option<User>
```

Semantic rules:
- When applied to `Result<T, E>`, converts `Ok(T)` to `Some(T)`.
- When applied to `Err(E)`, returns `None`.
- Only valid on `Result<T, E>` types.
- Chainable for sequential error propagation.

Optional access shorthand:
```text
let user = db.get(id)?.name;  // Option<User> -> Option<string>
```

### 18.2 `!` Operator (Unwrap Error)

The `!` operator extracts values from `Option<T>` or `Result<T, E>`, panicking if empty.

```text
let value = some_option;  // Some(T)
let name = value!;     // panics if value is None
```

Semantic rules:
- On `Option<T>`, panics if value is `None`.
- On `Result<T, E>`, panics if value is `Err(E)`.
- `??` (`??`) provides non-panicking alternative.
- Intentional panic, not error handling.

### 18.3 `??` Operator (Null Coalescing)

The `??` operator provides a default value when left-hand side is empty or invalid.

```text
let maybe_value: Option<i32> = None;
let value = maybe_value ?? 0;  // value is 0
```

Semantic rules:
- Left-hand side must have type `Option<T>`.
- Right-hand side type must match `T` from left-hand side.
- Not valid on `Result<T, E>` types.
- Non-panicking alternative to `!` operator.

## 19. Semantic Notes (Non-EBNF)

- Function aliasing in foreign declarations uses `fn local = native(...)`.
- In foreign lowering, function-level call convention overrides block-level.
- When no foreign call-convention attribute is present, default is C.
- Grammar here specifies syntax shape; runtime ABI truth is validated separately.

Example:
```text
@call(.C)
extern libSystem {
    fn strlen(s: *void) -> usize;
    fn pid = getpid() -> i32;
}

protocol Observable {
    fn observe(&self) -> string;
}

fn demo() -> i32 {
    let x = add(1, 2);
    if x > 0 { x } else { 0 }
}
```

## 20. Memory Model and Ownership

### 20.1 Native Value Ownership

CoreX uses Rust-style ownership semantics for native values.

Core principles:
- Every native value has exactly one owner at any time.
- When a value is assigned to a new binding, ownership transfers unless the type is `Copy`.
- When the owner goes out of scope, the value is dropped deterministically.
- No tracing garbage collection.
Move vs Copy:
- Types implementing `Copy` are implicitly copied on assignment.
- Non-`Copy` types are moved on assignment.
- After a move, the source binding is no longer usable.

Examples:

```text
let a = String("hello");
let b = a;  // moves a to b; a is no longer valid
// a.c_str();  // error: use of moved value

let x = 42;
let y = x;  // copies x; both x and y remain valid
```

### 20.2 Borrowed References

CoreX distinguishes borrowed references from raw pointers.

Borrow forms:
- `&T` - shared borrow, allows reading but not modification
- `&mut T` - exclusive mutable borrow, allows both reading and writing

Borrow rules:
- Borrows are validated at compile time.
- Multiple shared borrows (`&T`) can coexist.
- Only one exclusive borrow (`&mut T`) can exist, and no other borrows can coexist with it.
- Borrows must not outlive the value they borrow.

Example:

```text
fn shared_borrow(s: &String) -> usize {
    s.len()
}

fn exclusive_borrow(s: &mut String) {
    s.push('!')
}
```

### 20.3 Raw Pointers

Raw pointers provide manual memory control without borrow checking.

Raw pointer forms:
- `*T` - raw const pointer
- `*mut T` - raw mutable pointer

Raw pointer rules:
- No lifetime guarantees.
- No automatic dereferencing or bounds checking.
- Dereference requires `unsafe`.
- Used for FFI, custom allocators, and unsafe low-level operations.

Example:

```text
unsafe fn raw_ptr_example(ptr: *mut i32) {
    *ptr = 42;
}
```

### 20.4 Drop Behavior

The `Drop` compiler-recognized protocol defines cleanup behavior.

`Drop` semantics:
- When a value goes out of scope, its `drop` method is called.
- `Drop` is called exactly once per value.
- `drop` order is reverse of construction order.
- Types implementing `NoDrop` are not dropped and can be bulk-freed.

Example:

```text
protocol Drop {
    fn drop(&mut self);
}

struct Buffer {
    ptr: *mut u8,
    len: usize,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // free memory
    }
}
```

### 20.5 Allocator Awareness

CoreX design leaves room for allocator-aware destruction without specifying allocator parameters.

`NoDrop` for arena-style allocation:
- `NoDrop` types can be safely destroyed by freeing their arena.
- Arena allocators can track allocations and drop-free.
- `NoDrop` is a compiler-recognized protocol, distinct from `Drop`.

## 21. Compiler-Recognized Protocols (Lang Items)

CoreX recognizes a set of foundational protocols as compiler lang items.

Lang item protocols are distinguished from ordinary protocols:
- They have compiler-known semantic effects.
- They cannot be used as general traits for arbitrary types.
- Implementations may require `unsafe` where soundness requires explicit trust.

### 21.1 Drop

The `Drop` protocol defines custom cleanup logic.

```text
protocol Drop {
    fn drop(&mut self);
}
```

Semantic effect:
- Called deterministically when value goes out of scope.
- Must not panic or abort in normal operation.

### 21.2 Copy

The `Copy` protocol marks types that can be implicitly copied.

```text
protocol Copy {}
```

Semantic effect:
- Assignment copies instead of moves.
- Function arguments are copied instead of moved.
- Return values are copied instead of moved.
- Trivial types (primitives) implement `Copy` by default.

### 21.3 Clone

The `Clone` protocol defines explicit cloning semantics.

```text
protocol Clone {
    fn clone(&self) -> Self;
}
```

Semantic effect:
- Provides explicit `clone()` method for deep copying.
- Unlike `Copy`, `clone()` is an explicit method call.
- Non-`Copy` types should implement `Clone`.

### 21.4 NoDrop

The `NoDrop` protocol marks types safe for bulk destruction.

```text
protocol NoDrop {}
```

Semantic effect:
- Values are not dropped individually.
- Arena allocators can free entire arenas without calling drop.
- Types with manual memory management may implement `NoDrop`.

### 21.5 Sized

The `Sized` protocol marks types with known size at compile time.

```text
protocol Sized {}
```

Semantic effect:
- Used by generic bounds to require compile-time known size.
- Most types implement `Sized` by default.
- Dynamically-sized types (slices) may not implement `Sized`.

### 21.6 Deref

The `Deref` protocol defines pointer-like behavior.

```text
protocol Deref {
    fn deref(&self) -> &Self::Target;
}
```

Semantic effect:
- Enables smart pointer behavior via `*expr` operator.
- Used for custom pointer types, iterators, and wrappers.

### 21.7 Move

The `Move` protocol marks types with explicit move semantics (optional).

```text
protocol Move {}
```

Semantic effect:
- Used by generic bounds to require move-only types.
- `Copy` and `Move` are mutually exclusive.
- Non-`Copy` types are implicitly move-only.

## 22. Implementation Intent

CoreX semantics follow Rust-style ownership and borrowing principles.

Implementation characteristics:
- Deterministic destruction without tracing GC.
- Foreign domains (C, Objective-C) are first-class and distinct from native ownership.
- Unsafe boundaries are explicit and well-defined.
- Compiler-recognized protocols are surgical lang items, not general abstractions.
- Implementation architecture aims for cleaner compiler design than rustc, with incremental and query-friendly construction.

