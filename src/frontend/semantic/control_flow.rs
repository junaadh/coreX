use super::body_env::BodyTypeEnvironmentTable;
use super::expr_check::{
    ExprCheckIssue, ExprCheckIssueKind, ExpressionTypeTable,
    check_expression_types,
};
use super::stmt_check::{
    StatementKind, StatementTypeEntry, StatementTypeTable,
    check_statements_with_expression_types,
};
use super::{BuiltinType, Type, TypedItemTable};
use crate::frontend::ExpandedFile;
use crate::frontend::ast::{Block, Item, Span, StructMember};
use crate::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ResolvedBodyTable, ScopeGraph,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BodyControlFlowId {
    pub owner: DeclarationOwner,
    pub body_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyControlFlowResult {
    pub id: BodyControlFlowId,
    pub expected_return_type: Type,
    pub block_result_type: Type,
    pub has_tail_expression: bool,
    pub return_count: usize,
    pub is_compatible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlFlowIssueKind {
    MissingBodyAst,
    MissingBodyEnvironment,
    MissingBlockResultType,
    MissingReturnValue { expected: Type },
    UnexpectedReturnValue { found: Type },
    ReturnTypeMismatch { expected: Type, found: Type },
    MissingTailExpression { expected: Type },
    TailTypeMismatch { expected: Type, found: Type },
    UnexpectedTailValue { found: Type },
    IfBranchTypeMismatch { then_type: Type, else_type: Type },
    MissingElseBranch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowIssue {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub span: Span,
    pub kind: ControlFlowIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowTable {
    by_body: BTreeMap<BodyControlFlowId, BodyControlFlowResult>,
    pub issues: Vec<ControlFlowIssue>,
}

impl ControlFlowTable {
    #[must_use]
    pub fn body(
        &self,
        id: &BodyControlFlowId,
    ) -> Option<&BodyControlFlowResult> {
        self.by_body.get(id)
    }

    #[must_use]
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&BodyControlFlowId, &BodyControlFlowResult)>
    {
        self.by_body.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_body.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_body.is_empty()
    }
}

#[must_use]
pub fn check_control_flow(
    graph: &ScopeGraph,
    parsed_files: &[ExpandedFile],
    global_items: &GlobalItemTable,
    typed_items: &TypedItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
) -> ControlFlowTable {
    let expr_types = check_expression_types(
        graph,
        parsed_files,
        global_items,
        typed_items,
        resolved_bodies,
        body_envs,
    );
    let stmt_types = check_statements_with_expression_types(
        graph,
        parsed_files,
        global_items,
        resolved_bodies,
        body_envs,
        &expr_types,
    );
    check_control_flow_with_tables(
        graph,
        parsed_files,
        global_items,
        resolved_bodies,
        body_envs,
        &expr_types,
        &stmt_types,
    )
}

#[must_use]
pub fn check_control_flow_with_tables(
    graph: &ScopeGraph,
    parsed_files: &[ExpandedFile],
    global_items: &GlobalItemTable,
    resolved_bodies: &ResolvedBodyTable,
    body_envs: &BodyTypeEnvironmentTable,
    expr_types: &ExpressionTypeTable,
    stmt_types: &StatementTypeTable,
) -> ControlFlowTable {
    let parsed_by_id: BTreeMap<FileId, &ExpandedFile> = parsed_files
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect();
    let body_blocks = collect_body_blocks(graph, &parsed_by_id, global_items);
    let expr_issues_by_body = index_expr_issues_by_body(&expr_types.issues);

    let mut by_body = BTreeMap::new();
    let mut issues = Vec::new();

    for body in resolved_bodies.iter() {
        let body_id = BodyControlFlowId {
            owner: body.owner.clone(),
            body_index: body.body_index,
        };

        let Some(env) = body_envs
            .envs_for_owner(&body.owner)
            .iter()
            .find(|candidate| candidate.body_index == body.body_index)
        else {
            issues.push(ControlFlowIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: ControlFlowIssueKind::MissingBodyEnvironment,
            });
            continue;
        };

        let Some(block_entry) = body_blocks
            .get(&body.owner)
            .and_then(|entries| entries.get(body.body_index))
        else {
            issues.push(ControlFlowIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: Span::new(0, 0),
                kind: ControlFlowIssueKind::MissingBodyAst,
            });
            continue;
        };

        let mut is_compatible = true;
        let expected = env.expected_return_type.clone();
        let block_result_type = if let Some(root_type) =
            expr_types.root_type(&body.owner, body.body_index)
        {
            root_type.clone()
        } else {
            issues.push(ControlFlowIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: block_entry.issue_span,
                kind: ControlFlowIssueKind::MissingBlockResultType,
            });
            is_compatible = false;
            Type::error()
        };

        let mut return_count = 0usize;
        for stmt_id in
            stmt_types.stmt_ids_for_body(&body.owner, body.body_index)
        {
            let Some(stmt_entry) = stmt_types.stmt_entry(&stmt_id) else {
                continue;
            };
            if stmt_entry.kind != StatementKind::Return {
                continue;
            }
            return_count = return_count.saturating_add(1);
            if !check_return_statement(body, stmt_entry, &expected, &mut issues)
            {
                is_compatible = false;
            }
        }

        if block_entry.has_tail_expression {
            if expected == Type::void() {
                if !is_type_compatible(&expected, &block_result_type) {
                    issues.push(ControlFlowIssue {
                        owner: body.owner.clone(),
                        body_index: body.body_index,
                        span: block_entry.tail_span,
                        kind: ControlFlowIssueKind::UnexpectedTailValue {
                            found: block_result_type.clone(),
                        },
                    });
                    is_compatible = false;
                }
            } else if !is_type_compatible(&expected, &block_result_type) {
                issues.push(ControlFlowIssue {
                    owner: body.owner.clone(),
                    body_index: body.body_index,
                    span: block_entry.tail_span,
                    kind: ControlFlowIssueKind::TailTypeMismatch {
                        expected: expected.clone(),
                        found: block_result_type.clone(),
                    },
                });
                is_compatible = false;
            }
        } else if expected != Type::void() && return_count == 0 {
            issues.push(ControlFlowIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: block_entry.issue_span,
                kind: ControlFlowIssueKind::MissingTailExpression {
                    expected: expected.clone(),
                },
            });
            is_compatible = false;
        }

        for expr_issue in expr_issues_by_body
            .get(&(body.owner.clone(), body.body_index))
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            if !map_if_branch_issue(body, expr_issue, &mut issues) {
                continue;
            }
            is_compatible = false;
        }

        let result = BodyControlFlowResult {
            id: body_id.clone(),
            expected_return_type: expected,
            block_result_type,
            has_tail_expression: block_entry.has_tail_expression,
            return_count,
            is_compatible,
        };
        by_body.insert(body_id, result);
    }

    ControlFlowTable { by_body, issues }
}

