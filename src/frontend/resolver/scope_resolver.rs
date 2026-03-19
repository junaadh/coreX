use crate::frontend::DesugaredFile;
use crate::frontend::DiagnosticsBag;
use crate::frontend::ast::Item;
use crate::frontend::diagnostic_from_resolve_error;
use crate::frontend::resolver::error::ResolveError;
use crate::frontend::resolver::model::{
    ResolvedScope, ResolvedScopeKind, ScopeGraph,
};
use crate::frontend::source::{FileId, SourceDb, SourceFile};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

type ResolvedChildMetadata = Vec<(String, FileId, ResolvedScopeKind)>;
type ResolvedChildrenBundle = (Vec<FileId>, ResolvedChildMetadata);

pub struct ScopeResolver<'a> {
    db: &'a SourceDb,
    parsed_files: &'a [DesugaredFile],
}

impl<'a> ScopeResolver<'a> {
    #[must_use]
    pub fn new(db: &'a SourceDb, parsed_files: &'a [DesugaredFile]) -> Self {
        Self { db, parsed_files }
    }

    /// Resolves scopes for a library root file.
    ///
    /// # Errors
    ///
    /// Returns `ResolveError` when the root or declared child scopes cannot be
    /// resolved.
    pub fn resolve_library_root(
        &self,
        root_file_id: FileId,
    ) -> Result<ScopeGraph, ResolveError> {
        self.resolve_with_root_kind(root_file_id, ResolvedScopeKind::Root)
    }

    /// Resolves scopes for a binary root file.
    ///
    /// # Errors
    ///
    /// Returns `ResolveError` when the root or declared child scopes cannot be
    /// resolved.
    pub fn resolve_binary_root(
        &self,
        root_file_id: FileId,
    ) -> Result<ScopeGraph, ResolveError> {
        self.resolve_with_root_kind(root_file_id, ResolvedScopeKind::BinaryRoot)
    }

    /// Resolves a library scope graph while accumulating diagnostics.
    #[must_use]
    pub fn resolve_library_root_with_diagnostics(
        &self,
        root_file_id: FileId,
        db: &SourceDb,
    ) -> (Option<ScopeGraph>, DiagnosticsBag) {
        self.resolve_with_root_kind_with_diagnostics(
            root_file_id,
            ResolvedScopeKind::Root,
            db,
        )
    }

    /// Resolves a binary scope graph while accumulating diagnostics.
    #[must_use]
    pub fn resolve_binary_root_with_diagnostics(
        &self,
        root_file_id: FileId,
        db: &SourceDb,
    ) -> (Option<ScopeGraph>, DiagnosticsBag) {
        self.resolve_with_root_kind_with_diagnostics(
            root_file_id,
            ResolvedScopeKind::BinaryRoot,
            db,
        )
    }

    fn resolve_with_root_kind(
        &self,
        root_file_id: FileId,
        root_kind: ResolvedScopeKind,
    ) -> Result<ScopeGraph, ResolveError> {
        let source_file = self.db.file(root_file_id).ok_or_else(|| {
            let expected_path = match root_kind {
                ResolvedScopeKind::BinaryRoot => PathBuf::from("src/main.cx"),
                _ => PathBuf::from("src/root.cx"),
            };
            ResolveError::MissingRootFile { expected_path }
        })?;
        let _ = self.parsed_file_by_id(root_file_id).ok_or_else(|| {
            ResolveError::MissingRootFile {
                expected_path: source_file.path().to_path_buf(),
            }
        })?;

        let mut ctx = ResolveContext {
            parsed_by_id: self.parsed_by_id_map(),
            parsed_path_to_id: self.parsed_path_to_id_map(),
            scopes: BTreeMap::new(),
            visiting_stack: Vec::new(),
            visiting_pos: HashMap::new(),
            resolved: HashSet::new(),
        };

        let root_name = match root_kind {
            ResolvedScopeKind::BinaryRoot => "main",
            _ => "root",
        };
        self.resolve_scope_recursive(
            &mut ctx,
            root_file_id,
            root_kind,
            root_name.to_string(),
            &[],
        )?;

        Ok(ScopeGraph {
            root_file_id,
            scopes: ctx.scopes,
        })
    }

