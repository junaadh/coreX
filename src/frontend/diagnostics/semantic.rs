use crate::frontend::diagnostics::{
    Diagnostic, DiagnosticLabel, DiagnosticsBag, FileSpan,
};
use crate::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ResolvedBodyTable,
};
use crate::frontend::semantic::{
    BodyTypeEnvironmentTable, ControlFlowIssue, ControlFlowIssueKind,
    ControlFlowTable, ExprCheckIssue, ExprCheckIssueKind, ExpressionTypeTable,
    SignatureTypingIssue, StatementTypeTable, StmtCheckIssue,
    StmtCheckIssueKind, TypedItemTable, TypedItemTableIssue,
    TypedItemTableIssueKind,
};
use crate::frontend::source::{FileId, SourceDb};

/// Converts full semantic checker outputs into structured diagnostics.
#[must_use]
pub fn diagnostics_from_semantic_checks(
    db: &SourceDb,
    global_items: &GlobalItemTable,
    resolved_bodies: &ResolvedBodyTable,
    signature_issues: &[SignatureTypingIssue],
    typed_items: &TypedItemTable,
    body_envs: &BodyTypeEnvironmentTable,
    expr_types: &ExpressionTypeTable,
    stmt_types: &StatementTypeTable,
    control_flow: &ControlFlowTable,
) -> DiagnosticsBag {
    let mut unique = Vec::new();

    for diagnostic in signature_issues
        .iter()
        .map(|issue| diagnostic_from_signature_issue(db, issue))
    {
        push_unique_diagnostic(&mut unique, diagnostic);
    }
    for diagnostic in typed_items.issues.iter().map(|issue| {
        diagnostic_from_typed_item_table_issue(db, global_items, issue)
    }) {
        push_unique_diagnostic(&mut unique, diagnostic);
    }
    for diagnostic in body_envs
        .issues
        .iter()
        .map(|issue| diagnostic_from_body_env_issue(db, issue))
    {
        push_unique_diagnostic(&mut unique, diagnostic);
    }
    for diagnostic in expr_types.issues.iter().map(|issue| {
        diagnostic_from_expr_check_issue(db, resolved_bodies, issue)
    }) {
        push_unique_diagnostic(&mut unique, diagnostic);
    }
    for diagnostic in stmt_types.issues.iter().map(|issue| {
        diagnostic_from_stmt_check_issue(db, resolved_bodies, issue)
    }) {
        push_unique_diagnostic(&mut unique, diagnostic);
    }
    for diagnostic in control_flow.issues.iter().map(|issue| {
        diagnostic_from_control_flow_issue(db, resolved_bodies, issue)
    }) {
        push_unique_diagnostic(&mut unique, diagnostic);
    }

    let mut diagnostics = DiagnosticsBag::new();
    diagnostics.extend(unique);

    diagnostics
}

fn push_unique_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    diagnostic: Diagnostic,
) {
    if diagnostics.contains(&diagnostic) {
        return;
    }
    diagnostics.push(diagnostic);
}

/// Converts expression type-checking failures into structured diagnostics.
#[must_use]
pub fn diagnostic_from_expr_check_issue(
    db: &SourceDb,
    resolved_bodies: &ResolvedBodyTable,
    issue: &ExprCheckIssue,
) -> Diagnostic {
    use ExprCheckIssueKind as Kind;
    match &issue.kind {
        Kind::AssignmentTypeMismatch { target, value } => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("type mismatch"),
            format!("cannot assign `{value}` to `{target}`"),
        ),
        Kind::InvalidAssignmentTarget => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid assignment"),
            "assignment target is not assignable".to_string(),
        ),
        Kind::MutabilityViolation { .. } => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("mutability violation"),
            "cannot assign to immutable binding".to_string(),
        ),
        Kind::CallArityMismatch { expected, found } => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid call arity"),
            format!("expected {expected} argument(s), found {found}"),
        ),
        Kind::CallArgTypeMismatch {
            index,
            expected,
            found,
        } => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid argument type"),
            format!(
                "argument {} expects `{expected}`, found `{found}`",
                index.saturating_add(1),
            ),
        ),
        Kind::MissingResolvedReference { segments } => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("unresolved semantic body reference"),
            format!("cannot resolve `{}` in this body", segments.join("::")),
        ),
        Kind::MissingLocalType { local_id } => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("unresolved semantic body reference"),
            format!("missing type for local binding #{}", local_id.raw()),
        ),
        Kind::MissingTypedItem { item_id } => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("unresolved semantic type"),
            format!("missing typed item metadata for item #{}", item_id.raw()),
        ),
        Kind::InvalidCallCallee => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid call target"),
            "callee does not resolve to a callable function".to_string(),
        ),
        Kind::IncompatibleIfBranches {
            then_type,
            else_type,
        } => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("if branch type mismatch"),
            format!(
                "then branch has `{then_type}`, else branch has `{else_type}`"
            ),
        ),
        Kind::MissingElseBranch => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("if expression missing else branch"),
            "if expression requires an else branch".to_string(),
        ),
        Kind::InvalidUnaryOp | Kind::InvalidBinaryOp => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("type mismatch"),
            "operator operands are not type-compatible".to_string(),
        ),
        Kind::MissingBodyAst | Kind::MissingBodyEnvironment => with_body_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("semantic analysis failed"),
            "missing body analysis prerequisites".to_string(),
        ),
    }
}

