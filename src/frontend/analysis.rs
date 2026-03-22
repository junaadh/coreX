use crate::frontend::context::FrontendContext;
use crate::frontend::diagnostics::diagnostics_from_inference_checks;
use crate::frontend::resolver::{
    ItemId, NamedImportRoot, ResolvedImports, ResolvedScopeKind, ScopeGraph,
    ScopeResolver, ScopeSymbols,
    resolve_project_imports_with_named_roots_and_diagnostics,
    resolve_project_scopes,
};
use crate::frontend::semantic::{
    DefinitionLocation, ExternalSemanticLookup, SemanticAnalysis,
    SemanticHirInput, analyze_semantics_with_external_lookup,
    build_external_semantic_lookup, collect_item_definition_locations,
    resolve_hir_semantic_input,
};
use crate::frontend::source::FileId;
use crate::frontend::{
    DesugaredFile, DiagnosticsBag, ExpandedFile, ExpansionOptions,
    ParseSessionError, ParsedFile, diagnostic_from_resolve_error,
};
use crate::midend::{BodyInferenceTable, infer_body_types};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct FrontendResolutionTables {
    pub root_kind: ResolvedScopeKind,
    pub graph: ScopeGraph,
    pub symbols: BTreeMap<FileId, ScopeSymbols>,
    pub imports: BTreeMap<FileId, ResolvedImports>,
    pub external_lookup: ExternalSemanticLookup,
    pub item_definitions: BTreeMap<ItemId, DefinitionLocation>,
}

#[derive(Debug, Clone)]
pub struct FrontendAnalysis {
    pub parsed: Vec<ParsedFile>,
    pub expanded: Vec<ExpandedFile>,
    pub desugared: Vec<DesugaredFile>,
    pub hir: BTreeMap<FileId, SemanticHirInput>,
    pub resolution_tables: BTreeMap<FileId, FrontendResolutionTables>,
    pub semantic_tables: BTreeMap<FileId, SemanticAnalysis>,
    /// Midend inference tables produced from resolved HIR + typed signatures.
    ///
    /// Canonical stage order:
    /// parse -> expand -> desugar -> HIR -> resolve -> type_check/type_infer
    pub inference_tables: BTreeMap<FileId, BodyInferenceTable>,
    pub diagnostics: DiagnosticsBag,
}

pub fn analyze_project(
    context: &mut FrontendContext,
    entry_files: &[FileId],
) -> Result<FrontendAnalysis, ParseSessionError> {
    let ordered_file_ids = context.ordered_file_ids().to_vec();
    let parsed = context.parsed_files_with_recovery(&ordered_file_ids)?;
    context.ensure_macro_definition_index()?;
    context.ensure_macro_scope_table()?;
    let expanded = context
        .expanded_files(&ordered_file_ids, ExpansionOptions::default())?;
    let desugared = context
        .desugared_files(&ordered_file_ids, ExpansionOptions::default())?;

    let mut diagnostics = DiagnosticsBag::new();
    for file in &desugared {
        extend_analysis_diagnostics(&mut diagnostics, &file.diagnostics);
    }

    let mut visiting = BTreeSet::new();
    for &entry_file_id in entry_files {
        analyze_entry_recursive(
            context,
            entry_file_id,
            &desugared,
            &mut visiting,
        )?;
        if let Some(entry_diagnostics) =
            context.cached_analysis_diagnostics_for_entry(entry_file_id)
        {
            extend_analysis_diagnostics(&mut diagnostics, entry_diagnostics);
        }
    }

    let mut hir = BTreeMap::new();
    let mut resolution_tables = BTreeMap::new();
    let mut semantic_tables = BTreeMap::new();
    let mut inference_tables = BTreeMap::new();
    for &entry_file_id in entry_files {
        if let Some(hir_table) = context.cached_hir_for_entry(entry_file_id) {
            hir.insert(entry_file_id, hir_table.clone());
        }
        if let (
            Some(graph),
            Some(symbols),
            Some(imports),
            Some(external_lookup),
            Some(item_definitions),
        ) = (
            context.cached_scope_graph_for_entry(entry_file_id),
            context.cached_scope_symbols_for_entry(entry_file_id),
            context.cached_imports_for_entry(entry_file_id),
            context.cached_external_lookup_for_entry(entry_file_id),
            context.cached_item_definitions_for_entry(entry_file_id),
        ) {
            resolution_tables.insert(
                entry_file_id,
                FrontendResolutionTables {
                    root_kind: context.root_kind_for_file_id(entry_file_id),
                    graph: graph.clone(),
                    symbols: symbols.clone(),
                    imports: imports.clone(),
                    external_lookup: external_lookup.clone(),
                    item_definitions: item_definitions.clone(),
                },
            );
        }
        if let Some(semantic) = context.cached_semantic_for_entry(entry_file_id)
        {
            let inferred = infer_body_types(
                &semantic.hir,
                &semantic.typed_items,
                &semantic.resolved_bodies,
                &semantic.body_envs,
            );
            let inference_diagnostics = diagnostics_from_inference_checks(
                context.db(),
                &semantic.resolved_bodies,
                &inferred.issues,
            );
            extend_analysis_diagnostics(
                &mut diagnostics,
                &inference_diagnostics,
            );
            inference_tables.insert(entry_file_id, inferred);
            semantic_tables.insert(entry_file_id, semantic.clone());
        }
    }

    Ok(FrontendAnalysis {
        parsed,
        expanded,
        desugared,
        hir,
        resolution_tables,
        semantic_tables,
        inference_tables,
        diagnostics,
    })
}

