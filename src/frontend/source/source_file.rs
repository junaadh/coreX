use crate::frontend::ast::Span;

use super::{FileId, LineCol, LineIndex};
use std::path::{Path, PathBuf};

/// Parsed-input source file with precomputed line index.
#[derive(Debug, Clone)]
pub struct SourceFile {
    id: FileId,
    path: PathBuf,
    source: String,
    line_index: LineIndex,
}

impl SourceFile {
    /// Creates a source file from id, path, and source text.
    #[must_use]
    pub fn new(id: FileId, path: PathBuf, source: String) -> Self {
        let line_index = LineIndex::new(&source);
        Self {
            id,
            path,
            source,
            line_index,
        }
    }

    /// Returns this file's stable id.
    #[must_use]
    pub fn id(&self) -> FileId {
        self.id
    }

    /// Returns this file's stored path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns this file's source text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns this file's line index.
    #[must_use]
    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }

    /// Returns source length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.source.len()
    }

    /// Returns true when source text is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    /// Resolves a byte offset to zero-based line/column.
    #[must_use]
    pub fn line_col(&self, offset: usize) -> Option<LineCol> {
        self.line_index.line_col(offset)
    }

    /// Returns a safe source slice for a span.
    #[must_use]
    pub fn slice(&self, span: Span) -> Option<&str> {
        if span.start > span.end || span.end > self.source.len() {
            return None;
        }
        if !self.source.is_char_boundary(span.start)
            || !self.source.is_char_boundary(span.end)
        {
            return None;
        }

        self.source.get(span.start..span.end)
    }
}