/// Converts statement type-checking failures into structured diagnostics.
#[must_use]
pub fn diagnostic_from_stmt_check_issue(
    db: &SourceDb,
    resolved_bodies: &ResolvedBodyTable,
    issue: &StmtCheckIssue,
) -> Diagnostic {
    use StmtCheckIssueKind as Kind;
    match &issue.kind {
        Kind::AnnotatedLocalTypeMismatch {
            annotated,
            initializer,
            ..
        } => with_stmt_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("type mismatch"),
            format!(
                "annotated type `{annotated}` does not match initializer `{initializer}`"
            ),
        ),
        Kind::AssignmentTypeMismatch { target, value } => with_stmt_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("type mismatch"),
            format!("cannot assign `{value}` to `{target}`"),
        ),
        Kind::ReturnTypeMismatch { expected, found } => with_stmt_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid return type"),
            format!("expected `{expected}`, found `{found}`"),
        ),
        Kind::MissingReturnValue { expected } => with_stmt_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid return type"),
            format!("expected return value of type `{expected}`"),
        ),
        Kind::UnexpectedReturnValue { found } => with_stmt_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid return type"),
            format!("void function cannot return `{found}`"),
        ),
        Kind::InvalidConditionType { found } => with_stmt_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid condition type"),
            format!("expected `bool`, found `{found}`"),
        ),
        Kind::MissingPatternLocal { .. }
        | Kind::MissingExpressionType { .. } => with_stmt_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("unresolved semantic body reference"),
            "statement references unresolved body symbols".to_string(),
        ),
        Kind::MissingBodyAst | Kind::MissingBodyEnvironment => with_stmt_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("semantic analysis failed"),
            "missing statement analysis prerequisites".to_string(),
        ),
    }
}

/// Converts control-flow checking failures into structured diagnostics.
#[must_use]
pub fn diagnostic_from_control_flow_issue(
    db: &SourceDb,
    resolved_bodies: &ResolvedBodyTable,
    issue: &ControlFlowIssue,
) -> Diagnostic {
    use ControlFlowIssueKind as Kind;
    match &issue.kind {
        Kind::ReturnTypeMismatch { expected, found } => with_control_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid return type"),
            format!("expected `{expected}`, found `{found}`"),
        ),
        Kind::MissingReturnValue { expected } => with_control_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid return type"),
            format!("expected return value of type `{expected}`"),
        ),
        Kind::UnexpectedReturnValue { found } => with_control_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid return type"),
            format!("void function cannot return `{found}`"),
        ),
        Kind::TailTypeMismatch { expected, found } => with_control_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("type mismatch"),
            format!(
                "tail expression has `{found}` but function expects `{expected}`"
            ),
        ),
        Kind::MissingTailExpression { expected } => with_control_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid return type"),
            format!(
                "function with return type `{expected}` requires return or tail expression"
            ),
        ),
        Kind::UnexpectedTailValue { found } => with_control_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("invalid return type"),
            format!("void function cannot have tail value `{found}`"),
        ),
        Kind::IfBranchTypeMismatch {
            then_type,
            else_type,
        } => with_control_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("if branch type mismatch"),
            format!(
                "then branch has `{then_type}`, else branch has `{else_type}`"
            ),
        ),
        Kind::MissingElseBranch => with_control_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("if expression missing else branch"),
            "if expression requires an else branch".to_string(),
        ),
        Kind::MissingBodyAst
        | Kind::MissingBodyEnvironment
        | Kind::MissingBlockResultType => with_control_label(
            db,
            resolved_bodies,
            issue,
            Diagnostic::error("semantic analysis failed"),
            "missing control-flow analysis prerequisites".to_string(),
        ),
    }
}