fn check_return_statement(
    body: &crate::frontend::resolver::ResolvedBody,
    stmt_entry: &StatementTypeEntry,
    expected: &Type,
    issues: &mut Vec<ControlFlowIssue>,
) -> bool {
    let found = &stmt_entry.ty;
    if expected == &Type::void() {
        if found != &Type::void() && !is_type_compatible(expected, found) {
            issues.push(ControlFlowIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: stmt_entry.span,
                kind: ControlFlowIssueKind::UnexpectedReturnValue {
                    found: found.clone(),
                },
            });
            return false;
        }
        return true;
    }

    if found == &Type::void() {
        issues.push(ControlFlowIssue {
            owner: body.owner.clone(),
            body_index: body.body_index,
            span: stmt_entry.span,
            kind: ControlFlowIssueKind::MissingReturnValue {
                expected: expected.clone(),
            },
        });
        return false;
    }

    if !is_type_compatible(expected, found) {
        issues.push(ControlFlowIssue {
            owner: body.owner.clone(),
            body_index: body.body_index,
            span: stmt_entry.span,
            kind: ControlFlowIssueKind::ReturnTypeMismatch {
                expected: expected.clone(),
                found: found.clone(),
            },
        });
        return false;
    }

    true
}

fn is_type_compatible(expected: &Type, found: &Type) -> bool {
    expected == found
        || expected.is_error()
        || found.is_error()
        || *found == Type::builtin(BuiltinType::Never)
}

fn index_expr_issues_by_body(
    expr_issues: &[ExprCheckIssue],
) -> BTreeMap<(DeclarationOwner, usize), Vec<&ExprCheckIssue>> {
    let mut grouped = BTreeMap::new();
    for issue in expr_issues {
        grouped
            .entry((issue.owner.clone(), issue.body_index))
            .or_insert_with(Vec::new)
            .push(issue);
    }
    grouped
}

fn map_if_branch_issue(
    body: &crate::frontend::resolver::ResolvedBody,
    expr_issue: &ExprCheckIssue,
    issues: &mut Vec<ControlFlowIssue>,
) -> bool {
    match &expr_issue.kind {
        ExprCheckIssueKind::IncompatibleIfBranches {
            then_type,
            else_type,
        } => {
            issues.push(ControlFlowIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: expr_issue.span,
                kind: ControlFlowIssueKind::IfBranchTypeMismatch {
                    then_type: then_type.clone(),
                    else_type: else_type.clone(),
                },
            });
            true
        }
        ExprCheckIssueKind::MissingElseBranch => {
            issues.push(ControlFlowIssue {
                owner: body.owner.clone(),
                body_index: body.body_index,
                span: expr_issue.span,
                kind: ControlFlowIssueKind::MissingElseBranch,
            });
            true
        }
        _ => false,
    }
}

struct BodyBlockEntry {
    has_tail_expression: bool,
    tail_span: Span,
    issue_span: Span,
}

