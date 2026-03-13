use crate::frontend::ast::{Item, Span};
use crate::frontend::diagnostics::{Diagnostic, DiagnosticLabel, FileSpan};
use crate::frontend::parser::parse_source_file_from_source_file_with_recovery;
use crate::frontend::resolver::ResolveError;
use crate::frontend::source::{FileId, SourceDb};

/// Converts a scope-resolution error into a file-aware frontend diagnostic.
#[must_use]
pub fn diagnostic_from_resolve_error(
    db: &SourceDb,
    error: &ResolveError,
) -> Diagnostic {
    match error {
        ResolveError::MissingRootFile { expected_path } => Diagnostic::error(
            "missing root scope file",
        )
        .with_note(format!("expected path: {}", expected_path.display())),
        ResolveError::MissingDeclaredScope {
            parent_file_id,
            declared_name,
            candidate_file,
            candidate_dir_file,
            ..
        } => {
            let mut diagnostic = Diagnostic::error("missing declared scope");
            if let Some(span) =
                find_scope_decl_span(db, *parent_file_id, declared_name)
                    .or_else(|| file_start_span(db, *parent_file_id))
            {
                diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
                    span,
                    format!(
                        "declared scope `{declared_name}` has no matching file"
                    ),
                ));
            }
            diagnostic
                .with_note(format!(
                    "probed candidate: {}",
                    candidate_file.display()
                ))
                .with_note(format!(
                    "probed candidate: {}",
                    candidate_dir_file.display()
                ))
        }
        ResolveError::AmbiguousDeclaredScope {
            parent_file_id,
            declared_name,
            file_candidate,
            dir_candidate,
            ..
        } => {
            let mut diagnostic = Diagnostic::error("ambiguous declared scope");
            if let Some(span) =
                find_scope_decl_span(db, *parent_file_id, declared_name)
                    .or_else(|| file_start_span(db, *parent_file_id))
            {
                diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
                    span,
                    format!(
                        "declared scope `{declared_name}` matches more than one file"
                    ),
                ));
            }
            diagnostic
                .with_note(format!(
                    "matching candidate: {}",
                    file_candidate.display()
                ))
                .with_note(format!(
                    "matching candidate: {}",
                    dir_candidate.display()
                ))
        }
        ResolveError::ScopeCycle { cycle } => {
            let mut diagnostic = Diagnostic::error("scope cycle detected");
            if let Some(file_id) = cycle.first().copied()
                && let Some(span) = file_start_span(db, file_id)
            {
                diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
                    span,
                    "cycle reaches this scope file",
                ));
            }

            let cycle_text = cycle
                .iter()
                .map(|file_id| render_cycle_entry(db, *file_id))
                .collect::<Vec<_>>()
                .join(" -> ");
            diagnostic.with_note(format!("cycle: {cycle_text}"))
        }
        ResolveError::NonUtf8Path => {
            Diagnostic::error("non-utf8 path is not supported")
        }
    }
}

fn find_scope_decl_span(
    db: &SourceDb,
    file_id: FileId,
    declared_name: &str,
) -> Option<FileSpan> {
    let file = db.file(file_id)?;
    let parsed = parse_source_file_from_source_file_with_recovery(file).ok()?;
    for item in &parsed.ast.items {
        let Item::Scope(scope_decl) = &item.node else {
            continue;
        };
        if scope_decl.node.name == declared_name {
            return Some(FileSpan::new(file_id, scope_decl.span));
        }
    }
    None
}

fn file_start_span(db: &SourceDb, file_id: FileId) -> Option<FileSpan> {
    let file = db.file(file_id)?;
    let end = usize::from(!file.is_empty());
    Some(FileSpan::new(file_id, Span::new(0, end)))
}

fn render_cycle_entry(db: &SourceDb, file_id: FileId) -> String {
    match db.file(file_id) {
        Some(file) => format!("{} ({})", file_id.raw(), file.path().display()),
        None => file_id.raw().to_string(),
    }
}
