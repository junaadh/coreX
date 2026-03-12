//! Handwritten source parser for `coreX`.
//!
//! This module consumes lexer tokens into source AST nodes.
//! It owns parser cursor utilities, declaration parsing, block/statement
//! parsing, and expression parsing over the current frontend grammar surface.
//! Outer doc comments are collected from source and attached to declarations.

mod error;
mod parser;

pub use error::ParseError;
pub use parser::{Parser, parse_source_file};