    fn resolve_with_root_kind_with_diagnostics(
        &self,
        root_file_id: FileId,
        root_kind: ResolvedScopeKind,
        render_db: &SourceDb,
    ) -> (Option<ScopeGraph>, DiagnosticsBag) {
        let mut diagnostics = DiagnosticsBag::new();
        let Some(source_file) = self.db.file(root_file_id) else {
            let expected_path = match root_kind {
                ResolvedScopeKind::BinaryRoot => PathBuf::from("src/main.cx"),
                _ => PathBuf::from("src/root.cx"),
            };
            let error = ResolveError::MissingRootFile { expected_path };
            diagnostics.push(diagnostic_from_resolve_error(render_db, &error));
            return (None, diagnostics);
        };
        if self.parsed_file_by_id(root_file_id).is_none() {
            let error = ResolveError::MissingRootFile {
                expected_path: source_file.path().to_path_buf(),
            };
            diagnostics.push(diagnostic_from_resolve_error(render_db, &error));
            return (None, diagnostics);
        }

        let mut ctx = ResolveContext {
            parsed_by_id: self.parsed_by_id_map(),
            parsed_path_to_id: self.parsed_path_to_id_map(),
            scopes: BTreeMap::new(),
            visiting_stack: Vec::new(),
            visiting_pos: HashMap::new(),
            resolved: HashSet::new(),
        };

        let root_name = match root_kind {
            ResolvedScopeKind::BinaryRoot => "main",
            _ => "root",
        };
        let mut diag_ctx = ResolveDiagnostics {
            render_db,
            diagnostics: &mut diagnostics,
        };
        let resolve_result = self.resolve_scope_recursive_with_diagnostics(
            &mut ctx,
            root_file_id,
            root_kind,
            root_name.to_string(),
            &[],
            &mut diag_ctx,
        );
        if let Err(error) = resolve_result {
            diagnostics.push(diagnostic_from_resolve_error(render_db, &error));
            return (None, diagnostics);
        }

        (
            Some(ScopeGraph {
                root_file_id,
                scopes: ctx.scopes,
            }),
            diagnostics,
        )
    }

