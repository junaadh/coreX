//! HIR-driven semantic completion.
//!
//! This module provides context-aware completion powered by the canonical
//! compiler pipeline: parse -> expand -> desugar -> HIR -> resolve -> type_check/type_infer.
//!
//! Completion is based on:
//! - HIR node/span lookup
//! - Resolver outputs (scope graphs, item tables)
//! - Type inference results
//! - Visibility/scope rules
//!
//! Completion does NOT use raw source text pattern matching.

mod candidates;
mod context;
mod hir_lookup;
mod span_index;
pub mod types;

pub use span_index::{HirNodeId, HirSpanIndex};
pub use types::{
    CompletionCandidate, CompletionContext, CompletionData, CompletionKind,
    CompletionMetadata,
};

use crate::frontend::hir::{HirExpr, HirExprKind, HirFile};
use crate::frontend::resolver::ResolvedImports;
use crate::frontend::semantic::{ExternalSemanticLookup, SemanticAnalysis};
use crate::frontend::source::{FileId, SourceDb};
use crate::midend::completion::context::determine_completion_context;
use crate::midend::completion::hir_lookup::{HirNodeContext, HirNodeKind};
use crate::midend::type_check::{ExpressionTypeTable, TypedSignatureTable};
use std::collections::BTreeMap;

/// Shared input data for completion computation.
///
/// This struct bundles together all the analysis results needed for
/// context-aware completion.
pub struct CompletionInput<'a> {
    /// The source database for file/position lookups.
    pub source_db: &'a SourceDb,

    /// HIR modules indexed by FileId.
    pub hir_files: &'a BTreeMap<FileId, HirFile>,

    /// Span indexes for efficient HIR node lookup.
    pub span_indexes: BTreeMap<FileId, HirSpanIndex>,

    /// Semantic analysis results including resolver outputs.
    pub semantic: &'a SemanticAnalysis,

    /// Typed signature tables for type declarations.
    pub signatures: &'a TypedSignatureTable,

    /// Expression type table for type inference results.
    pub expression_types: &'a ExpressionTypeTable,

    /// Import resolution by file.
    pub imports: &'a BTreeMap<FileId, ResolvedImports>,

    /// External semantic lookup for dependencies.
    pub external_lookup: &'a ExternalSemanticLookup,
}

impl<'a> CompletionInput<'a> {
    /// Create a new CompletionInput from the standard analysis pipeline outputs.
    #[must_use]
    pub fn new(
        source_db: &'a SourceDb,
        hir_files: &'a BTreeMap<FileId, HirFile>,
        semantic: &'a SemanticAnalysis,
        signatures: &'a TypedSignatureTable,
        expression_types: &'a ExpressionTypeTable,
        imports: &'a BTreeMap<FileId, ResolvedImports>,
        external_lookup: &'a ExternalSemanticLookup,
    ) -> Self {
        // Build span indexes for all HIR files
        let mut span_indexes = BTreeMap::new();
        for (&file_id, hir_file) in hir_files.iter() {
            if let Some(hir_module) = semantic.hir.hir_modules.get(&file_id) {
                let index =
                    HirSpanIndex::from_hir_file(file_id, hir_file, hir_module);
                span_indexes.insert(file_id, index);
            }
        }

        Self {
            source_db,
            hir_files,
            span_indexes,
            semantic,
            signatures,
            expression_types,
            imports,
            external_lookup,
        }
    }
}

