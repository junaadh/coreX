use super::body_env::{
    BodyEnvIssue, BodyTypeEnvironmentTable, build_body_type_environments,
};
use super::control_flow::{
    ControlFlowIssue, ControlFlowTable, check_control_flow_with_tables,
};
use super::expr_check::{
    ExprCheckIssue, ExpressionTypeTable,
    check_expression_types_with_external_lookup,
};
use super::external_lookup::ExternalSemanticLookup;
use super::item_table::{
    TypedItemTable, TypedItemTableIssue, build_typed_item_table,
};
use super::signatures::{
    SignatureTypingIssue, TypedSignatureTable, type_declaration_signatures,
};
use super::stmt_check::{
    StatementTypeTable, StmtCheckIssue, check_statements_with_expression_types,
};
use super::typed_bodies::{
    TypedBodyTable, TypedBodyTableIssue, build_typed_body_table,
};
use crate::frontend::ParsedFile;
use crate::frontend::diagnostics::{
    DiagnosticsBag, diagnostics_from_semantic_checks,
};
use crate::frontend::resolver::{
    GlobalItemTable, ResolvedBodyTable, ResolvedDeclarationTable,
    ResolvedImports, ScopeGraph, resolve_bodies, resolve_declaration_types,
};
use crate::frontend::source::{FileId, SourceDb};
use std::collections::BTreeMap;

/// Full semantic-analysis outputs for one resolved target graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticAnalysis {
    pub global_items: GlobalItemTable,
    pub declarations: ResolvedDeclarationTable,
    pub signatures: TypedSignatureTable,
    pub typed_items: TypedItemTable,
    pub resolved_bodies: ResolvedBodyTable,
    pub body_envs: BodyTypeEnvironmentTable,
    pub expr_types: ExpressionTypeTable,
    pub stmt_types: StatementTypeTable,
    pub control_flow: ControlFlowTable,
    pub typed_bodies: TypedBodyTable,
    pub diagnostics: DiagnosticsBag,
}

/// Borrowed issue views grouped by semantic stage.
#[derive(Debug, Clone, Copy)]
pub struct SemanticAnalysisIssues<'a> {
    pub signature: &'a [SignatureTypingIssue],
    pub typed_item: &'a [TypedItemTableIssue],
    pub body_env: &'a [BodyEnvIssue],
    pub expr: &'a [ExprCheckIssue],
    pub stmt: &'a [StmtCheckIssue],
    pub control_flow: &'a [ControlFlowIssue],
    pub typed_body: &'a [TypedBodyTableIssue],
}

impl<'a> SemanticAnalysisIssues<'a> {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.signature.is_empty()
            && self.typed_item.is_empty()
            && self.body_env.is_empty()
            && self.expr.is_empty()
            && self.stmt.is_empty()
            && self.control_flow.is_empty()
            && self.typed_body.is_empty()
    }
}

impl SemanticAnalysis {
    /// Returns grouped semantic issues without duplicating issue storage.
    #[must_use]
    pub fn issues(&self) -> SemanticAnalysisIssues<'_> {
        SemanticAnalysisIssues {
            signature: &self.signatures.issues,
            typed_item: &self.typed_items.issues,
            body_env: &self.body_envs.issues,
            expr: &self.expr_types.issues,
            stmt: &self.stmt_types.issues,
            control_flow: &self.control_flow.issues,
            typed_body: &self.typed_bodies.issues,
        }
    }
}

/// Runs the full semantic pass chain for one resolved graph/import context.
#[must_use]
pub fn analyze_semantics(
    db: &SourceDb,
    graph: &ScopeGraph,
    parsed_files: &[ParsedFile],
    imports: &BTreeMap<FileId, ResolvedImports>,
) -> SemanticAnalysis {
    let external_lookup = ExternalSemanticLookup::default();
    analyze_semantics_with_external_lookup(
        db,
        graph,
        parsed_files,
        imports,
        &external_lookup,
    )
}

/// Runs the semantic pass chain with lookup-only external semantic context.
#[must_use]
pub fn analyze_semantics_with_external_lookup(
    db: &SourceDb,
    graph: &ScopeGraph,
    parsed_files: &[ParsedFile],
    imports: &BTreeMap<FileId, ResolvedImports>,
    external_lookup: &ExternalSemanticLookup,
) -> SemanticAnalysis {
    let global_items = GlobalItemTable::collect(graph, parsed_files);
    let declarations =
        resolve_declaration_types(graph, parsed_files, imports, &global_items);
    let signatures = type_declaration_signatures(&declarations, &global_items);
    let typed_items = build_typed_item_table(&global_items, &signatures);
    let resolved_bodies = resolve_bodies(
        graph,
        parsed_files,
        imports,
        &global_items,
        &declarations,
    );
    let body_envs =
        build_body_type_environments(&resolved_bodies, &typed_items);
    let expr_types = check_expression_types_with_external_lookup(
        graph,
        parsed_files,
        &global_items,
        &typed_items,
        &resolved_bodies,
        &body_envs,
        imports,
        external_lookup,
    );
    let stmt_types = check_statements_with_expression_types(
        graph,
        parsed_files,
        &global_items,
        &resolved_bodies,
        &body_envs,
        &expr_types,
    );
    let control_flow = check_control_flow_with_tables(
        graph,
        parsed_files,
        &global_items,
        &resolved_bodies,
        &body_envs,
        &expr_types,
        &stmt_types,
    );
    let typed_bodies = build_typed_body_table(
        &resolved_bodies,
        &body_envs,
        &expr_types,
        &stmt_types,
        &control_flow,
    );
    let diagnostics = diagnostics_from_semantic_checks(
        db,
        &global_items,
        &resolved_bodies,
        &signatures.issues,
        &typed_items,
        &body_envs,
        &expr_types,
        &stmt_types,
        &control_flow,
    );

    SemanticAnalysis {
        global_items,
        declarations,
        signatures,
        typed_items,
        resolved_bodies,
        body_envs,
        expr_types,
        stmt_types,
        control_flow,
        typed_bodies,
        diagnostics,
    }
}