fn with_body_label(
    db: &SourceDb,
    resolved_bodies: &ResolvedBodyTable,
    issue: &ExprCheckIssue,
    diagnostic: Diagnostic,
    message: String,
) -> Diagnostic {
    with_owner_label(
        db,
        resolved_bodies,
        &issue.owner,
        issue.body_index,
        issue.span,
        diagnostic,
        message,
    )
}

fn with_stmt_label(
    db: &SourceDb,
    resolved_bodies: &ResolvedBodyTable,
    issue: &StmtCheckIssue,
    diagnostic: Diagnostic,
    message: String,
) -> Diagnostic {
    with_owner_label(
        db,
        resolved_bodies,
        &issue.owner,
        issue.body_index,
        issue.span,
        diagnostic,
        message,
    )
}

fn with_control_label(
    db: &SourceDb,
    resolved_bodies: &ResolvedBodyTable,
    issue: &ControlFlowIssue,
    diagnostic: Diagnostic,
    message: String,
) -> Diagnostic {
    with_owner_label(
        db,
        resolved_bodies,
        &issue.owner,
        issue.body_index,
        issue.span,
        diagnostic,
        message,
    )
}

fn with_owner_label(
    db: &SourceDb,
    resolved_bodies: &ResolvedBodyTable,
    owner: &DeclarationOwner,
    body_index: usize,
    span: crate::frontend::ast::Span,
    diagnostic: Diagnostic,
    message: String,
) -> Diagnostic {
    let Some(file_id) = body_file_id(resolved_bodies, owner, body_index) else {
        return diagnostic;
    };
    let Some(label_span) = file_span(db, file_id, span) else {
        return diagnostic;
    };
    diagnostic.with_label(DiagnosticLabel::primary(label_span, message))
}

fn body_file_id(
    resolved_bodies: &ResolvedBodyTable,
    owner: &DeclarationOwner,
    body_index: usize,
) -> Option<FileId> {
    resolved_bodies
        .bodies_for_owner(owner)
        .iter()
        .find(|body| body.body_index == body_index)
        .map(|body| body.containing_scope_file_id)
}

fn file_span(
    db: &SourceDb,
    file_id: FileId,
    span: crate::frontend::ast::Span,
) -> Option<FileSpan> {
    let file = db.file(file_id)?;
    let fallback_end = usize::from(!file.is_empty());
    let final_span = if span.start == 0 && span.end == 0 {
        crate::frontend::ast::Span::new(0, fallback_end)
    } else {
        span
    };
    Some(FileSpan::new(file_id, final_span))
}

fn diagnostic_from_signature_issue(
    db: &SourceDb,
    issue: &crate::frontend::semantic::SignatureTypingIssue,
) -> Diagnostic {
    use crate::frontend::semantic::SignatureTypingIssueKind as Kind;
    match &issue.kind {
        Kind::UnresolvedPath { path } => with_optional_file_label(
            db,
            issue.containing_scope_file_id,
            Diagnostic::error("unresolved semantic type"),
            format!("cannot resolve type path `{}`", path.join("::")),
        ),
        Kind::InvalidTypeItem {
            path, item_kind, ..
        } => with_optional_file_label(
            db,
            issue.containing_scope_file_id,
            Diagnostic::error("invalid type reference"),
            format!(
                "path `{}` resolves to non-type item `{item_kind:?}`",
                path.join("::"),
            ),
        ),
        Kind::MissingResolvedItem { path, item_id } => {
            with_optional_file_label(
                db,
                issue.containing_scope_file_id,
                Diagnostic::error("unresolved semantic type"),
                format!(
                    "type path `{}` resolved to missing item #{}",
                    path.join("::"),
                    item_id.raw()
                ),
            )
        }
        Kind::MissingGlobalItemMetadata { item_id } => {
            Diagnostic::error("semantic analysis failed").with_note(format!(
                "missing global item metadata for item #{}",
                item_id.raw()
            ))
        }
        Kind::UnsupportedTypeSurface { description } => {
            Diagnostic::error("unsupported type surface")
                .with_note(*description)
        }
    }
}

