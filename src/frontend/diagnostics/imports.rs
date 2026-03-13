use crate::frontend::ast::{Item, Span};
use crate::frontend::diagnostics::{Diagnostic, DiagnosticLabel, FileSpan};
use crate::frontend::parser::parse_source_file_from_source_file_with_recovery;
use crate::frontend::resolver::ImportResolveError;
use crate::frontend::source::{FileId, SourceDb};

/// Converts an import-resolution error into a file-aware frontend diagnostic.
#[must_use]
pub fn diagnostic_from_import_resolve_error(
    db: &SourceDb,
    error: &ImportResolveError,
) -> Diagnostic {
    match error {
        ImportResolveError::UnknownRoot { from_file_id, root } => {
            let mut diagnostic = Diagnostic::error("unknown import root");
            if let Some(span) =
                use_item_span(db, *from_file_id, Some(root.as_str()))
                    .or_else(|| file_start_span(db, *from_file_id))
            {
                diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
                    span,
                    format!("cannot resolve import root `{root}`"),
                ));
            }
            diagnostic.with_note(
                "valid import roots are root, super, and configured target/dependency roots",
            )
        }
        ImportResolveError::UnloadedDependencyRoot { from_file_id, root } => {
            let mut diagnostic =
                Diagnostic::error("dependency root is not loaded");
            if let Some(span) =
                use_item_span(db, *from_file_id, Some(root.as_str()))
                    .or_else(|| file_start_span(db, *from_file_id))
            {
                diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
                    span,
                    format!(
                        "import root `{root}` refers to a dependency that is declared but not loaded"
                    ),
                ));
            }
            diagnostic
        }
        ImportResolveError::UnresolvedPath { from_file_id, path } => {
            let rendered_path = path.join("::");
            let mut diagnostic = Diagnostic::error("unresolved import path");
            if let Some(span) =
                use_item_span(db, *from_file_id, Some(&rendered_path))
                    .or_else(|| file_start_span(db, *from_file_id))
            {
                diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
                    span,
                    format!("cannot resolve `{rendered_path}`"),
                ));
            }
            diagnostic
        }
        ImportResolveError::InvalidSelfImport { from_file_id } => {
            let mut diagnostic = Diagnostic::error("invalid self import");
            if let Some(span) = use_item_span(db, *from_file_id, Some("self"))
                .or_else(|| file_start_span(db, *from_file_id))
            {
                diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
                    span,
                    "self is only valid inside grouped imports",
                ));
            }
            diagnostic
        }
        ImportResolveError::InvalidGlobTarget { from_file_id, path } => {
            let rendered_path = path.join("::");
            let mut diagnostic =
                Diagnostic::error("invalid glob import target");
            if let Some(span) =
                use_item_span(db, *from_file_id, Some(&rendered_path))
                    .or_else(|| file_start_span(db, *from_file_id))
            {
                diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
                    span,
                    "glob import target is not a scope",
                ));
            }
            diagnostic
        }
        ImportResolveError::DuplicateBinding {
            file_id,
            binding_name,
        } => {
            let mut diagnostic =
                Diagnostic::error("duplicate imported binding");
            if let Some(span) =
                use_item_span(db, *file_id, Some(binding_name.as_str()))
                    .or_else(|| file_start_span(db, *file_id))
            {
                diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
                    span,
                    format!(
                        "import introduces duplicate binding `{binding_name}`"
                    ),
                ));
            }
            diagnostic.with_note(
                "another import in this scope already introduced this name",
            )
        }
    }
}

fn use_item_span(
    db: &SourceDb,
    file_id: FileId,
    needle: Option<&str>,
) -> Option<FileSpan> {
    let file = db.file(file_id)?;
    let parsed = parse_source_file_from_source_file_with_recovery(file).ok()?;
    let mut first_use_span = None;

    for item in &parsed.ast.items {
        let Item::Use(_) = &item.node else {
            continue;
        };

        first_use_span.get_or_insert(item.span);

        if let Some(needle) = needle
            && file
                .slice(item.span)
                .is_some_and(|snippet| snippet.contains(needle))
        {
            return Some(FileSpan::new(file_id, item.span));
        }
    }

    first_use_span.map(|span| FileSpan::new(file_id, span))
}

fn file_start_span(db: &SourceDb, file_id: FileId) -> Option<FileSpan> {
    let file = db.file(file_id)?;
    let end = usize::from(!file.is_empty());
    Some(FileSpan::new(file_id, Span::new(0, end)))
}
