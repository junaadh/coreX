use super::{FileId, SourceFile};
use std::path::PathBuf;

/// In-memory database of source files keyed by stable [`FileId`].
#[derive(Debug, Clone, Default)]
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
    ///
    /// # Panics
    /// Panics if the database grows beyond `u32::MAX` files.
    pub fn add_file(
        &mut self,
        path: impl Into<PathBuf>,
        source: impl Into<String>,
    ) -> FileId {
        let id = FileId::new(
            u32::try_from(self.files.len())
                .expect("source db file id overflow"),
        );
        let file = SourceFile::new(id, path.into(), source.into());
        self.files.push(file);
        id
    }

    /// Returns a file by id.
    #[must_use]
    pub fn file(&self, id: FileId) -> Option<&SourceFile> {
        usize::try_from(id.raw())
            .ok()
            .and_then(|index| self.files.get(index))
    }

    /// Returns a mutable file by id.
    #[must_use]
    pub fn file_mut(&mut self, id: FileId) -> Option<&mut SourceFile> {
        usize::try_from(id.raw())
            .ok()
            .and_then(|index| self.files.get_mut(index))
    }

    /// Replaces source text for an existing file id while preserving id/path.
    ///
    /// Returns `true` when the file existed and was updated.
    pub fn update_file_source(
        &mut self,
        id: FileId,
        source: impl Into<String>,
    ) -> bool {
        let Some(existing) = self.file(id) else {
            return false;
        };
        let updated = SourceFile::new(
            id,
            existing.path().to_path_buf(),
            source.into(),
        );
        let Some(slot) = self.file_mut(id) else {
            return false;
        };
        *slot = updated;
        true
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
