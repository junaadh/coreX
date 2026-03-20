use crate::frontend::parser::parse_source_file_from_source_file_with_recovery;
use crate::frontend::resolver::{
    ItemId, NamedImportRoot, ResolvedImports, ResolvedScopeKind, ScopeGraph,
    ScopeSymbols,
};
use crate::frontend::semantic::{
    DefinitionLocation, ExternalSemanticLookup, SemanticAnalysis,
    SemanticHirInput,
};
use crate::frontend::source::{FileId, SourceDb, SourceFile};
use crate::frontend::{
    DesugaredFile, DiagnosticsBag, ExpandedFile, ExpansionOptions,
    FileParseError, MacroDefinitionIndex, MacroScopeTable, ParseSessionError,
    ParsedFile, desugar_file, expand_parsed_files_with_index_and_scope,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Shared frontend pipeline context for parse/expand/desugar reuse.
///
/// The context owns source files and all derived frontend artifacts and keeps
/// deterministic lookup maps for file/path addressing used by CLI and LSP
/// flows.
#[derive(Debug, Default)]
pub struct FrontendContext {
    db: SourceDb,
    ordered_file_ids: Vec<FileId>,
    path_by_file_id: BTreeMap<FileId, PathBuf>,
    file_id_by_path: BTreeMap<PathBuf, FileId>,
    parsed_by_file_id: BTreeMap<FileId, ParsedFile>,
    expanded_by_file_id: BTreeMap<FileId, ExpandedFile>,
    desugared_by_file_id: BTreeMap<FileId, DesugaredFile>,
    expanded_options_by_file_id: BTreeMap<FileId, ExpansionOptions>,
    macro_definition_index: Option<MacroDefinitionIndex>,
    macro_scope_table: Option<MacroScopeTable>,
    root_kind_by_file_id: BTreeMap<FileId, ResolvedScopeKind>,
    dependency_named_roots: BTreeMap<String, NamedImportRoot>,
    current_library_import_root: Option<String>,
    library_root_file_id: Option<FileId>,
    hir_by_entry_file_id: BTreeMap<FileId, SemanticHirInput>,
    scope_graph_by_entry_file_id: BTreeMap<FileId, ScopeGraph>,
    scope_symbols_by_entry_file_id: BTreeMap<FileId, BTreeMap<FileId, ScopeSymbols>>,
    imports_by_entry_file_id: BTreeMap<FileId, BTreeMap<FileId, ResolvedImports>>,
    external_lookup_by_entry_file_id: BTreeMap<FileId, ExternalSemanticLookup>,
    item_definitions_by_entry_file_id:
        BTreeMap<FileId, BTreeMap<ItemId, DefinitionLocation>>,
    semantic_by_entry_file_id: BTreeMap<FileId, SemanticAnalysis>,
    analysis_diagnostics_by_entry_file_id: BTreeMap<FileId, DiagnosticsBag>,
    unresolved_entries: BTreeSet<FileId>,
}

impl FrontendContext {
    /// Creates an empty frontend context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn invalidate_hir_resolution_semantic(&mut self) {
        self.hir_by_entry_file_id.clear();
        self.scope_graph_by_entry_file_id.clear();
        self.scope_symbols_by_entry_file_id.clear();
        self.imports_by_entry_file_id.clear();
        self.external_lookup_by_entry_file_id.clear();
        self.item_definitions_by_entry_file_id.clear();
        self.semantic_by_entry_file_id.clear();
        self.analysis_diagnostics_by_entry_file_id.clear();
        self.unresolved_entries.clear();
    }

    fn invalidate_post_parse_stages(&mut self) {
        self.macro_definition_index = None;
        self.macro_scope_table = None;
        self.expanded_by_file_id.clear();
        self.expanded_options_by_file_id.clear();
        self.desugared_by_file_id.clear();
        self.invalidate_hir_resolution_semantic();
    }

    fn invalidate_file_and_dependents(&mut self, file_id: FileId) {
        self.parsed_by_file_id.remove(&file_id);
        self.invalidate_post_parse_stages();
    }

    /// Returns the owned source database.
    #[must_use]
    pub fn db(&self) -> &SourceDb {
        &self.db
    }

    /// Consumes the context and returns its owned source database.
    #[must_use]
    pub fn into_db(self) -> SourceDb {
        self.db
    }

    /// Returns file ids in insertion order.
    #[must_use]
    pub fn ordered_file_ids(&self) -> &[FileId] {
        &self.ordered_file_ids
    }

    /// Returns path lookup by file id.
    #[must_use]
    pub fn path_by_file_id(&self) -> &BTreeMap<FileId, PathBuf> {
        &self.path_by_file_id
    }

    /// Returns file-id lookup by path.
    #[must_use]
    pub fn file_id_by_path(&self) -> &BTreeMap<PathBuf, FileId> {
        &self.file_id_by_path
    }

    /// Returns a source file by id.
    #[must_use]
    pub fn file(&self, file_id: FileId) -> Option<&SourceFile> {
        self.db.file(file_id)
    }

    /// Registers a file source. Existing paths return their existing `FileId`.
    pub fn add_file(
        &mut self,
        path: impl Into<PathBuf>,
        source: impl Into<String>,
    ) -> FileId {
        let path = path.into();
        let source = source.into();
        if let Some(existing) = self.file_id_by_path.get(&path).copied() {
            let _ = self.db.update_file_source(existing, source);
            self.invalidate_file_and_dependents(existing);
            return existing;
        }

        let file_id = self.db.add_file(path.clone(), source);
        self.ordered_file_ids.push(file_id);
        self.path_by_file_id.insert(file_id, path.clone());
        self.file_id_by_path.insert(path, file_id);
        self.invalidate_post_parse_stages();
        file_id
    }

    /// Replaces source text for an existing file id while preserving `FileId`.
    ///
    /// # Errors
    ///
    /// Returns `ParseSessionError::MissingFile` when `file_id` is unknown.
    pub fn replace_file_source(
        &mut self,
        file_id: FileId,
        source: impl Into<String>,
    ) -> Result<(), ParseSessionError> {
        if !self.db.update_file_source(file_id, source) {
            return Err(ParseSessionError::MissingFile { file_id });
        }
        self.invalidate_file_and_dependents(file_id);
        Ok(())
    }

    /// Returns the file id for a path.
    #[must_use]
    pub fn file_id_for_path(&self, path: &Path) -> Option<FileId> {
        self.file_id_by_path.get(path).copied()
    }

    /// Returns the file path for an id.
    #[must_use]
    pub fn path_for_file_id(&self, file_id: FileId) -> Option<&Path> {
        self.path_by_file_id.get(&file_id).map(PathBuf::as_path)
    }

    /// Returns a parsed file if already cached.
    #[must_use]
    pub fn parsed_file(&self, file_id: FileId) -> Option<&ParsedFile> {
        self.parsed_by_file_id.get(&file_id)
    }

    /// Returns an expanded file if already cached.
    #[must_use]
    pub fn expanded_file(&self, file_id: FileId) -> Option<&ExpandedFile> {
        self.expanded_by_file_id.get(&file_id)
    }

    /// Returns a desugared file if already cached.
    #[must_use]
    pub fn desugared_file_cached(
        &self,
        file_id: FileId,
    ) -> Option<&DesugaredFile> {
        self.desugared_by_file_id.get(&file_id)
    }

    /// Returns the cached macro-definition index, if built.
    #[must_use]
    pub fn macro_definition_index(&self) -> Option<&MacroDefinitionIndex> {
        self.macro_definition_index.as_ref()
    }

    /// Returns the cached macro-scope table, if built.
    #[must_use]
    pub fn macro_scope_table(&self) -> Option<&MacroScopeTable> {
        self.macro_scope_table.as_ref()
    }

    /// Overrides root-kind classification for an entry file id.
    pub fn set_root_kind(
        &mut self,
        file_id: FileId,
        root_kind: ResolvedScopeKind,
    ) {
        self.root_kind_by_file_id.insert(file_id, root_kind);
        self.invalidate_hir_resolution_semantic();
    }

    /// Returns root-kind classification for an entry file id.
    #[must_use]
    pub fn root_kind_for_file_id(&self, file_id: FileId) -> ResolvedScopeKind {
        if let Some(kind) = self.root_kind_by_file_id.get(&file_id).copied() {
            return kind;
        }

        let Some(path) = self.path_for_file_id(file_id) else {
            return ResolvedScopeKind::Root;
        };
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("main.cx"))
        {
            ResolvedScopeKind::BinaryRoot
        } else {
            ResolvedScopeKind::Root
        }
    }

    /// Replaces dependency named-import roots used during import resolution.
    pub fn set_dependency_named_roots(
        &mut self,
        named_roots: BTreeMap<String, NamedImportRoot>,
    ) {
        self.dependency_named_roots = named_roots;
        self.invalidate_hir_resolution_semantic();
    }

    /// Returns dependency named-import roots.
    #[must_use]
    pub fn dependency_named_roots(&self) -> &BTreeMap<String, NamedImportRoot> {
        &self.dependency_named_roots
    }

    /// Configures current-library named root bridge for binary analysis.
    pub fn configure_current_library_root(
        &mut self,
        root_name: Option<String>,
        library_root_file_id: Option<FileId>,
    ) {
        self.current_library_import_root = root_name;
        self.library_root_file_id = library_root_file_id;
        self.invalidate_hir_resolution_semantic();
    }

    /// Returns configured current-library named root bridge.
    #[must_use]
    pub fn current_library_root_config(&self) -> (Option<&str>, Option<FileId>) {
        (
            self.current_library_import_root.as_deref(),
            self.library_root_file_id,
        )
    }

    /// Parses a single file with recovery and caches the result by file id.
    ///
    /// # Errors
    ///
    /// Returns `ParseSessionError::MissingFile` for unknown ids and
    /// `ParseSessionError::Parse` for parser failures.
    pub fn parse_file_with_recovery(
        &mut self,
        file_id: FileId,
    ) -> Result<&ParsedFile, ParseSessionError> {
        if !self.parsed_by_file_id.contains_key(&file_id) {
            let file = self
                .db
                .file(file_id)
                .ok_or(ParseSessionError::MissingFile { file_id })?;
            let parsed = parse_source_file_from_source_file_with_recovery(file)
                .map_err(|error| {
                    ParseSessionError::Parse(FileParseError { file_id, error })
                })?;
            self.parsed_by_file_id.insert(file_id, parsed);
        }

        Ok(self
            .parsed_by_file_id
            .get(&file_id)
            .expect("parsed cache entry should exist"))
    }

    /// Parses and returns files in the same order as `file_ids`.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors from the first failed file.
    pub fn ensure_parsed_files_with_recovery(
        &mut self,
        file_ids: &[FileId],
    ) -> Result<(), ParseSessionError> {
        for &file_id in file_ids {
            self.parse_file_with_recovery(file_id)?;
        }
        Ok(())
    }

    /// Parses and returns files in the same order as `file_ids`.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors from the first failed file.
    pub fn parsed_files_with_recovery(
        &mut self,
        file_ids: &[FileId],
    ) -> Result<Vec<ParsedFile>, ParseSessionError> {
        self.ensure_parsed_files_with_recovery(file_ids)?;

        Ok(file_ids
            .iter()
            .map(|file_id| {
                self.parsed_by_file_id
                    .get(file_id)
                    .expect("parsed cache entry should exist")
                    .clone()
            })
            .collect())
    }

    /// Parses all files in insertion order with recovery and returns them.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors from the first failed file.
    pub fn parse_all_files_with_recovery(
        &mut self,
    ) -> Result<Vec<ParsedFile>, ParseSessionError> {
        let file_ids = self.ordered_file_ids.clone();
        self.parsed_files_with_recovery(&file_ids)
    }

    /// Runs the full pre-resolution frontend pipeline for selected files.
    ///
    /// Pipeline order:
    /// 1. Parse all reachable files in this context
    /// 2. Build global macro-definition index from parsed cache
    /// 3. Build per-file macro-scope table from parsed/index cache
    /// 4. Expand and desugar requested files
    ///
    /// # Errors
    ///
    /// Returns parser/session errors when any stage fails.
    pub fn pre_resolution_pipeline(
        &mut self,
        file_ids: &[FileId],
        options: ExpansionOptions,
    ) -> Result<Vec<DesugaredFile>, ParseSessionError> {
        self.parse_all_files_with_recovery()?;
        self.ensure_macro_definition_index()?;
        self.ensure_macro_scope_table()?;
        self.desugared_files(file_ids, options)
    }

    /// Runs the full pre-resolution pipeline for all files in insertion order.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors when any stage fails.
    pub fn pre_resolution_pipeline_in_order(
        &mut self,
        options: ExpansionOptions,
    ) -> Result<Vec<DesugaredFile>, ParseSessionError> {
        let file_ids = self.ordered_file_ids.clone();
        self.pre_resolution_pipeline(&file_ids, options)
    }

    /// Builds a macro-definition index from already parsed files.
    ///
    /// This step does not lex/parse source. It only reads parsed files already
    /// cached in this context.
    ///
    /// # Errors
    ///
    /// Returns `ParseSessionError::MissingFile` when any ordered file has not
    /// been parsed yet.
    pub fn ensure_macro_definition_index(
        &mut self,
    ) -> Result<&MacroDefinitionIndex, ParseSessionError> {
        if self.macro_definition_index.is_none() {
            let parsed_files = self
                .ordered_file_ids
                .iter()
                .copied()
                .map(|file_id| {
                    self.parsed_by_file_id
                        .get(&file_id)
                        .cloned()
                        .ok_or(ParseSessionError::MissingFile { file_id })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let index = MacroDefinitionIndex::from_parsed_files_with_paths(
                &parsed_files,
                &self.path_by_file_id,
            );
            self.macro_definition_index = Some(index);
            self.macro_scope_table = None;
        }

        Ok(self
            .macro_definition_index
            .as_ref()
            .expect("macro definition index should be cached"))
    }

    /// Ensures macro-definition index is built and cached.
    ///
    /// # Errors
    ///
    /// Returns parse-session errors when parsed inputs are missing.
    pub fn build_macro_definition_index(
        &mut self,
    ) -> Result<(), ParseSessionError> {
        self.ensure_macro_definition_index().map(|_| ())
    }

    /// Builds macro-only scope/import resolution table from cached parse/index data.
    ///
    /// This pass does not parse source and uses only cached frontend context data.
    ///
    /// # Errors
    ///
    /// Returns parse-session errors when parsed/index inputs are missing.
    pub fn ensure_macro_scope_table(
        &mut self,
    ) -> Result<&MacroScopeTable, ParseSessionError> {
        if self.macro_scope_table.is_none() {
            let index = self.ensure_macro_definition_index()?.clone();
            let parsed_files = self
                .ordered_file_ids
                .iter()
                .copied()
                .map(|file_id| {
                    self.parsed_by_file_id
                        .get(&file_id)
                        .cloned()
                        .ok_or(ParseSessionError::MissingFile { file_id })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let scope_table = MacroScopeTable::from_parsed_files_with_index(
                &parsed_files,
                &index,
                &self.path_by_file_id,
            );
            self.macro_scope_table = Some(scope_table);
        }

        Ok(self
            .macro_scope_table
            .as_ref()
            .expect("macro scope table should be cached"))
    }

    /// Ensures macro-scope table is built and cached.
    ///
    /// # Errors
    ///
    /// Returns parse-session errors when parse/index inputs are missing.
    pub fn build_macro_scope_table(&mut self) -> Result<(), ParseSessionError> {
        self.ensure_macro_scope_table().map(|_| ())
    }

    /// Expands requested files and caches expansion results.
    ///
    /// Already-expanded files are not re-expanded.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors when parsing missing inputs.
    pub fn ensure_expanded(
        &mut self,
        file_ids: &[FileId],
        options: ExpansionOptions,
    ) -> Result<(), ParseSessionError> {
        self.parse_all_files_with_recovery()?;
        let macro_index = self.ensure_macro_definition_index()?.clone();
        let macro_scope_table = self.ensure_macro_scope_table()?.clone();

        let missing = file_ids
            .iter()
            .copied()
            .filter(|file_id| {
                !self.expanded_by_file_id.contains_key(file_id)
                    || self.expanded_options_by_file_id.get(file_id).copied()
                        != Some(options)
            })
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }

        let parsed = missing
            .iter()
            .map(|file_id| {
                self.parsed_by_file_id
                    .get(file_id)
                    .expect("parsed cache entry should exist")
                    .clone()
            })
            .collect::<Vec<_>>();
        for expanded in expand_parsed_files_with_index_and_scope(
            &self.db,
            &parsed,
            &macro_index,
            &macro_scope_table,
            options,
        ) {
            self.desugared_by_file_id.remove(&expanded.file_id);
            self.expanded_options_by_file_id
                .insert(expanded.file_id, options);
            self.expanded_by_file_id.insert(expanded.file_id, expanded);
        }

        Ok(())
    }

    /// Returns expanded files in the same order as `file_ids`.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors when missing parsed inputs fail.
    pub fn expanded_files(
        &mut self,
        file_ids: &[FileId],
        options: ExpansionOptions,
    ) -> Result<Vec<ExpandedFile>, ParseSessionError> {
        self.ensure_expanded(file_ids, options)?;
        Ok(file_ids
            .iter()
            .map(|file_id| {
                self.expanded_by_file_id
                    .get(file_id)
                    .expect("expanded cache entry should exist")
                    .clone()
            })
            .collect())
    }

    /// Desugars requested files and caches desugared results.
    ///
    /// Already-desugared files are not desugared again.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors when expansion inputs cannot be produced.
    pub fn ensure_desugared(
        &mut self,
        file_ids: &[FileId],
        options: ExpansionOptions,
    ) -> Result<(), ParseSessionError> {
        let missing = file_ids
            .iter()
            .copied()
            .filter(|file_id| !self.desugared_by_file_id.contains_key(file_id))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }

        self.ensure_expanded(&missing, options)?;
        for file_id in missing {
            let expanded = self
                .expanded_by_file_id
                .get(&file_id)
                .expect("expanded cache entry should exist");
            self.desugared_by_file_id
                .entry(file_id)
                .or_insert_with(|| desugar_file(expanded));
        }

        Ok(())
    }

    /// Returns desugared files in the same order as `file_ids`.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors when parse/expand setup fails.
    pub fn desugared_files(
        &mut self,
        file_ids: &[FileId],
        options: ExpansionOptions,
    ) -> Result<Vec<DesugaredFile>, ParseSessionError> {
        self.ensure_desugared(file_ids, options)?;
        Ok(file_ids
            .iter()
            .map(|file_id| {
                self.desugared_by_file_id
                    .get(file_id)
                    .expect("desugared cache entry should exist")
                    .clone()
            })
            .collect())
    }

    /// Returns a single desugared file.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors when parse/expand setup fails.
    pub fn desugared_file(
        &mut self,
        file_id: FileId,
        options: ExpansionOptions,
    ) -> Result<DesugaredFile, ParseSessionError> {
        self.ensure_desugared(&[file_id], options)?;
        Ok(self
            .desugared_by_file_id
            .get(&file_id)
            .expect("desugared cache entry should exist")
            .clone())
    }

    /// Returns all desugared files in insertion order.
    ///
    /// # Errors
    ///
    /// Returns parser/session errors when parse/expand setup fails.
    pub fn desugared_files_in_order(
        &mut self,
        options: ExpansionOptions,
    ) -> Result<Vec<DesugaredFile>, ParseSessionError> {
        let file_ids = self.ordered_file_ids.clone();
        self.desugared_files(&file_ids, options)
    }

    #[must_use]
    pub(crate) fn cached_hir_for_entry(
        &self,
        entry_file_id: FileId,
    ) -> Option<&SemanticHirInput> {
        self.hir_by_entry_file_id.get(&entry_file_id)
    }

    #[must_use]
    pub(crate) fn cached_scope_graph_for_entry(
        &self,
        entry_file_id: FileId,
    ) -> Option<&ScopeGraph> {
        self.scope_graph_by_entry_file_id.get(&entry_file_id)
    }

    #[must_use]
    pub(crate) fn cached_scope_symbols_for_entry(
        &self,
        entry_file_id: FileId,
    ) -> Option<&BTreeMap<FileId, ScopeSymbols>> {
        self.scope_symbols_by_entry_file_id.get(&entry_file_id)
    }

    #[must_use]
    pub(crate) fn cached_imports_for_entry(
        &self,
        entry_file_id: FileId,
    ) -> Option<&BTreeMap<FileId, ResolvedImports>> {
        self.imports_by_entry_file_id.get(&entry_file_id)
    }

    #[must_use]
    pub(crate) fn cached_external_lookup_for_entry(
        &self,
        entry_file_id: FileId,
    ) -> Option<&ExternalSemanticLookup> {
        self.external_lookup_by_entry_file_id.get(&entry_file_id)
    }

    #[must_use]
    pub(crate) fn cached_item_definitions_for_entry(
        &self,
        entry_file_id: FileId,
    ) -> Option<&BTreeMap<ItemId, DefinitionLocation>> {
        self.item_definitions_by_entry_file_id.get(&entry_file_id)
    }

    #[must_use]
    pub(crate) fn cached_semantic_for_entry(
        &self,
        entry_file_id: FileId,
    ) -> Option<&SemanticAnalysis> {
        self.semantic_by_entry_file_id.get(&entry_file_id)
    }

    #[must_use]
    pub(crate) fn cached_analysis_diagnostics_for_entry(
        &self,
        entry_file_id: FileId,
    ) -> Option<&DiagnosticsBag> {
        self.analysis_diagnostics_by_entry_file_id.get(&entry_file_id)
    }

    #[must_use]
    pub(crate) fn is_entry_unresolved(&self, entry_file_id: FileId) -> bool {
        self.unresolved_entries.contains(&entry_file_id)
    }

    pub(crate) fn cache_entry_hir(
        &mut self,
        entry_file_id: FileId,
        hir: SemanticHirInput,
    ) {
        self.hir_by_entry_file_id.insert(entry_file_id, hir);
    }

    pub(crate) fn cache_entry_resolution(
        &mut self,
        entry_file_id: FileId,
        graph: ScopeGraph,
        symbols: BTreeMap<FileId, ScopeSymbols>,
        imports: BTreeMap<FileId, ResolvedImports>,
        external_lookup: ExternalSemanticLookup,
        item_definitions: BTreeMap<ItemId, DefinitionLocation>,
    ) {
        self.scope_graph_by_entry_file_id.insert(entry_file_id, graph);
        self.scope_symbols_by_entry_file_id
            .insert(entry_file_id, symbols);
        self.imports_by_entry_file_id.insert(entry_file_id, imports);
        self.external_lookup_by_entry_file_id
            .insert(entry_file_id, external_lookup);
        self.item_definitions_by_entry_file_id
            .insert(entry_file_id, item_definitions);
        self.unresolved_entries.remove(&entry_file_id);
    }

    pub(crate) fn cache_entry_semantic(
        &mut self,
        entry_file_id: FileId,
        semantic: SemanticAnalysis,
    ) {
        self.semantic_by_entry_file_id.insert(entry_file_id, semantic);
        self.unresolved_entries.remove(&entry_file_id);
    }

    pub(crate) fn cache_entry_diagnostics(
        &mut self,
        entry_file_id: FileId,
        diagnostics: DiagnosticsBag,
    ) {
        self.analysis_diagnostics_by_entry_file_id
            .insert(entry_file_id, diagnostics);
    }

    pub(crate) fn mark_entry_unresolved(
        &mut self,
        entry_file_id: FileId,
        diagnostics: DiagnosticsBag,
    ) {
        self.unresolved_entries.insert(entry_file_id);
        self.analysis_diagnostics_by_entry_file_id
            .insert(entry_file_id, diagnostics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_file_tracks_order_and_path_maps() {
        let mut context = FrontendContext::new();
        let file_id = context.add_file("src/main.cx", "fn main() {}");

        assert_eq!(context.ordered_file_ids(), &[file_id]);
        assert_eq!(
            context.file_id_for_path(Path::new("src/main.cx")),
            Some(file_id)
        );
        assert_eq!(
            context.path_for_file_id(file_id),
            Some(Path::new("src/main.cx"))
        );
    }

    #[test]
    fn desugar_pipeline_reuses_cached_results() {
        let mut context = FrontendContext::new();
        let file_id = context.add_file("src/main.cx", "fn main() {}");

        let first = context
            .desugared_file(file_id, ExpansionOptions::default())
            .expect("first desugar should succeed");
        let second = context
            .desugared_file(file_id, ExpansionOptions::default())
            .expect("second desugar should reuse cache");

        assert_eq!(first.file_id, file_id);
        assert_eq!(second.file_id, file_id);
        assert_eq!(context.parsed_by_file_id.len(), 1);
        assert_eq!(context.expanded_by_file_id.len(), 1);
        assert_eq!(context.desugared_by_file_id.len(), 1);
    }

    #[test]
    fn parse_all_files_with_recovery_populates_cache_once() {
        let mut context = FrontendContext::new();
        let first = context.add_file("src/root.cx", "scope util {}");
        let second = context.add_file("src/main.cx", "fn main() {}");

        let parsed = context
            .parse_all_files_with_recovery()
            .expect("parse all should succeed");
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed.iter().map(|file| file.file_id).collect::<Vec<_>>(),
            vec![first, second]
        );
        assert_eq!(context.parsed_by_file_id.len(), 2);

        let parsed_again = context
            .parse_all_files_with_recovery()
            .expect("second parse-all should reuse cache");
        assert_eq!(parsed_again.len(), 2);
        assert_eq!(context.parsed_by_file_id.len(), 2);
    }

    #[test]
    fn add_file_reuses_existing_file_id_for_same_path() {
        let mut context = FrontendContext::new();
        let first = context.add_file("src/main.cx", "fn one() {}");
        let second = context.add_file("src/main.cx", "fn two() {}");

        assert_eq!(first, second);
        assert_eq!(context.db.len(), 1);
    }

    #[test]
    fn macro_scope_table_is_cached_after_build() {
        let mut context = FrontendContext::new();
        context.add_file(
            "src/root.cx",
            "macro root_m { rule(input: Expr) => { input }; }",
        );
        context.add_file(
            "src/util.cx",
            "macro util_m { rule(input: Expr) => { input }; }",
        );
        let consumer_id = context.add_file(
            "src/consumer.cx",
            "use root::util::util_m;\nfn main() {}",
        );

        context
            .parse_all_files_with_recovery()
            .expect("parse-all should succeed");
        context
            .build_macro_definition_index()
            .expect("macro index should build");
        context
            .build_macro_scope_table()
            .expect("macro scope table should build");

        let table = context
            .macro_scope_table()
            .expect("scope table should be cached");
        let bindings = table
            .bindings_for_file(consumer_id)
            .expect("consumer bindings should exist");
        assert!(bindings.contains_key("util_m"));
    }
}
