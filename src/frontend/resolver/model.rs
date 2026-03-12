use crate::frontend::source::FileId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedScopeKind {
    Root,
    FileBacked,
    DirectoryBacked,
    BinaryRoot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedScope {
    pub file_id: FileId,
    pub kind: ResolvedScopeKind,
    pub name: String,
    pub scope_path: Vec<String>,
    pub child_scope_ids: Vec<FileId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeGraph {
    pub root_file_id: FileId,
    pub scopes: BTreeMap<FileId, ResolvedScope>,
}

impl ScopeGraph {
    #[must_use]
    pub fn scope(&self, file_id: FileId) -> Option<&ResolvedScope> {
        self.scopes.get(&file_id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.scopes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }
}
