/// Source span paired with a stable source file identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileSpan {
    pub file_id: crate::frontend::source::FileId,
    pub span: crate::frontend::ast::Span,
}

impl FileSpan {
    /// Creates a new file-aware span.
    #[must_use]
    pub const fn new(
        file_id: crate::frontend::source::FileId,
        span: crate::frontend::ast::Span,
    ) -> Self {
        Self { file_id, span }
    }
}
