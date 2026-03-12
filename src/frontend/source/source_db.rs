use super::{FileId, SourceFile};
use std::path::PathBuf;

/// In-memory database of source files keyed by stable [`FileId`].
#[derive(Debug, Default)]
pub struct SourceDb {
    files: Vec<SourceFile>,
}

impl SourceDb {
    /// Creates an empty source database.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file and returns its stable id.
    pub fn add_file(
        &mut self,
        path: impl Into<PathBuf>,
        source: impl Into<String>,
    ) -> FileId {
        let id = FileId::new(self.files.len() as u32);
        let file = SourceFile::new(id, path.into(), source.into());
        self.files.push(file);
        id
    }

    /// Returns a file by id.
    #[must_use]
    pub fn file(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.raw() as usize)
    }

    /// Returns all files in insertion order.
    #[must_use]
    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    /// Returns file count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Returns true when no files are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}
