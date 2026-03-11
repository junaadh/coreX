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

### 2.4 Statement Termination

```ebnf
terminated_stmt = stmt, ";" ;
```

Simple statements require `;`.
Final expression elision rules are semantic and not fully fixed here.

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
type = "void"
     | "i32"
     | "usize"
     | "*", "const", "void"
     | "*", "mut", "void"
     ;
```

Examples:

```text
@call(.C)
extern libSystem {
    fn strlen(_ s: *const void) -> usize;
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
attribute = "@", identifier, macro_args ;
```

### 9.2 Macro Argument Forms

```ebnf
macro_args = "(", [ argument_list ], ")"
           | block
           ;
```

### 9.3 Context-sensitive macro and attribute interpretation

```ebnf
macro_expr      = "@", identifier, macro_args ;
macro_expr_stmt = macro_expr, ";" ;
```

Context rules:

- In attribute slots before declarations/items, `@name(...)` and `@name { ... }` are parsed as `attribute`.
- In expression position, the same surface syntax is parsed as `macro_expr`.
- In statement position, macro invocation is parsed through normal expression-statement rules and therefore requires `;`.

### 9.4 Context examples

```text
@call(.C)

extern libSystem {
    fn strlen(_ s: *const void) -> usize;
}

let s = @format("value \(x)");

@log("hello");
```

## 13. Expressions

### 13.1 Expression precedence overview

From lowest precedence to highest precedence:

| Level | Category | Associativity |
|---|---|---|
| 1 | Assignment | right |
| 2 | Range (`..`, `..=`) | left |
| 3 | Logical OR | left |
| 4 | Logical AND | left |
| 5 | Equality | left |
| 6 | Comparison | left |
| 7 | Additive (`+`, `-`) | left |
| 8 | Multiplicative (`*`, `/`, `%`) | left |
| 9 | Prefix unary (`try`, `!`, `-`, future `&`) | right |
| 10 | Postfix (`()`, `[]`, `.`, `::`, trailing closure) | left |
| 11 | Primary | n/a |

This table is a frontend parsing contract. Operator overloading is intentionally not part of this draft.

### 13.2 Operators currently assumed by the grammar

```ebnf
assignment_op      = "=" ;
range_op           = ".." | "..=" ;
logical_or_op      = "||" ;
logical_and_op     = "&&" ;
equality_op        = "==" | "!=" ;
comparison_op      = "<" | "<=" | ">" | ">=" ;
additive_op        = "+" | "-" ;
multiplicative_op  = "*" | "/" | "%" ;
prefix_op          = "!" | "-" ;
```

Additional operators may be added later, but parser precedence should be extended explicitly rather than inferred.

### 13.3 Grammar by precedence layer

```ebnf
expr                = assignment_expr ;

assignment_expr     = range_expr
                    | postfix_expr, assignment_op, assignment_expr
                    ;

range_expr          = logical_or_expr
                    | logical_or_expr, "..", logical_or_expr
                    | logical_or_expr, "..=", logical_or_expr
                    | logical_or_expr, ".."
                    | "..", logical_or_expr
                    ;

logical_or_expr     = logical_and_expr,
                      { logical_or_op, logical_and_expr } ;

logical_and_expr    = equality_expr,
                      { logical_and_op, equality_expr } ;

equality_expr       = comparison_expr,
                      { equality_op, comparison_expr } ;

comparison_expr     = additive_expr,
                      { comparison_op, additive_expr } ;

additive_expr       = multiplicative_expr,
                      { additive_op, multiplicative_expr } ;

multiplicative_expr = unary_expr,
                      { multiplicative_op, unary_expr } ;

unary_expr          = "try", unary_expr
                    | prefix_op, unary_expr
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
                        | namespace_suffix
                        | call_suffix
                        | index_suffix
                        | trailing_closure_suffix
                        ;

member_suffix           = ".", identifier ;
namespace_suffix        = "::", identifier, [ turbofish_suffix ] ;
call_suffix             = "(", [ argument_list ], ")" ;
index_suffix            = "[", expr, "]" ;
trailing_closure_suffix = closure_expr ;

argument_list           = argument, { ",", argument } ;
argument                = expr
                        | identifier, ":", expr
                        ;

turbofish_suffix        = "<", type_list, ">" ;
```

Semantic restriction:

- only one trailing closure is supported for now
- trailing closure attaches to the immediately preceding callable postfix expression
- `::` binds namespace/type/static lookup, while `.` binds value/member access and qualified enum-case syntax

Examples:

- `foo()`
- `foo(x: 1, y: 2)`
- `foo { $0 }`
- `foo(bar) { $0 }`
- `xs[0]`
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

`spread_expr` is only valid in literal contexts that explicitly allow spread entries.

### 13.7 Array literals

```ebnf
array_literal      = "[", [ array_element_list ], "]" ;
array_element_list = array_element, { ",", array_element } ;
array_element      = expr | spread_expr ;
```

Examples:

- `[]`
- `[1, 2, 3]`
- `[1, 2, ..xs]`

Empty array literals require contextual type inference if element type cannot be inferred locally.

### 13.8 Struct literals

```ebnf
struct_literal         = type_expr, "{", [ struct_field_init_list ], [ "," ], "}" ;

type_expr              = identifier | "Self" ;

struct_field_init_list = struct_field_init, { ",", struct_field_init } ;
struct_field_init      = identifier
                       | identifier, ":", expr
                       | spread_expr
                       ;
```

Examples:

- `Self { inner }`
- `Type { inner: 1, other }`
- `Foo { x: 1, ..rest }`

Shorthand form means field name and local variable name match.

### 13.9 Closures

```ebnf
closure_expr       = "{", [ closure_signature ], closure_body, "}" ;

closure_signature  = closure_param_list, "in" ;
closure_param_list = closure_param, { ",", closure_param } ;
closure_param      = identifier
                   | identifier, ":", type
                   ;

closure_body       = { statement }, [ expr ] ;
```

Examples:

- `{ print($0) }`
- `{ p1 in print(p1) }`
- `{ p1: string in print(p1) }`

Semantic rule:

- `$0`, `$1`, etc. are only valid when there is no explicit closure parameter list

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
    fn strlen(s: *const void) -> usize;
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
