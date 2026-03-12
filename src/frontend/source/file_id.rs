/// Stable identifier for a source file stored in [`super::SourceDb`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(u32);

impl FileId {
    /// Creates a file id from its raw integer value.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw integer value of this file id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}
