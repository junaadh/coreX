use crate::frontend::diagnostics::{
    Diagnostic, DiagnosticLabel, DiagnosticLabelKind, DiagnosticSeverity,
};
use crate::frontend::source::SourceDb;

/// Plain-text renderer for frontend diagnostics using source file context.
pub struct DiagnosticRenderer<'a> {
    db: &'a SourceDb,
}

impl<'a> DiagnosticRenderer<'a> {
    /// Creates a renderer over a source database.
    #[must_use]
    pub fn new(db: &'a SourceDb) -> Self {
        Self { db }
    }

    /// Renders one diagnostic into deterministic plain ASCII text.
    #[must_use]
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "{}: {}",
            severity_text(diagnostic.severity),
            diagnostic.message
        ));

        if let Some((path, line, column)) = self.primary_location(diagnostic) {
            lines.push(format!(" --> {}:{}:{}", path, line, column));
        }

        for label in ordered_labels(diagnostic) {
            if let Some(rendered) = self.render_label(label) {
                lines.extend(rendered);
            }
        }

        for note in &diagnostic.notes {
            lines.push(format!("note: {}", note));
        }

        if let Some(help) = &diagnostic.help {
            lines.push(format!("help: {}", help));
        }

        lines.join("\n")
    }

    /// Renders diagnostics in slice order with one blank line between entries.
    #[must_use]
    pub fn render_all(&self, diagnostics: &[Diagnostic]) -> String {
        diagnostics
            .iter()
            .map(|diagnostic| self.render(diagnostic))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn primary_location(
        &self,
        diagnostic: &Diagnostic,
    ) -> Option<(String, usize, usize)> {
        diagnostic
            .labels
            .iter()
            .filter(|label| label.kind == DiagnosticLabelKind::Primary)
            .find_map(|label| {
                let file = self.db.file(label.span.file_id)?;
                let line_col = file.line_col(label.span.span.start)?;
                Some((
                    file.path().display().to_string(),
                    line_col.line + 1,
                    line_col.column + 1,
                ))
            })
    }

    fn render_label(&self, label: &DiagnosticLabel) -> Option<Vec<String>> {
        let file = self.db.file(label.span.file_id)?;
        let span = label.span.span;
        let line_col = file.line_col(span.start)?;
        let line_number = line_col.line + 1;
        let line_start = file.line_index().line_start(line_col.line)?;
        let next_line_start = file
            .line_index()
            .line_start(line_col.line + 1)
            .unwrap_or(file.len());

        let source = file.source();
        let mut line_end = next_line_start.min(source.len());
        if line_end > line_start
            && source.as_bytes().get(line_end - 1) == Some(&b'\n')
        {
            line_end -= 1;
        }
        if line_end > line_start
            && source.as_bytes().get(line_end - 1) == Some(&b'\r')
        {
            line_end -= 1;
        }

        let line_text = source.get(line_start..line_end)?;
        let marker_char = if label.kind == DiagnosticLabelKind::Primary {
            '^'
        } else {
            '-'
        };
        let marker_start = line_col.column;

        let desired_end = if span.end > span.start {
            span.end
        } else {
            span.start.saturating_add(1)
        };
        let clamped_end = desired_end.min(line_end);
        let mut marker_end = clamped_end.saturating_sub(line_start);
        if marker_end <= marker_start {
            marker_end = marker_start.saturating_add(1);
        }
        let marker_len = marker_end - marker_start;
        let marker_text: String =
            std::iter::repeat_n(marker_char, marker_len).collect();

        let mut marker_line =
            format!("  | {}{}", " ".repeat(marker_start), marker_text);
        if let Some(message) = &label.message {
            marker_line.push(' ');
            marker_line.push_str(message);
        }

        Some(vec![
            "  |".to_string(),
            format!("{} | {}", line_number, line_text),
            marker_line,
        ])
    }
}

fn severity_text(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Note => "note",
        DiagnosticSeverity::Help => "help",
    }
}

fn ordered_labels(diagnostic: &Diagnostic) -> Vec<&DiagnosticLabel> {
    let has_primary = diagnostic
        .labels
        .iter()
        .any(|label| label.kind == DiagnosticLabelKind::Primary);
    if !has_primary {
        return diagnostic.labels.iter().collect();
    }

    let mut ordered = Vec::with_capacity(diagnostic.labels.len());
    ordered.extend(
        diagnostic
            .labels
            .iter()
            .filter(|label| label.kind == DiagnosticLabelKind::Primary),
    );
    ordered.extend(
        diagnostic
            .labels
            .iter()
            .filter(|label| label.kind != DiagnosticLabelKind::Primary),
    );
    ordered
}