fn diagnostic_from_typed_item_table_issue(
    db: &SourceDb,
    global_items: &GlobalItemTable,
    issue: &TypedItemTableIssue,
) -> Diagnostic {
    let mut diagnostic = match &issue.kind {
        TypedItemTableIssueKind::MissingSignatureForGlobalItem { item_kind } => {
            Diagnostic::error("semantic analysis failed").with_note(format!(
                "missing typed signature for global `{item_kind:?}` item"
            ))
        }
        TypedItemTableIssueKind::SignatureWithoutGlobalItem {
            signature_kind,
        } => Diagnostic::error("semantic analysis failed")
            .with_note(format!(
                "typed signature exists without global item for `{signature_kind:?}`"
            )),
        TypedItemTableIssueKind::SignatureKindMismatch {
            global_kind,
            signature_kind,
        } => Diagnostic::error("semantic analysis failed").with_note(format!(
            "global item kind `{global_kind:?}` mismatches typed signature kind `{signature_kind:?}`"
        )),
        TypedItemTableIssueKind::DuplicateImplOwner { owner } => {
            Diagnostic::error("semantic analysis failed")
                .with_note(format!("duplicate typed impl owner `{owner:?}`"))
        }
    };

    if let Some(item_id) = issue.associated_item_id
        && let Some(global_item) = global_items.get(item_id)
        && let Some(label_span) = file_span(
            db,
            global_item.defining_file_id,
            crate::frontend::ast::Span::new(0, 0),
        )
    {
        diagnostic = diagnostic.with_label(DiagnosticLabel::primary(
            label_span,
            format!(
                "semantic table issue is associated with `{}`",
                global_item.full_path.join("::")
            ),
        ));
    }

    diagnostic
}

fn diagnostic_from_body_env_issue(
    db: &SourceDb,
    issue: &crate::frontend::semantic::BodyEnvIssue,
) -> Diagnostic {
    use crate::frontend::semantic::BodyEnvIssueKind as Kind;
    match &issue.kind {
        Kind::UnresolvedLocalTypePath { path, .. } => with_optional_file_label(
            db,
            Some(issue.containing_scope_file_id),
            Diagnostic::error("unresolved semantic type"),
            format!("cannot resolve local type path `{}`", path.join("::")),
        ),
        Kind::InvalidLocalTypeItem { item_id, .. } => with_optional_file_label(
            db,
            Some(issue.containing_scope_file_id),
            Diagnostic::error("invalid type reference"),
            format!("local type resolves to invalid item #{}", item_id.raw()),
        ),
        Kind::MissingTypedItemForLocalType { item_id, .. } => {
            with_optional_file_label(
                db,
                Some(issue.containing_scope_file_id),
                Diagnostic::error("unresolved semantic type"),
                format!(
                    "missing typed metadata for local type item #{}",
                    item_id.raw()
                ),
            )
        }
        Kind::MissingBodySignature
        | Kind::MissingParameterType { .. }
        | Kind::MissingSelfType { .. } => with_optional_file_label(
            db,
            Some(issue.containing_scope_file_id),
            Diagnostic::error("semantic analysis failed"),
            "missing body signature metadata".to_string(),
        ),
        Kind::UnsupportedLocalTypeSurface { description, .. } => {
            with_optional_file_label(
                db,
                Some(issue.containing_scope_file_id),
                Diagnostic::error("unsupported type surface"),
                (*description).to_string(),
            )
        }
    }
}

fn with_optional_file_label(
    db: &SourceDb,
    file_id: Option<FileId>,
    diagnostic: Diagnostic,
    message: String,
) -> Diagnostic {
    let Some(file_id) = file_id else {
        return diagnostic;
    };
    let Some(label_span) =
        file_span(db, file_id, crate::frontend::ast::Span::new(0, 0))
    else {
        return diagnostic;
    };
    diagnostic.with_label(DiagnosticLabel::primary(label_span, message))
}
