//! Source-oriented frontend compilation modules.
//!
//! This namespace holds syntax-level compilation structures that operate on
//! source text before semantic analysis, including lexing and later parser/AST
//! layers.

pub mod ast;
pub mod diagnostics;
pub mod lexer;
pub mod parse_session;
pub mod parsed_file;
pub mod parser;
pub mod resolver;
pub mod source;

pub use diagnostics::{
    Diagnostic, DiagnosticLabel, DiagnosticLabelKind, DiagnosticRenderer,
    DiagnosticSeverity, DiagnosticsBag, FileSpan,
    diagnostic_from_file_parse_error, diagnostic_from_parse_error,
};
pub use parse_session::ParseSession;
pub use parsed_file::{FileParseError, ParseSessionError, ParsedFile};
pub use resolver::{
    ImportBindingKind, ImportResolveError, ImportResolver, ResolveError,
    ResolvedImportBinding, ResolvedImports, ResolvedScope, ResolvedScopeKind,
    ScopeGraph, ScopeResolver, ScopeSymbols, Symbol, SymbolKind,
    resolve_project_imports, resolve_project_scopes,
};
