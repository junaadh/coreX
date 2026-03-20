use crate::frontend::ast::Span;
use crate::frontend::expansion::Provenance;
use crate::frontend::source::FileId;

/// Source origin metadata attached to each lowered HIR node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirOrigin {
    pub file_id: FileId,
    pub span: Span,
    pub provenance: Provenance,
}

impl HirOrigin {
    #[must_use]
    pub const fn new(
        file_id: FileId,
        span: Span,
        provenance: Provenance,
    ) -> Self {
        Self {
            file_id,
            span,
            provenance,
        }
    }

    #[must_use]
    pub fn direct_source(file_id: FileId, span: Span) -> Self {
        Self {
            file_id,
            span,
            provenance: Provenance::DirectSource { file_id, span },
        }
    }
}