/// Compute completion candidates for a given file and cursor position.
///
/// This is the main entry point for semantic completion. It:
/// 1. Determines the completion context from HIR
/// 2. Computes candidates based on the context
/// 3. Returns filtered, sorted completion results
///
/// # Arguments
/// * `input` - Shared analysis input
/// * `file_id` - The file where completion was triggered
/// * `offset` - The cursor offset in bytes
///
/// # Returns
/// * `Some(CompletionData)` if completion context could be determined
/// * `None` if the cursor is in a location where completion is not applicable
#[must_use]
pub fn completion_candidates(
    input: &CompletionInput,
    file_id: FileId,
    offset: usize,
) -> Option<CompletionData> {
    let span_index = input.span_indexes.get(&file_id)?;

    // First try to find the node at the cursor
    let (node_id, origin) = span_index
        .find_node_at_offset(offset)
        .or_else(|| span_index.find_node_before_offset(offset))?;

    let hir_file = input.hir_files.get(&file_id)?;
    let node_context = hir_node_from_id(hir_file, node_id, origin, input)?;

    let context =
        determine_completion_context(input, file_id, offset, &node_context)?;

    let candidates = match &context {
        CompletionContext::Global => {
            candidates::complete_global(input, file_id)
        }
        CompletionContext::PathAccess { scope_item } => {
            candidates::complete_path_access(input, file_id, *scope_item)
        }
        CompletionContext::AssociatedAccess { base_type } => {
            candidates::complete_associated_access(input, base_type)
        }
        CompletionContext::MemberAccess { receiver_type } => {
            candidates::complete_member_access(input, receiver_type)
        }
        CompletionContext::EnumCaseAccess { enum_type } => {
            candidates::complete_enum_cases(input, enum_type)
        }
    };

    Some(CompletionData {
        context,
        candidates,
    })
}

/// Determine the completion context from HIR node at cursor position.
///
/// This is a convenience wrapper that finds the HIR node and then
/// determines the completion context.
///
/// # Arguments
/// * `input` - Shared analysis input
/// * `file_id` - The file where completion was triggered
/// * `offset` - The cursor offset in bytes
///
/// # Returns
/// * `Some(CompletionContext)` if the cursor is in a completable location
/// * `None` if completion is not applicable here
/// Determine the completion context from HIR node at cursor position.
///
/// This is a convenience wrapper that finds the HIR node and then
/// determines the completion context.
///
/// # Arguments
/// * `input` - Shared analysis input
/// * `file_id` - The file where completion was triggered
/// * `offset` - The cursor offset in bytes
///
/// # Returns
/// * `Some(CompletionContext)` if the cursor is in a completable location
/// * `None` if completion is not applicable here
#[must_use]
pub fn completion_context_from_hir(
    input: &CompletionInput,
    file_id: FileId,
    offset: usize,
) -> Option<CompletionContext> {
    let span_index = input.span_indexes.get(&file_id)?;

    // First try to find the node at the cursor
    let (node_id, origin) = span_index
        .find_node_at_offset(offset)
        .or_else(|| span_index.find_node_before_offset(offset))?;

    let hir_file = input.hir_files.get(&file_id)?;
    let node_context = hir_node_from_id(hir_file, node_id, origin, input)?;
    determine_completion_context(input, file_id, offset, &node_context)
}

/// Convert a HirNodeId to a HirNodeContext with expression info.
fn hir_node_from_id(
    hir_file: &HirFile,
    node_id: HirNodeId,
    origin: crate::frontend::hir::HirOrigin,
    input: &CompletionInput,
) -> Option<HirNodeContext> {
    let hir_module = input.semantic.hir.hir_modules.get(&hir_file.file_id)?;

    let kind = match node_id {
        HirNodeId::Expr(expr_id) => {
            let expr = hir_module.exprs.get(&expr_id)?;
            determine_expr_kind(expr)
        }
        HirNodeId::Stmt(_) => HirNodeKind::OtherExpr,
        HirNodeId::Item(_) => HirNodeKind::NonExpr,
        HirNodeId::Body(_) => HirNodeKind::OtherExpr,
    };

    // Get expression type if this is an expression
    // TODO: Implement proper type lookup from ExpressionTypeTable
    let expr_info = match node_id {
        HirNodeId::Expr(expr_id) => {
            let _ty = None; // input.expression_types.get_type(expr_id);
            Some((expr_id, _ty))
        }
        _ => None,
    };

    Some(HirNodeContext {
        kind,
        origin,
        expr_info,
    })
}

/// Determine the kind of HIR expression for completion context.
fn determine_expr_kind(expr: &HirExpr) -> HirNodeKind {
    match &expr.kind {
        HirExprKind::Path(_) => HirNodeKind::Path,
        HirExprKind::Field { .. } => HirNodeKind::FieldAccess,
        HirExprKind::NamespaceField { .. } => HirNodeKind::NamespaceAccess,
        HirExprKind::MethodCall { .. } => HirNodeKind::MethodCall,
        _ => HirNodeKind::OtherExpr,
    }
}
