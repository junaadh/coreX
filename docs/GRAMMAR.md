# coreX Grammar (Draft)

This document defines the current grammar draft in EBNF form.
It is organized by numbered sections and keeps semantic notes separate from syntax.

## 1. Scope

The grammar below currently focuses on:

- lexical conventions
- top-level item structure
- function/type declarations
- foreign declaration syntax used by the runtime pipeline

## 2. Lexical Conventions

### 2.1 Identifiers

```ebnf
identifier      = ident_start, { ident_continue } ;
ident_start     = "_" | letter ;
ident_continue  = "_" | letter | digit ;
```

### 2.2 Literals

```ebnf
literal            = integer_literal
                   | float_literal
                   | char_literal
                   | string_literal
                   | boolean_literal
                   ;

integer_literal    = dec_integer
                   | hex_integer
                   | oct_integer
                   | legacy_oct_integer
                   ;

dec_integer        = digit, { digit_or_sep } , [ integer_suffix ] ;
hex_integer        = "0x", hex_digit, { hex_digit_or_sep } , [ integer_suffix ] ;
oct_integer        = "0o", oct_digit, { oct_digit_or_sep } , [ integer_suffix ] ;
legacy_oct_integer = "0", oct_digit, { oct_digit_or_sep } , [ integer_suffix ] ;

float_literal      = dec_integer_no_suffix, ".", digit, { digit_or_sep }, [ exponent_part ]
                   | dec_integer_no_suffix, exponent_part
                   ;

dec_integer_no_suffix = digit, { digit_or_sep } ;

exponent_part      = ("e" | "E"), [ "+" | "-" ], digit, { digit_or_sep } ;

integer_suffix     = primitive_int_type
                   | "_", primitive_int_type
                   ;

char_literal       = "'", char_content, "'" ;
string_literal     = '"', { string_char | interpolation }, '"' ;
interpolation      = "\\(", expr, ")" ;
boolean_literal    = "true" | "false" ;
```

Literal notes:

- Numeric literals allow `_` separators in integer and float spellings.
- Supported integer examples: `123`, `0x7A`, `0o65`, `044`, `87u8`, `87_u8`.
- Supported float examples: `1.25`, `1e9`, `1.0e-3`, `2E+10`.
- A dot starts the fractional part of a float only when followed by a digit; this avoids conflict with range operators.
- `.5` and `1.` are not part of this literal surface.
- `string` values are UTF-8 throughout the language.

### 2.3 Whitespace and Comments

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

### 2.4 Statement Termination

```ebnf
terminated_stmt = stmt, ";" ;
```

Simple statements require `;`.
Blocks may also end with a tail expression (final expression without `;`).

## 3. File Structure

```ebnf
file = { item } ;

item = use_item
     | struct_decl
     | enum_decl
     | impl_decl
     | protocol_decl
     | fn_decl
     | extern_block
     ;
```

## 4. Use Items

```ebnf
use_item       = "use", path_use_tree, ";" ;
path_use_tree  = path_prefix, [ "::", use_tree ] | use_tree ;
path_prefix    = identifier, { "::", identifier } ;

use_tree       = identifier
               | "self"
               | "{", use_tree_list, [ "," ], "}"
               ;

use_tree_list  = use_tree, { ",", use_tree } ;
```

Example:

```text
use core::mod1::mod2::{self, st1, st2};
```

## 5. Modifiers

```ebnf
modifier      = "pub" | "async" ;
modifier_list = { modifier } ;
```

## 6. Functions and Initializers

### 6.1 Function Declarations

```ebnf
fn_decl      = modifier_list, "fn", identifier, [ generic_params ],
               "(", [ param_list ], ")", [ return_type ],
               [ where_clause ], block ;

return_type  = "->", type ;
```

### 6.2 Initializer Declarations

```ebnf
init_decl    = modifier_list, "init", [ init_kind ],
               "(", [ param_list ], ")", block ;

init_kind    = "?" | "!" ;
```

### 6.3 Parameter Forms

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

### 6.4 Generics and Where Clauses

```ebnf
generic_params      = "<", generic_param_list, ">" ;
generic_param_list  = generic_param, { ",", generic_param } ;
generic_param       = identifier ;

where_clause        = "where", where_predicate_list ;
where_predicate_list = where_predicate, { ",", where_predicate } ;
where_predicate     = type, ":", type_bound_list ;
type_bound_list     = type, { "+", type } ;
```

## 7. Type Declarations

### 7.1 Structs

```ebnf
struct_decl   = modifier_list, "struct", identifier, [ generic_params ],
                struct_body ;

struct_body   = "{", { struct_member }, "}" ;

struct_member = field_decl
              | init_decl
              | fn_decl
              ;

field_decl    = identifier, ":", type, "," ;
```

### 7.2 Enums

```ebnf
enum_decl           = modifier_list, "enum", identifier, [ generic_params ],
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

### 7.3 Impl Blocks

```ebnf
impl_decl             = "impl", type, [ protocol_conformance ], impl_body ;
protocol_conformance  = "for", type ;

