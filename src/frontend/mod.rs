//! Source-oriented frontend compilation modules.
//!
//! This namespace holds syntax-level compilation structures that operate on
//! source text before semantic analysis, including lexing and later parser/AST
//! layers.

pub mod ast;
pub mod lexer;
pub mod parser;