    fn parsed_by_id_map(&self) -> HashMap<FileId, &'a DesugaredFile> {
        self.parsed_files.iter().map(|p| (p.file_id, p)).collect()
    }

    fn parsed_path_to_id_map(&self) -> HashMap<PathBuf, FileId> {
        self.parsed_files
            .iter()
            .filter_map(|parsed| {
                self.db
                    .file(parsed.file_id)
                    .map(|file| (file.path().to_path_buf(), parsed.file_id))
            })
            .collect()
    }

    fn parsed_file_by_id(&self, file_id: FileId) -> Option<&DesugaredFile> {
        self.parsed_files
            .iter()
            .find(|parsed| parsed.file_id == file_id)
    }

    fn source_file_by_id(&self, file_id: FileId) -> Option<&SourceFile> {
        self.db.file(file_id)
    }

    fn resolve_scope_recursive(
        &self,
        ctx: &mut ResolveContext<'a>,
        file_id: FileId,
        kind: ResolvedScopeKind,
        name: String,
        scope_path: &[String],
    ) -> Result<(), ResolveError> {
        if let Some(pos) = ctx.visiting_pos.get(&file_id).copied() {
            let mut cycle = ctx.visiting_stack[pos..].to_vec();
            cycle.push(file_id);
            return Err(ResolveError::ScopeCycle { cycle });
        }

        if ctx.resolved.contains(&file_id) {
            return Ok(());
        }

        let source_file = self.source_file_by_id(file_id).ok_or_else(|| {
            ResolveError::MissingRootFile {
                expected_path: PathBuf::from("src/root.cx"),
            }
        })?;
        let parsed = ctx.parsed_by_id.get(&file_id).ok_or_else(|| {
            ResolveError::MissingRootFile {
                expected_path: source_file.path().to_path_buf(),
            }
        })?;

        ctx.visiting_pos.insert(file_id, ctx.visiting_stack.len());
        ctx.visiting_stack.push(file_id);

        let declared_children = Self::collect_declared_child_scopes(parsed);
        let child_base_dir =
            Self::child_base_dir_for(source_file.path(), kind)?;

        let mut resolved_children = Vec::with_capacity(declared_children.len());
        let mut child_meta = Vec::with_capacity(declared_children.len());

        for declared_name in declared_children {
            let resolved_child = Self::probe_declared_child_scope(
                ctx,
                file_id,
                scope_path,
                &child_base_dir,
                &declared_name,
            )?;
            resolved_children.push(resolved_child.file_id);
            child_meta.push((
                declared_name,
                resolved_child.file_id,
                resolved_child.kind,
            ));
        }

        let scope = ResolvedScope {
            file_id,
            kind,
            name,
            scope_path: scope_path.to_owned(),
            child_scope_ids: resolved_children,
        };
        ctx.scopes.insert(file_id, scope);

        for (child_name, child_file_id, child_kind) in child_meta {
            let mut child_scope_path = scope_path.to_owned();
            child_scope_path.push(child_name.clone());
            self.resolve_scope_recursive(
                ctx,
                child_file_id,
                child_kind,
                child_name,
                &child_scope_path,
            )?;
        }

        ctx.visiting_pos.remove(&file_id);
        let _ = ctx.visiting_stack.pop();
        ctx.resolved.insert(file_id);
        Ok(())
    }

    fn resolve_scope_recursive_with_diagnostics(
        &self,
        ctx: &mut ResolveContext<'a>,
        file_id: FileId,
        kind: ResolvedScopeKind,
        name: String,
        scope_path: &[String],
        diag_ctx: &mut ResolveDiagnostics<'_>,
    ) -> Result<(), ResolveError> {
        if let Some(pos) = ctx.visiting_pos.get(&file_id).copied() {
            let mut cycle = ctx.visiting_stack[pos..].to_vec();
            cycle.push(file_id);
            return Err(ResolveError::ScopeCycle { cycle });
        }

        if ctx.resolved.contains(&file_id) {
            return Ok(());
        }

        let source_file = self.source_file_by_id(file_id).ok_or_else(|| {
            ResolveError::MissingRootFile {
                expected_path: PathBuf::from("src/root.cx"),
            }
        })?;
        let parsed = ctx.parsed_by_id.get(&file_id).ok_or_else(|| {
            ResolveError::MissingRootFile {
                expected_path: source_file.path().to_path_buf(),
            }
        })?;

        ctx.visiting_pos.insert(file_id, ctx.visiting_stack.len());
        ctx.visiting_stack.push(file_id);

        let (resolved_children, child_meta) =
            match Self::resolve_child_metadata_with_diagnostics(
                ctx,
                file_id,
                kind,
                source_file,
                parsed,
                scope_path,
                diag_ctx,
            ) {
                Ok(children) => children,
                Err(error) => {
                    ctx.visiting_pos.remove(&file_id);
                    let _ = ctx.visiting_stack.pop();
                    return Err(error);
                }
            };

        let scope = ResolvedScope {
            file_id,
            kind,
            name,
            scope_path: scope_path.to_owned(),
            child_scope_ids: resolved_children,
        };
        ctx.scopes.insert(file_id, scope);

        self.resolve_child_scopes_with_diagnostics(
            ctx,
            &child_meta,
            scope_path,
            diag_ctx,
        )?;

        ctx.visiting_pos.remove(&file_id);
        let _ = ctx.visiting_stack.pop();

        ctx.resolved.insert(file_id);
        Ok(())
    }

    fn resolve_child_metadata_with_diagnostics(
        ctx: &ResolveContext<'a>,
        file_id: FileId,
        kind: ResolvedScopeKind,
        source_file: &SourceFile,
        parsed: &DesugaredFile,
        scope_path: &[String],
        diag_ctx: &mut ResolveDiagnostics<'_>,
    ) -> Result<ResolvedChildrenBundle, ResolveError> {
        let declared_children = Self::collect_declared_child_scopes(parsed);
        let child_base_dir =
            Self::child_base_dir_for(source_file.path(), kind)?;
        let mut resolved_children = Vec::with_capacity(declared_children.len());
        let mut child_meta = Vec::with_capacity(declared_children.len());

        for declared_name in declared_children {
            match Self::probe_declared_child_scope(
                ctx,
                file_id,
                scope_path,
                &child_base_dir,
                &declared_name,
            ) {
                Ok(resolved_child) => {
                    resolved_children.push(resolved_child.file_id);
                    child_meta.push((
                        declared_name,
                        resolved_child.file_id,
                        resolved_child.kind,
                    ));
                }
                Err(
                    error @ (ResolveError::MissingDeclaredScope { .. }
                    | ResolveError::AmbiguousDeclaredScope { .. }),
                ) => {
                    diag_ctx.diagnostics.push(diagnostic_from_resolve_error(
                        diag_ctx.render_db,
                        &error,
                    ));
                }
                Err(error) => return Err(error),
            }
        }

        Ok((resolved_children, child_meta))
    }

    fn resolve_child_scopes_with_diagnostics(
        &self,
        ctx: &mut ResolveContext<'a>,
        child_meta: &[(String, FileId, ResolvedScopeKind)],
        scope_path: &[String],
        diag_ctx: &mut ResolveDiagnostics<'_>,
    ) -> Result<(), ResolveError> {
        for (child_name, child_file_id, child_kind) in child_meta {
            let mut child_scope_path = scope_path.to_owned();
            child_scope_path.push(child_name.clone());
            match self.resolve_scope_recursive_with_diagnostics(
                ctx,
                *child_file_id,
                *child_kind,
                child_name.clone(),
                &child_scope_path,
                diag_ctx,
            ) {
                Ok(()) => {}
                Err(
                    error @ (ResolveError::MissingDeclaredScope { .. }
                    | ResolveError::AmbiguousDeclaredScope { .. }),
                ) => {
                    diag_ctx.diagnostics.push(diagnostic_from_resolve_error(
                        diag_ctx.render_db,
                        &error,
                    ));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn collect_declared_child_scopes(parsed: &DesugaredFile) -> Vec<String> {
        parsed
            .ast
            .items
            .iter()
            .filter_map(|item| match &item.node {
                Item::Scope(scope_decl) => Some(scope_decl.node.name.clone()),
                _ => None,
            })
            .collect()
    }

    fn child_base_dir_for(
        file_path: &Path,
        kind: ResolvedScopeKind,
    ) -> Result<PathBuf, ResolveError> {
        match kind {
            ResolvedScopeKind::Root | ResolvedScopeKind::BinaryRoot => {
                Ok(file_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default())
            }
            ResolvedScopeKind::DirectoryBacked => Ok(file_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default()),
            ResolvedScopeKind::FileBacked => {
                let parent =
                    file_path.parent().ok_or(ResolveError::NonUtf8Path)?;
                let stem =
                    file_path.file_stem().ok_or(ResolveError::NonUtf8Path)?;
                Ok(parent.join(stem))
            }
        }
    }

    fn probe_declared_child_scope(
        ctx: &ResolveContext<'a>,
        parent_file_id: FileId,
        parent_scope_path: &[String],
        base_dir: &Path,
        declared_name: &str,
    ) -> Result<ResolvedChild, ResolveError> {
        let candidate_file = base_dir.join(format!("{declared_name}.cx"));
        let candidate_dir_file = base_dir
            .join(declared_name)
            .join(format!("{declared_name}.cx"));

        let file_id = ctx.parsed_path_to_id.get(&candidate_file).copied();
        let dir_file_id =
            ctx.parsed_path_to_id.get(&candidate_dir_file).copied();

        match (file_id, dir_file_id) {
            (Some(_), Some(_)) => Err(ResolveError::AmbiguousDeclaredScope {
                parent_file_id,
                parent_scope_path: parent_scope_path.to_vec(),
                declared_name: declared_name.to_string(),
                file_candidate: candidate_file,
                dir_candidate: candidate_dir_file,
            }),
            (None, None) => Err(ResolveError::MissingDeclaredScope {
                parent_file_id,
                parent_scope_path: parent_scope_path.to_vec(),
                declared_name: declared_name.to_string(),
                candidate_file,
                candidate_dir_file,
            }),
            (Some(found_id), None) => Ok(ResolvedChild {
                file_id: found_id,
                kind: ResolvedScopeKind::FileBacked,
            }),
            (None, Some(found_id)) => Ok(ResolvedChild {
                file_id: found_id,
                kind: ResolvedScopeKind::DirectoryBacked,
            }),
        }
    }
}

/// Resolves scopes for a project root file and root kind.
///
/// # Errors
///
/// Returns `ResolveError` when the root file is missing, child scope probing
/// fails, or a scope cycle is detected.
pub fn resolve_project_scopes(
    db: &SourceDb,
    parsed_files: &[DesugaredFile],
    root_file_id: FileId,
    kind: ResolvedScopeKind,
) -> Result<ScopeGraph, ResolveError> {
    let resolver = ScopeResolver::new(db, parsed_files);
    match kind {
        ResolvedScopeKind::Root => resolver.resolve_library_root(root_file_id),
        ResolvedScopeKind::BinaryRoot => {
            resolver.resolve_binary_root(root_file_id)
        }
        other => resolver.resolve_with_root_kind(root_file_id, other),
    }
}

struct ResolveContext<'a> {
    parsed_by_id: HashMap<FileId, &'a DesugaredFile>,
    parsed_path_to_id: HashMap<PathBuf, FileId>,
    scopes: BTreeMap<FileId, ResolvedScope>,
    visiting_stack: Vec<FileId>,
    visiting_pos: HashMap<FileId, usize>,
    resolved: HashSet<FileId>,
}

struct ResolvedChild {
    file_id: FileId,
    kind: ResolvedScopeKind,
}

struct ResolveDiagnostics<'a> {
    render_db: &'a SourceDb,
    diagnostics: &'a mut DiagnosticsBag,
}
