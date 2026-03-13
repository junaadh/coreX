//! Shared frontend diagnostics model, intentionally separate from rendering.

mod bag;
mod diagnostic;
mod file_span;
mod imports;
mod parse;
mod render;
mod resolve;
mod semantic;

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
pub use semantic::{
    diagnostic_from_control_flow_issue, diagnostic_from_expr_check_issue,
    diagnostic_from_stmt_check_issue, diagnostics_from_semantic_checks,
};