fn extend_analysis_diagnostics(
    target: &mut DiagnosticsBag,
    source: &DiagnosticsBag,
) {
    target.extend(
        source
            .as_slice()
            .iter()
            .filter(|diagnostic| {
                diagnostic.message != "unresolved macro import"
            })
            .cloned(),
    );
}

fn analyze_entry_recursive(
    context: &mut FrontendContext,
    entry_file_id: FileId,
    desugared_files: &[DesugaredFile],
    visiting: &mut BTreeSet<FileId>,
) -> Result<(), ParseSessionError> {
    if context
        .cached_analysis_diagnostics_for_entry(entry_file_id)
        .is_some()
        && (context.cached_semantic_for_entry(entry_file_id).is_some()
            || context.is_entry_unresolved(entry_file_id))
    {
        return Ok(());
    }

    if !visiting.insert(entry_file_id) {
        return Ok(());
    }

    let root_kind = context.root_kind_for_file_id(entry_file_id);
    let (graph, mut diagnostics) = {
        let resolver = ScopeResolver::new(context.db(), desugared_files);
        resolve_scope_graph_with_diagnostics(
            &resolver,
            context.db(),
            desugared_files,
            entry_file_id,
            root_kind,
        )
    };

    let Some(graph) = graph else {
        context.mark_entry_unresolved(entry_file_id, diagnostics);
        visiting.remove(&entry_file_id);
        return Ok(());
    };

    let mut named_roots = context.dependency_named_roots().clone();
    let (library_root_name, library_root_file_id) =
        context.current_library_root_config();
    let library_root_name = library_root_name.map(ToOwned::to_owned);
    if root_kind == ResolvedScopeKind::BinaryRoot
        && let (Some(root_name), Some(library_root_file_id)) =
            (library_root_name, library_root_file_id)
        && library_root_file_id != entry_file_id
    {
        analyze_entry_recursive(
            context,
            library_root_file_id,
            desugared_files,
            visiting,
        )?;
        if let Some(library_graph) = context
            .cached_scope_graph_for_entry(library_root_file_id)
            .cloned()
        {
            named_roots.insert(
                root_name.to_string(),
                NamedImportRoot::LoadedLibrary {
                    graph: library_graph,
                    parsed_files: desugared_files.to_vec(),
                    path_by_file_id: context.path_by_file_id().clone(),
                },
            );
        }
    }

    let (symbols, imports, import_diagnostics) =
        resolve_project_imports_with_named_roots_and_diagnostics(
            &graph,
            desugared_files,
            &named_roots,
            context.db(),
        );
    diagnostics.extend(import_diagnostics.as_slice().iter().cloned());

    let external_lookup = build_external_semantic_lookup(
        context.db(),
        &named_roots,
        &graph,
        desugared_files,
    );
    let resolved_input =
        resolve_hir_semantic_input(&graph, desugared_files, &imports);
    let hir = resolved_input.hir.clone();
    let semantic = analyze_semantics_with_external_lookup(
        context.db(),
        resolved_input,
        &external_lookup,
    );
    diagnostics.extend(semantic.diagnostics.as_slice().iter().cloned());
    let item_definitions = collect_item_definition_locations(
        &graph,
        desugared_files,
        &semantic.global_items,
    );

    context.cache_entry_resolution(
        entry_file_id,
        graph,
        symbols,
        imports,
        external_lookup,
        item_definitions,
    );
    context.cache_entry_hir(entry_file_id, hir);
    context.cache_entry_semantic(entry_file_id, semantic);
    context.cache_entry_diagnostics(entry_file_id, diagnostics);

    visiting.remove(&entry_file_id);
    Ok(())
}

fn resolve_scope_graph_with_diagnostics(
    resolver: &ScopeResolver<'_>,
    db: &crate::frontend::source::SourceDb,
    desugared_files: &[DesugaredFile],
    root_file_id: FileId,
    kind: ResolvedScopeKind,
) -> (Option<ScopeGraph>, DiagnosticsBag) {
    match kind {
        ResolvedScopeKind::Root => {
            resolver.resolve_library_root_with_diagnostics(root_file_id, db)
        }
        ResolvedScopeKind::BinaryRoot => {
            resolver.resolve_binary_root_with_diagnostics(root_file_id, db)
        }
        other => {
            let mut diagnostics = DiagnosticsBag::new();
            match resolve_project_scopes(
                db,
                desugared_files,
                root_file_id,
                other,
            ) {
                Ok(graph) => (Some(graph), diagnostics),
                Err(error) => {
                    diagnostics.push(diagnostic_from_resolve_error(db, &error));
                    (None, diagnostics)
                }
            }
        }
    }
}
