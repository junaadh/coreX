/// Parsed source file paired with its originating [`crate::frontend::source::FileId`].
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub file_id: crate::frontend::source::FileId,
    pub ast: crate::frontend::ast::File,
}

/// File-aware parser failure wrapper preserving the source file id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileParseError {
    pub file_id: crate::frontend::source::FileId,
    pub error: crate::frontend::parser::ParseError,
}

/// Parse-session error that distinguishes missing files from parser failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseSessionError {
    MissingFile {
        file_id: crate::frontend::source::FileId,
    },
    Parse(crate::frontend::FileParseError),
}
