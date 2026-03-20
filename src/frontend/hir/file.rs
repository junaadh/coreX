use super::ids::HirItemId;
use crate::frontend::source::FileId;

/// Root HIR artifact for one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFile {
    pub file_id: FileId,
    pub root_items: Vec<HirItemId>,
}
