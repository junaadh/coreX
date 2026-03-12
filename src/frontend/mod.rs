//! Source-oriented frontend compilation modules.
//!
//! This namespace holds syntax-level compilation structures that operate on
//! source text before semantic analysis, including lexing and later parser/AST
//! layers.

pub mod ast;
pub mod lexer;
pub mod parse_session;
pub mod parser;
pub mod parsed_file;
pub mod source;

pub use parsed_file::{FileParseError, ParseSessionError, ParsedFile};
pub use parse_session::ParseSession;
