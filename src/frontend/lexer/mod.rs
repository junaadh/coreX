//! Canonical in-code lexer specification for `coreX`.
//!
//! This module defines the token surface and lexical policy used by the
//! frontend. It intentionally does not implement a full lexer yet; the goal is
//! to keep lexer rules explicit, versioned with code, and ready for parser
//! integration.
//!
//! ## Responsibilities
//! - split UTF-8 source into a token stream
//! - classify identifiers vs keywords
//! - classify comments and string interpolation boundaries
//! - provide span-aware tokens for diagnostics/parser recovery
//! - keep numeric literal spellings source-preserving
//!
//! ## Longest-match policy
//! Lexing applies longest-match for overlapping punctuators:
//! - `..=` before `..` before `.`
//! - `::` before `:`
//! - `->` before `-`
//! - `=>` before `=`
//! - `==` before `=`
//! - `!=` before `!`
//! - `<=` before `<`
//! - `>=` before `>`
//!
//! Float/range disambiguation rule:
//! - a `.` starts a float fractional part only when followed by a digit
//! - `1.25` is a float literal
//! - `1..3` is integer + `DotDot` + integer
//! - `1..=3` is integer + `DotDotEq` + integer
//! - `.5` and `1.` are not in this literal surface
//!
//! ## Comment policy
//! Supported comment forms:
//! - line: `//`
//! - doc line: `///`
//! - inner doc line: `//!`
//! - block: `/* ... */`
//! - doc block: `/** ... */`
//! - inner doc block: `/*! ... */`
//!
//! Block comments close with normal `*/`.
//!
//! ## Primitive type names
//! Builtin primitive type names (`u8`, `u16`, `u32`, `u64`, `usize`, `i8`,
//! `i16`, `i32`, `i64`, `isize`, `f32`, `f64`, `bool`, `char`, `string`,
//! `void`) are lexed as ordinary identifiers. Primitive/builtin recognition is
//! a later semantic step, not a dedicated keyword-token classification.
//!
//! ## Numeric literal surface
//! Integer literal spellings include:
//! - decimal (`123`)
//! - hex (`0x7A`)
//! - octal explicit (`0o65`)
//! - octal legacy leading zero (`044`)
//! - optional int suffix (`87u8`, `87_u8`)
//! - `_` separators
//!
//! Float literal spellings include:
//! - decimal fractional (`1.25`)
//! - exponent forms (`1e9`, `1.0e-3`, `2E+10`)
//! - `_` separators
//!
//! ## Trivia policy
//! Whitespace is skipped. Comments are not ordinary parser tokens by default;
//! they are classified and can later be preserved as trivia/side-channel data.
//!
//! ## Interpolation-aware strings
//! String tokenization is segmented to support parser re-entry inside
//! interpolations:
//! - `StringStart`
//! - `StringText`
//! - `InterpolationStart`
//! - normal expression tokens inside interpolation
//! - `InterpolationEnd`
//! - `StringEnd`
//!
//! Source strings are UTF-8.
//!
//! ## Parser-contextual distinctions (not lexical)
//! - `@name(...)` / `@name { ... }` starts the same token sequence; attribute
//!   vs macro expression is parser-contextual.
//! - `..` / `..=` are lexical tokens; whether `..expr` is spread or range is
//!   parser-contextual.
//! - `.` is lexical; shorthand member (`.variant`) vs member access
//!   (`value.member`) is parser-contextual.

pub mod comment;
pub mod cursor;
mod token;

pub use comment::{
    Comment, CommentError, consume_block_comment, consume_comment,
    consume_line_comment, skip_trivia, skip_whitespace,
};
pub use cursor::SourceCursor;
pub use token::{
    CommentKind, Keyword, Span, Token, TokenKind, classify_keyword,
    classify_keyword_token,
};
