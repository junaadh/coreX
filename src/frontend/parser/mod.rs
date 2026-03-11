//! Handwritten source parser scaffolding for `coreX`.
//!
//! This module consumes lexer tokens into source AST nodes.
//! This module currently focuses on parser architecture:
//! - token cursor helpers
//! - structured parse errors
//! - file/item entry points
//! - minimal top-level coverage to support incremental parser growth

mod error;
mod parser;

pub use error::ParseError;
pub use parser::{Parser, parse_source_file};
