/// Zero-based line/column pair resolved from a byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: usize,
    pub column: usize,
}

/// Source line index built from byte offsets.
#[derive(Debug, Clone)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    source_len: usize,
}

impl LineIndex {
    /// Builds a line index for the given source.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];

        for (idx, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(idx + 1);
            }
        }

        Self {
            line_starts,
            source_len: source.len(),
        }
    }

    /// Returns the number of indexed lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Returns the byte offset where the given line starts.
    #[must_use]
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.line_starts.get(line).copied()
    }

    /// Returns all line start byte offsets.
    #[must_use]
    pub fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }

    /// Maps a byte offset to a zero-based line/column pair.
    #[must_use]
    pub fn line_col(&self, offset: usize) -> Option<LineCol> {
        if offset > self.source_len {
            return None;
        }

        let line = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(insert_idx) => insert_idx.saturating_sub(1),
        };

        let column = offset - self.line_starts[line];
        Some(LineCol { line, column })
    }
}