fn collect_body_blocks(
    graph: &ScopeGraph,
    parsed_by_id: &BTreeMap<FileId, &ExpandedFile>,
    global_items: &GlobalItemTable,
) -> BTreeMap<DeclarationOwner, Vec<BodyBlockEntry>> {
    let mut result: BTreeMap<DeclarationOwner, Vec<BodyBlockEntry>> =
        BTreeMap::new();

    for (scope_file_id, scope) in &graph.scopes {
        let Some(parsed) = parsed_by_id.get(scope_file_id) else {
            continue;
        };

        let mut impl_index = 0usize;
        for item in &parsed.ast.items {
            match &item.node {
                Item::Function(function_decl) => {
                    if let Some(item_id) = item_id_for_top_level(
                        global_items,
                        *scope_file_id,
                        &scope.scope_path,
                        &function_decl.node.name,
                    ) {
                        result
                            .entry(DeclarationOwner::Item(item_id))
                            .or_default()
                            .push(block_entry(&function_decl.node.body));
                    }
                }
                Item::Struct(struct_decl) => {
                    if let Some(item_id) = item_id_for_top_level(
                        global_items,
                        *scope_file_id,
                        &scope.scope_path,
                        &struct_decl.node.name,
                    ) {
                        let owner = DeclarationOwner::Item(item_id);
                        for member in &struct_decl.node.members {
                            match &member.node {
                                StructMember::Function(function_decl) => {
                                    result
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(block_entry(
                                            &function_decl.node.body,
                                        ));
                                }
                                StructMember::Init(init_decl) => {
                                    result
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(block_entry(
                                            &init_decl.node.body,
                                        ));
                                }
                                StructMember::Field(_) => {}
                            }
                        }
                    }
                }
                Item::Enum(enum_decl) => {
                    if let Some(item_id) = item_id_for_top_level(
                        global_items,
                        *scope_file_id,
                        &scope.scope_path,
                        &enum_decl.node.name,
                    ) {
                        let owner = DeclarationOwner::Item(item_id);
                        for member in &enum_decl.node.members {
                            match &member.node {
                                crate::frontend::ast::EnumMember::Function(
                                    function_decl,
                                ) => {
                                    result
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(block_entry(
                                            &function_decl.node.body,
                                        ));
                                }
                                crate::frontend::ast::EnumMember::Init(
                                    init_decl,
                                ) => {
                                    result
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(block_entry(
                                            &init_decl.node.body,
                                        ));
                                }
                                crate::frontend::ast::EnumMember::Case(_) => {}
                            }
                        }
                    }
                }
                Item::Protocol(protocol_decl) => {
                    if let Some(item_id) = item_id_for_top_level(
                        global_items,
                        *scope_file_id,
                        &scope.scope_path,
                        &protocol_decl.node.name,
                    ) {
                        let owner = DeclarationOwner::Item(item_id);
                        for member in &protocol_decl.node.members {
                            match &member.node {
                                crate::frontend::ast::ProtocolMember::Function(
                                    function_member,
                                ) => {
                                    if let Some(default_body) =
                                        &function_member.node.default_body
                                    {
                                        result
                                            .entry(owner.clone())
                                            .or_default()
                                            .push(block_entry(default_body));
                                    }
                                }
                                crate::frontend::ast::ProtocolMember::Initializer(
                                    init_member,
                                ) => {
                                    if let Some(default_body) =
                                        &init_member.node.default_body
                                    {
                                        result
                                            .entry(owner.clone())
                                            .or_default()
                                            .push(block_entry(default_body));
                                    }
                                }
                                crate::frontend::ast::ProtocolMember::AssociatedType(_)
                                | crate::frontend::ast::ProtocolMember::Property(_) => {}
                            }
                        }
                    }
                }
                Item::Impl(impl_decl) => {
                    let owner = DeclarationOwner::Impl {
                        scope_file_id: *scope_file_id,
                        impl_index,
                    };
                    impl_index = impl_index.saturating_add(1);
                    for member in &impl_decl.node.members {
                        match &member.node {
                            crate::frontend::ast::ImplMember::Function(
                                function_decl,
                            ) => {
                                result.entry(owner.clone()).or_default().push(
                                    block_entry(&function_decl.node.body),
                                );
                            }
                            crate::frontend::ast::ImplMember::Init(
                                init_decl,
                            ) => {
                                result
                                    .entry(owner.clone())
                                    .or_default()
                                    .push(block_entry(&init_decl.node.body));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    result
}

fn block_entry(block: &Block) -> BodyBlockEntry {
    let tail_span = block
        .tail_expr
        .as_ref()
        .map(|tail| tail.span)
        .unwrap_or_else(|| Span::new(0, 0));
    let issue_span = block
        .tail_expr
        .as_ref()
        .map(|tail| tail.span)
        .or_else(|| block.statements.last().map(|stmt| stmt.span))
        .unwrap_or_else(|| Span::new(0, 0));
    BodyBlockEntry {
        has_tail_expression: block.tail_expr.is_some(),
        tail_span,
        issue_span,
    }
}

fn item_id_for_top_level(
    global_items: &GlobalItemTable,
    scope_file_id: FileId,
    scope_path: &[String],
    name: &str,
) -> Option<crate::frontend::resolver::ItemId> {
    let mut full_path = scope_path.to_vec();
    full_path.push(name.to_string());
    let item_id = global_items.item_id_by_full_path(&full_path)?;
    let item = global_items.get(item_id)?;
    (item.containing_scope_file_id == scope_file_id).then_some(item_id)
}
