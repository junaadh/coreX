/// Ordered collection of diagnostics accumulated during frontend passes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticsBag {
    diagnostics: Vec<crate::frontend::diagnostics::Diagnostic>,
}

impl DiagnosticsBag {
    /// Creates an empty diagnostics bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes one diagnostic at the end of the bag.
    pub fn push(
        &mut self,
        diagnostic: crate::frontend::diagnostics::Diagnostic,
    ) {
        self.diagnostics.push(diagnostic);
    }

    /// Appends diagnostics in iterator order.
    pub fn extend(
        &mut self,
        diagnostics: impl IntoIterator<
            Item = crate::frontend::diagnostics::Diagnostic,
        >,
    ) {
        self.diagnostics.extend(diagnostics);
    }

    /// Returns true if the bag has no diagnostics.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Returns the number of diagnostics in the bag.
    #[must_use]
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Returns a shared slice view of diagnostics.
    #[must_use]
    pub fn as_slice(&self) -> &[crate::frontend::diagnostics::Diagnostic] {
        &self.diagnostics
    }

    /// Consumes the bag and returns the owned diagnostics vector.
    #[must_use]
    pub fn into_vec(self) -> Vec<crate::frontend::diagnostics::Diagnostic> {
        self.diagnostics
    }
}