impl_body             = "{", { impl_member }, "}" ;
impl_member           = init_decl | fn_decl ;
```

### 7.4 Builtin primitive type names

Builtin primitive type names are recognized semantically as predefined types
while remaining ordinary identifier-shaped names in source:

- `u8`, `u16`, `u32`, `u64`, `usize`
- `i8`, `i16`, `i32`, `i64`, `isize`
- `f32`, `f64`
- `bool`, `char`, `string`, `void`

## 8. Foreign Declarations

### 8.1 Extern Block

```ebnf
extern_block       = { attribute }, "extern", identifier, "{",
                     { extern_member },
                     "}" ;
```

### 8.2 Foreign Function Declaration

```ebnf
extern_member      = { attribute },
                     "fn", identifier, [ "=", identifier ],
                     "(", [ extern_param_list ], ")",
                     [ return_type ],
                     ";" ;
```

### 8.3 Foreign Parameter Forms

```ebnf
extern_param_list  = extern_param, { ",", extern_param } ;

extern_param       = labeled_param ;
```

### 8.4 Supported Foreign Type Surface (Current Parser)

```ebnf
type         = "void"
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

## 9. Attributes and Macro-Like Forms

### 9.1 Attributes

```ebnf
attribute = "@", identifier, [ macro_args ] ;
```

### 9.2 Macro Argument Forms

```ebnf
macro_args = "(", [ argument_list ], ")"
           | block
           ;
```

### 9.3 Attribute placement and macro interpretation

```ebnf
macro_expr      = "@", identifier, [ macro_args ] ;
macro_expr_stmt = macro_expr, ";" ;
```

Context rules:

- Declaration prefixes use this order: outer doc comments first, then attributes,
  then the declaration head.
- Attribute placement currently includes:
  - top-level items
  - function and initializer declarations
  - struct/enum/impl/protocol members that are declaration-shaped (`fn`, `init`)
  - struct fields
  - enum variants / enum cases
  - extern members
  - protocol members
- Outer doc comments may precede and attach to the same declaration positions.
- In attribute slots before declarations/items, `@name`, `@name(...)`, and `@name { ... }` are parsed as `attribute`.
- In expression position, the same surface syntax is parsed as `macro_expr`.
- In statement position, macro invocation is parsed through normal expression-statement rules and therefore requires `;`.
- Attributes are not allowed on ordinary statements, patterns, or arbitrary non-macro expressions.
- Ordinary comments remain trivia and are not attached as docs.

### 9.4 Context examples

```text
@call(.C)

extern libSystem {
    fn strlen(_ s: *void) -> usize;
}

let s = @format("value \(x)");

@log("hello");
```

## 10. Patterns

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

## 11. Blocks and Statements

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

## 12. Clause Lists and Control Statements

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

## 13. Expressions

### 13.1 Expression precedence overview

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

### 13.2 Operators currently assumed by the grammar

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

### 13.3 Grammar by precedence level

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

### 13.4 Postfix expressions

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

### 13.5 Primary expressions

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

### 13.6 Spread expressions (literal contexts only)

```ebnf
spread_expr = "..", expr ;
```

`spread_expr` is currently modeled in AST but not part of the parser's array/struct literal entry grammar in this parser stage.

### 13.7 Array literals

```ebnf
array_literal      = "[", [ array_element_list ], "]" ;
array_element_list = expr, { ",", expr } ;
```

Examples:

- `[]`
- `[1, 2, 3]`

### 13.8 Struct literals

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

### 13.9 If and match expressions

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

### 13.10 Closures

```ebnf
closure_expr       = "{", [ closure_signature ], closure_body, "}" ;

closure_signature  = closure_param_list, "in" ;
closure_param_list = closure_param, { ",", closure_param } ;
closure_param      = identifier
                   | identifier, ":", type
                   ;

closure_body       = { stmt }, [ expr ] ;
```

Examples:

- `{ print($0) }`
- `{ p1 in print(p1) }`
- `{ p1: string in print(p1) }`

Semantic rule:

- `$0`, `$1`, etc. are only valid when there is no explicit closure parameter list
- In expression position, `{ ... }` parses as closure syntax. Generic block
  expressions are only produced in explicit grammar contexts (for example
  `if ... else { ... }` branches).

## 14. Protocol declarations

Protocols combine trait-like requirements with protocol-oriented surface syntax.

```ebnf
protocol_decl        = modifier_list, "protocol", identifier, [ generic_params ],
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

protocol_init_req    = modifier_list, "init", [ init_kind ],
                       "(", [ protocol_param_list ], ")",
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
4. `impl Protocol for Type { ... }` is the intended conformance form, even if the exact impl-conformance grammar may be refined later.
5. Property requirements are declaration-only contracts; storage is not part of protocol syntax.

## 15. Semantic Notes (Non-EBNF)

- Function aliasing in foreign declarations uses `fn local = native(...)`.
- In foreign lowering, function-level call convention overrides block-level.
- When no foreign call-convention attribute is present, default is C.
- Grammar here specifies syntax shape; runtime ABI truth is validated separately.

## 16. Example Fragment

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
