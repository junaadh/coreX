//! Shared frontend diagnostics model, intentionally separate from rendering.

mod bag;
mod diagnostic;
mod file_span;
mod parse;
mod render;

pub use bag::DiagnosticsBag;
pub use diagnostic::{
    Diagnostic, DiagnosticLabel, DiagnosticLabelKind, DiagnosticSeverity,
};
pub use file_span::FileSpan;
pub use parse::{
    diagnostic_from_file_parse_error, diagnostic_from_parse_error,
};
pub use render::DiagnosticRenderer;
