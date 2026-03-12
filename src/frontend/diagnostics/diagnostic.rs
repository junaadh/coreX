/// Diagnostic severity level used by frontend passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
    Help,
}

/// Label priority used to mark primary versus supporting spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticLabelKind {
    Primary,
    Secondary,
}

/// Span annotation attached to a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticLabel {
    pub kind: DiagnosticLabelKind,
    pub span: crate::frontend::diagnostics::FileSpan,
    pub message: Option<String>,
}

impl DiagnosticLabel {
    /// Creates a primary label with an owned message.
    #[must_use]
    pub fn primary(
        span: crate::frontend::diagnostics::FileSpan,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind: DiagnosticLabelKind::Primary,
            span,
            message: Some(message.into()),
        }
    }

    /// Creates a primary label without a message.
    #[must_use]
    pub fn primary_span(span: crate::frontend::diagnostics::FileSpan) -> Self {
        Self {
            kind: DiagnosticLabelKind::Primary,
            span,
            message: None,
        }
    }

    /// Creates a secondary label with an owned message.
    #[must_use]
    pub fn secondary(
        span: crate::frontend::diagnostics::FileSpan,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind: DiagnosticLabelKind::Secondary,
            span,
            message: Some(message.into()),
        }
    }

    /// Creates a secondary label without a message.
    #[must_use]
    pub fn secondary_span(
        span: crate::frontend::diagnostics::FileSpan,
    ) -> Self {
        Self {
            kind: DiagnosticLabelKind::Secondary,
            span,
            message: None,
        }
    }
}

/// Top-level frontend diagnostic payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub labels: Vec<DiagnosticLabel>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    /// Creates a new diagnostic with no labels, notes, or help text.
    #[must_use]
    pub fn new(
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            help: None,
        }
    }

    /// Creates an error diagnostic.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Error, message)
    }

    /// Creates a warning diagnostic.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Warning, message)
    }

    /// Creates a note diagnostic.
    #[must_use]
    pub fn note(message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Note, message)
    }

    /// Creates a help-severity diagnostic.
    #[must_use]
    pub fn help_diag(message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Help, message)
    }

    /// Appends one label to this diagnostic.
    #[must_use]
    pub fn with_label(mut self, label: DiagnosticLabel) -> Self {
        self.labels.push(label);
        self
    }

    /// Appends labels to this diagnostic in iterator order.
    #[must_use]
    pub fn with_labels(
        mut self,
        labels: impl IntoIterator<Item = DiagnosticLabel>,
    ) -> Self {
        self.labels.extend(labels);
        self
    }

    /// Appends one note to this diagnostic.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Appends notes to this diagnostic in iterator order.
    #[must_use]
    pub fn with_notes(
        mut self,
        notes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.notes.extend(notes.into_iter().map(Into::into));
        self
    }

    /// Sets or replaces help text for this diagnostic.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}
