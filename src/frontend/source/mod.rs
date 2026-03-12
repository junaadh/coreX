//! File-oriented source abstractions for the frontend parser layer.
//!
//! This module provides stable file ids and line/offset lookup primitives used
//! before diagnostics and LSP integration.

mod file_id;
mod line_index;
mod source_db;
mod source_file;

pub use file_id::FileId;
pub use line_index::{LineCol, LineIndex};
pub use source_db::SourceDb;
pub use source_file::SourceFile;
