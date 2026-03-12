//! Handwritten source parser for `coreX`.
//!
//! This module consumes lexer tokens into source AST nodes.
//! It owns parser cursor utilities, declaration parsing, block/statement
//! parsing, and expression parsing over the current frontend grammar surface.
//! Outer doc comments are collected from source and attached to declarations.

mod error;
mod parser;

pub use error::ParseError;
pub use parser::{parse_source_file, parse_source_file_with_recovery};

/// Parses a full source file from a file-oriented source abstraction.
pub fn parse_source_file_from_source_file(
    file: &crate::frontend::source::SourceFile,
) -> Result<crate::frontend::ParsedFile, ParseError> {
    parser::parse_source_file_with_file_id(file.source(), file.id())
}

/// Parses a full source file and accumulates diagnostics using conservative
/// recovery.
pub fn parse_source_file_from_source_file_with_recovery(
    file: &crate::frontend::source::SourceFile,
) -> Result<crate::frontend::ParsedFile, ParseError> {
    parser::parse_source_file_with_recovery_and_file_id(
        file.source(),
        file.id(),
    )
}
