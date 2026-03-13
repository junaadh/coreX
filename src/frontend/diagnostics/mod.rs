//! Shared frontend diagnostics model, intentionally separate from rendering.

mod bag;
mod diagnostic;
mod file_span;
mod imports;
mod parse;
mod render;
mod resolve;

pub use bag::DiagnosticsBag;
pub use diagnostic::{
    Diagnostic, DiagnosticLabel, DiagnosticLabelKind, DiagnosticSeverity,
};
pub use file_span::FileSpan;
pub use imports::diagnostic_from_import_resolve_error;
pub use parse::{
    diagnostic_from_file_parse_error, diagnostic_from_parse_error,
};
pub use render::DiagnosticRenderer;
pub use resolve::diagnostic_from_resolve_error;
