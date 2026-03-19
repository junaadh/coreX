/// Multi-file frontend parse orchestrator over an owned source database.
#[derive(Debug)]
pub struct ParseSession {
    db: crate::frontend::source::SourceDb,
}

impl ParseSession {
    /// Creates a new parse session from an existing source database.
    #[must_use]
    pub fn new(db: crate::frontend::source::SourceDb) -> Self {
        Self { db }
    }

    /// Returns the session's source database.
    #[must_use]
    pub fn db(&self) -> &crate::frontend::source::SourceDb {
        &self.db
    }

    /// Consumes the session and returns the owned source database.
    #[must_use]
    pub fn into_db(self) -> crate::frontend::source::SourceDb {
        self.db
    }

    /// Parses a single file by id.
    ///
    /// # Errors
    ///
    /// Returns `ParseSessionError::MissingFile` when `file_id` is unknown, or
    /// `ParseSessionError::Parse` when lexing/parsing fails.
    pub fn parse_file(
        &self,
        file_id: crate::frontend::source::FileId,
    ) -> Result<crate::frontend::ParsedFile, crate::frontend::ParseSessionError>
    {
        let file = self.db.file(file_id).ok_or(
            crate::frontend::ParseSessionError::MissingFile { file_id },
        )?;

        crate::frontend::parser::parse_source_file_from_source_file(file)
            .map_err(|error| {
                crate::frontend::ParseSessionError::Parse(
                    crate::frontend::FileParseError { file_id, error },
                )
            })
    }

    /// Parses and macro-expands a single file by id.
    ///
    /// # Errors
    ///
    /// Returns `ParseSessionError::MissingFile` when `file_id` is unknown, or
    /// `ParseSessionError::Parse` when lexing/parsing fails.
    pub fn parse_file_with_expansion(
        &self,
        file_id: crate::frontend::source::FileId,
    ) -> Result<crate::frontend::ExpandedFile, crate::frontend::ParseSessionError>
    {
        let parsed = self.parse_file(file_id)?;
        let expanded = crate::frontend::expand_parsed_files(
            &self.db,
            std::slice::from_ref(&parsed),
            crate::frontend::ExpansionOptions::default(),
        );
        Ok(expanded.into_iter().next().unwrap())
    }

    /// Parses a single file by id with conservative recovery and diagnostics.
    ///
    /// # Errors
    ///
    /// Returns `ParseSessionError::MissingFile` when `file_id` is unknown, or
    /// `ParseSessionError::Parse` when lexing fails before recovery parsing.
    pub fn parse_file_with_recovery(
        &self,
        file_id: crate::frontend::source::FileId,
    ) -> Result<crate::frontend::ParsedFile, crate::frontend::ParseSessionError>
    {
        let file = self.db.file(file_id).ok_or(
            crate::frontend::ParseSessionError::MissingFile { file_id },
        )?;

        crate::frontend::parser::parse_source_file_from_source_file_with_recovery(
            file,
        )
        .map_err(|error| {
            crate::frontend::ParseSessionError::Parse(
                crate::frontend::FileParseError { file_id, error },
            )
        })
    }

    /// Parses and macro-expands a single file by id with conservative
    /// recovery and diagnostics.
    ///
    /// # Errors
    ///
    /// Returns `ParseSessionError::MissingFile` when `file_id` is unknown, or
    /// `ParseSessionError::Parse` when lexing fails before recovery parsing.
    pub fn parse_file_with_recovery_and_expansion(
        &self,
        file_id: crate::frontend::source::FileId,
    ) -> Result<crate::frontend::ExpandedFile, crate::frontend::ParseSessionError>
    {
        let parsed = self.parse_file_with_recovery(file_id)?;
        let expanded = crate::frontend::expand_parsed_files(
            &self.db,
            std::slice::from_ref(&parsed),
            crate::frontend::ExpansionOptions::default(),
        );
        Ok(expanded.into_iter().next().unwrap())
    }

    /// Parses all files in insertion order.
    #[must_use]
    pub fn parse_all_files(
        &self,
    ) -> Vec<Result<crate::frontend::ParsedFile, crate::frontend::FileParseError>>
    {
        self.db
            .files()
            .iter()
            .map(|file| {
                let file_id = file.id();
                crate::frontend::parser::parse_source_file_from_source_file(
                    file,
                )
                .map_err(|error| {
                    crate::frontend::FileParseError { file_id, error }
                })
            })
            .collect()
    }

    /// Parses and macro-expands all files in insertion order.
    #[must_use]
    pub fn parse_all_files_with_expansion(
        &self,
    ) -> Vec<Result<crate::frontend::ExpandedFile, crate::frontend::FileParseError>>
    {
        self.parse_all_files_and_expand(false)
    }

    /// Parses all files in insertion order with conservative recovery.
    #[must_use]
    pub fn parse_all_files_with_recovery(
        &self,
    ) -> Vec<Result<crate::frontend::ParsedFile, crate::frontend::FileParseError>>
    {
        self.db
            .files()
            .iter()
            .map(|file| {
                let file_id = file.id();
                crate::frontend::parser::parse_source_file_from_source_file_with_recovery(
                    file,
                )
                .map_err(|error| crate::frontend::FileParseError {
                    file_id,
                    error,
                })
            })
            .collect()
    }

    /// Parses and macro-expands all files in insertion order with
    /// conservative recovery.
    #[must_use]
    pub fn parse_all_files_with_recovery_and_expansion(
        &self,
    ) -> Vec<Result<crate::frontend::ExpandedFile, crate::frontend::FileParseError>>
    {
        self.parse_all_files_and_expand(true)
    }

    fn parse_all_files_and_expand(
        &self,
        with_recovery: bool,
    ) -> Vec<Result<crate::frontend::ExpandedFile, crate::frontend::FileParseError>>
    {
        let parsed_results = if with_recovery {
            self.parse_all_files_with_recovery()
        } else {
            self.parse_all_files()
        };

        let parsed_files = parsed_results
            .iter()
            .filter_map(|result| result.as_ref().ok().cloned())
            .collect::<Vec<_>>();
        let expanded_files = crate::frontend::expand_parsed_files(
            &self.db,
            &parsed_files,
            crate::frontend::ExpansionOptions::default(),
        );
        let mut expanded_by_file_id = expanded_files
            .into_iter()
            .map(|expanded| (expanded.file_id, expanded))
            .collect::<std::collections::BTreeMap<_, _>>();

        parsed_results
            .into_iter()
            .map(|result| match result {
                Ok(parsed) => Ok(expanded_by_file_id
                    .remove(&parsed.file_id)
                    .expect("expanded file should exist for each parsed file")),
                Err(error) => Err(error),
            })
            .collect()
    }
}
