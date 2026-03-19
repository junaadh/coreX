use super::declaration_resolution::{
    DeclarationOwner, ResolvedDeclarationTable, ResolvedItemRef,
    ResolvedTypeRef,
};
use super::import_resolver::{ImportBindingKind, ResolvedImports};
use super::item_ids::ItemId;
use super::item_table::GlobalItemTable;
use super::local_ids::LocalId;
use super::model::ScopeGraph;
use crate::frontend::ExpandedFile;
use crate::frontend::ast::{
    ArrayElement, Clause, Expr, ForStmt, FunctionDecl, IfStmt, IfStmtElse,
    ImplMember, Item, MatchArmBody, Pattern, ProtocolMember, ReceiverKind,
    Span, Stmt, StructLiteralField, StructMember, Type as AstType, TypeExpr,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    Parameter,
    LocalBinding,
    PatternBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalMutability {
    Immutable,
    Mutable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBodyRef {
    Local(LocalId),
    Item(ItemId),
    Import(ItemId),
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLocalBinding {
    pub id: LocalId,
    pub kind: LocalKind,
    pub mutability: LocalMutability,
    pub name: String,
    pub declared_type: Option<ResolvedTypeRef>,
    pub declared_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Function,
    Initializer,
    ProtocolDefaultFunction,
    ProtocolDefaultInitializer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBodyReference {
    pub span: Span,
    pub segments: Vec<String>,
    pub resolved: ResolvedBodyRef,
    pub is_assignment_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedBodyReference {
    pub span: Span,
    pub segments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBody {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub signature_index: usize,
    pub kind: BodyKind,
    pub containing_scope_file_id: FileId,
    pub locals: Vec<ResolvedLocalBinding>,
    pub references: Vec<ResolvedBodyReference>,
    pub unresolved_references: Vec<UnresolvedBodyReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBodyTable {
    by_owner: BTreeMap<DeclarationOwner, Vec<ResolvedBody>>,
}

impl ResolvedBodyTable {
    #[must_use]
    pub fn bodies_for_owner(
        &self,
        owner: &DeclarationOwner,
    ) -> &[ResolvedBody] {
        self.by_owner.get(owner).map(Vec::as_slice).unwrap_or(&[])
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &ResolvedBody> {
        self.by_owner.values().flat_map(|bodies| bodies.iter())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_owner.values().map(Vec::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_owner.values().all(Vec::is_empty)
    }
}

#[must_use]
pub fn resolve_bodies(
    graph: &ScopeGraph,
    parsed_files: &[ExpandedFile],
    imports: &BTreeMap<FileId, ResolvedImports>,
    item_table: &GlobalItemTable,
    declarations: &ResolvedDeclarationTable,
) -> ResolvedBodyTable {
    BodyResolver {
        graph,
        parsed_by_id: parsed_files
            .iter()
            .map(|parsed| (parsed.file_id, parsed))
            .collect(),
        imports,
        item_table,
        declarations,
        next_local_id: 0,
    }
    .resolve()
}

struct BodyResolver<'a> {
    graph: &'a ScopeGraph,
    parsed_by_id: BTreeMap<FileId, &'a ExpandedFile>,
    imports: &'a BTreeMap<FileId, ResolvedImports>,
    item_table: &'a GlobalItemTable,
    declarations: &'a ResolvedDeclarationTable,
    next_local_id: u32,
}

impl<'a> BodyResolver<'a> {
    fn resolve(mut self) -> ResolvedBodyTable {
        let mut by_owner: BTreeMap<DeclarationOwner, Vec<ResolvedBody>> =
            BTreeMap::new();

        for (scope_file_id, scope) in &self.graph.scopes {
            let Some(parsed) = self.parsed_by_id.get(scope_file_id) else {
                continue;
            };

            let mut impl_index = 0usize;
            for item in &parsed.ast.items {
                match &item.node {
                    Item::Function(function_decl) => {
                        let Some(item_id) = self.item_id_for_top_level(
                            scope_file_id,
                            &scope.scope_path,
                            &function_decl.node.name,
                        ) else {
                            continue;
                        };
                        if !self.declarations.by_item_id.contains_key(&item_id)
                        {
                            continue;
                        }
                        let owner = DeclarationOwner::Item(item_id);
                        let body_index =
                            by_owner.get(&owner).map_or(0, Vec::len);
                        let body = self.resolve_function_body(
                            owner.clone(),
                            body_index,
                            0,
                            BodyKind::Function,
                            *scope_file_id,
                            &function_decl.node,
                        );
                        by_owner.entry(owner).or_default().push(body);
                    }
                    Item::Struct(struct_decl) => {
                        let Some(item_id) = self.item_id_for_top_level(
                            scope_file_id,
                            &scope.scope_path,
                            &struct_decl.node.name,
                        ) else {
                            continue;
                        };
                        if !self.declarations.by_item_id.contains_key(&item_id)
                        {
                            continue;
                        }
                        let owner = DeclarationOwner::Item(item_id);
                        let mut method_signature_index = 0usize;
                        let mut initializer_signature_index = 0usize;
                        for member in &struct_decl.node.members {
                            match &member.node {
                                StructMember::Function(function_decl) => {
                                    let body_index = by_owner
                                        .get(&owner)
                                        .map_or(0, Vec::len);
                                    let body = self.resolve_function_body(
                                        owner.clone(),
                                        body_index,
                                        method_signature_index,
                                        BodyKind::Function,
                                        *scope_file_id,
                                        &function_decl.node,
                                    );
                                    method_signature_index =
                                        method_signature_index
                                            .saturating_add(1);
                                    by_owner
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(body);
                                }
                                StructMember::Init(init_decl) => {
                                    let body_index = by_owner
                                        .get(&owner)
                                        .map_or(0, Vec::len);
                                    let body = self.resolve_init_body(
                                        owner.clone(),
                                        body_index,
                                        initializer_signature_index,
                                        BodyKind::Initializer,
                                        *scope_file_id,
                                        &init_decl.node,
                                    );
                                    initializer_signature_index =
                                        initializer_signature_index
                                            .saturating_add(1);
                                    by_owner
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(body);
                                }
                                StructMember::Field(_) => {}
                            }
                        }
                    }
                    Item::Enum(enum_decl) => {
                        let Some(item_id) = self.item_id_for_top_level(
                            scope_file_id,
                            &scope.scope_path,
                            &enum_decl.node.name,
                        ) else {
                            continue;
                        };
                        if !self.declarations.by_item_id.contains_key(&item_id)
                        {
                            continue;
                        }
                        let owner = DeclarationOwner::Item(item_id);
                        let mut method_signature_index = 0usize;
                        let mut initializer_signature_index = 0usize;
                        for member in &enum_decl.node.members {
                            match &member.node {
                                crate::frontend::ast::EnumMember::Function(
                                    function_decl,
                                ) => {
                                    let body_index = by_owner
                                        .get(&owner)
                                        .map_or(0, Vec::len);
                                    let body = self.resolve_function_body(
                                        owner.clone(),
                                        body_index,
                                        method_signature_index,
                                        BodyKind::Function,
                                        *scope_file_id,
                                        &function_decl.node,
                                    );
                                    method_signature_index =
                                        method_signature_index
                                            .saturating_add(1);
                                    by_owner
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(body);
                                }
                                crate::frontend::ast::EnumMember::Init(
                                    init_decl,
                                ) => {
                                    let body_index = by_owner
                                        .get(&owner)
                                        .map_or(0, Vec::len);
                                    let body = self.resolve_init_body(
                                        owner.clone(),
                                        body_index,
                                        initializer_signature_index,
                                        BodyKind::Initializer,
                                        *scope_file_id,
                                        &init_decl.node,
                                    );
                                    initializer_signature_index =
                                        initializer_signature_index
                                            .saturating_add(1);
                                    by_owner
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(body);
                                }
                                crate::frontend::ast::EnumMember::Case(_) => {}
                            }
                        }
                    }
                    Item::Protocol(protocol_decl) => {
                        let Some(item_id) = self.item_id_for_top_level(
                            scope_file_id,
                            &scope.scope_path,
                            &protocol_decl.node.name,
                        ) else {
                            continue;
                        };
                        if !self.declarations.by_item_id.contains_key(&item_id)
                        {
                            continue;
                        }
                        let owner = DeclarationOwner::Item(item_id);
                        let mut method_signature_index = 0usize;
                        let mut initializer_signature_index = 0usize;
                        for member in &protocol_decl.node.members {
                            match &member.node {
                                ProtocolMember::Function(function_member) => {
                                    let signature_index =
                                        method_signature_index;
                                    method_signature_index =
                                        method_signature_index
                                            .saturating_add(1);
                                    if function_member
                                        .node
                                        .default_body
                                        .is_none()
                                    {
                                        continue;
                                    }
                                    let body_index = by_owner
                                        .get(&owner)
                                        .map_or(0, Vec::len);
                                    let body = self
                                        .resolve_protocol_function_default_body(
                                            owner.clone(),
                                            body_index,
                                            signature_index,
                                            *scope_file_id,
                                            &function_member.node,
                                        );
                                    by_owner
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(body);
                                }
                                ProtocolMember::Initializer(init_member) => {
                                    let signature_index =
                                        initializer_signature_index;
                                    initializer_signature_index =
                                        initializer_signature_index
                                            .saturating_add(1);
                                    if init_member.node.default_body.is_none() {
                                        continue;
                                    }
                                    let body_index = by_owner
                                        .get(&owner)
                                        .map_or(0, Vec::len);
                                    let body = self
                                        .resolve_protocol_init_default_body(
                                            owner.clone(),
                                            body_index,
                                            signature_index,
                                            *scope_file_id,
                                            &init_member.node,
                                        );
                                    by_owner
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(body);
                                }
                                ProtocolMember::AssociatedType(_)
                                | ProtocolMember::Property(_) => {}
                            }
                        }
                    }
                    Item::Impl(impl_decl) => {
                        let owner = DeclarationOwner::Impl {
                            scope_file_id: *scope_file_id,
                            impl_index,
                        };
                        impl_index = impl_index.saturating_add(1);
                        if impl_index
                            > self
                                .declarations
                                .impls_in_scope(*scope_file_id)
                                .len()
                        {
                            continue;
                        }
                        let mut method_signature_index = 0usize;
                        let mut initializer_signature_index = 0usize;
                        for member in &impl_decl.node.members {
                            match &member.node {
                                ImplMember::Function(function_decl) => {
                                    let body_index = by_owner
                                        .get(&owner)
                                        .map_or(0, Vec::len);
                                    let body = self.resolve_function_body(
                                        owner.clone(),
                                        body_index,
                                        method_signature_index,
                                        BodyKind::Function,
                                        *scope_file_id,
                                        &function_decl.node,
                                    );
                                    method_signature_index =
                                        method_signature_index
                                            .saturating_add(1);
                                    by_owner
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(body);
                                }
                                ImplMember::Init(init_decl) => {
                                    let body_index = by_owner
                                        .get(&owner)
                                        .map_or(0, Vec::len);
                                    let body = self.resolve_init_body(
                                        owner.clone(),
                                        body_index,
                                        initializer_signature_index,
                                        BodyKind::Initializer,
                                        *scope_file_id,
                                        &init_decl.node,
                                    );
                                    initializer_signature_index =
                                        initializer_signature_index
                                            .saturating_add(1);
                                    by_owner
                                        .entry(owner.clone())
                                        .or_default()
                                        .push(body);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        ResolvedBodyTable { by_owner }
    }

    fn item_id_for_top_level(
        &self,
        scope_file_id: &FileId,
        scope_path: &[String],
        name: &str,
    ) -> Option<ItemId> {
        let mut full_path = scope_path.to_vec();
        full_path.push(name.to_string());
        let item_id = self.item_table.item_id_by_full_path(&full_path)?;
        let item = self.item_table.get(item_id)?;
        (item.containing_scope_file_id == *scope_file_id).then_some(item_id)
    }

    fn resolve_function_body(
        &mut self,
        owner: DeclarationOwner,
        body_index: usize,
        signature_index: usize,
        kind: BodyKind,
        scope_file_id: FileId,
        function_decl: &FunctionDecl,
    ) -> ResolvedBody {
        let mut resolved = ResolvedBody {
            owner,
            body_index,
            signature_index,
            kind,
            containing_scope_file_id: scope_file_id,
            locals: Vec::new(),
            references: Vec::new(),
            unresolved_references: Vec::new(),
        };
        let mut scopes = LocalScopeStack::default();
        scopes.push();
        if let Some(receiver) = &function_decl.receiver {
            self.declare_local(
                &mut scopes,
                &mut resolved.locals,
                "self".to_string(),
                LocalKind::Parameter,
                receiver.node.to_local_mutability_for_parameter(),
                None,
                receiver.span,
            );
        }
        for param in &function_decl.params {
            let declared_type = Some(self.resolve_type_ref(
                &resolved.owner,
                scope_file_id,
                &param.node.ty.node,
            ));
            self.declare_local(
                &mut scopes,
                &mut resolved.locals,
                param.node.name.clone(),
                LocalKind::Parameter,
                LocalMutability::Immutable,
                declared_type,
                param.span,
            );
        }
        self.resolve_block(
            scope_file_id,
            &mut scopes,
            &mut resolved,
            &function_decl.body,
        );
        resolved
    }

    fn resolve_init_body(
        &mut self,
        owner: DeclarationOwner,
        body_index: usize,
        signature_index: usize,
        kind: BodyKind,
        scope_file_id: FileId,
        init_decl: &crate::frontend::ast::InitDecl,
    ) -> ResolvedBody {
        let mut resolved = ResolvedBody {
            owner,
            body_index,
            signature_index,
            kind,
            containing_scope_file_id: scope_file_id,
            locals: Vec::new(),
            references: Vec::new(),
            unresolved_references: Vec::new(),
        };
        let mut scopes = LocalScopeStack::default();
        scopes.push();
        if let Some(receiver) = &init_decl.receiver {
            self.declare_local(
                &mut scopes,
                &mut resolved.locals,
                "self".to_string(),
                LocalKind::Parameter,
                receiver.node.to_local_mutability_for_parameter(),
                None,
                receiver.span,
            );
        }
        for param in &init_decl.params {
            let declared_type = Some(self.resolve_type_ref(
                &resolved.owner,
                scope_file_id,
                &param.node.ty.node,
            ));
            self.declare_local(
                &mut scopes,
                &mut resolved.locals,
                param.node.name.clone(),
                LocalKind::Parameter,
                LocalMutability::Immutable,
                declared_type,
                param.span,
            );
        }
        self.resolve_block(
            scope_file_id,
            &mut scopes,
            &mut resolved,
            &init_decl.body,
        );
        resolved
    }

    fn resolve_protocol_function_default_body(
        &mut self,
        owner: DeclarationOwner,
        body_index: usize,
        signature_index: usize,
        scope_file_id: FileId,
        function_member: &crate::frontend::ast::ProtocolFunctionMember,
    ) -> ResolvedBody {
        let mut resolved = ResolvedBody {
            owner,
            body_index,
            signature_index,
            kind: BodyKind::ProtocolDefaultFunction,
            containing_scope_file_id: scope_file_id,
            locals: Vec::new(),
            references: Vec::new(),
            unresolved_references: Vec::new(),
        };
        let Some(block) = &function_member.default_body else {
            return resolved;
        };

        let mut scopes = LocalScopeStack::default();
        scopes.push();
        if let Some(receiver) = &function_member.receiver {
            self.declare_local(
                &mut scopes,
                &mut resolved.locals,
                "self".to_string(),
                LocalKind::Parameter,
                receiver.node.to_local_mutability_for_parameter(),
                None,
                receiver.span,
            );
        }
        for param in &function_member.params {
            let declared_type = Some(self.resolve_type_ref(
                &resolved.owner,
                scope_file_id,
                &param.node.ty.node,
            ));
            self.declare_local(
                &mut scopes,
                &mut resolved.locals,
                param.node.name.clone(),
                LocalKind::Parameter,
                LocalMutability::Immutable,
                declared_type,
                param.span,
            );
        }
        self.resolve_block(scope_file_id, &mut scopes, &mut resolved, block);
        resolved
    }

    fn resolve_protocol_init_default_body(
        &mut self,
        owner: DeclarationOwner,
        body_index: usize,
        signature_index: usize,
        scope_file_id: FileId,
        init_member: &crate::frontend::ast::ProtocolInitMember,
    ) -> ResolvedBody {
        let mut resolved = ResolvedBody {
            owner,
            body_index,
            signature_index,
            kind: BodyKind::ProtocolDefaultInitializer,
            containing_scope_file_id: scope_file_id,
            locals: Vec::new(),
            references: Vec::new(),
            unresolved_references: Vec::new(),
        };
        let Some(block) = &init_member.default_body else {
            return resolved;
        };

        let mut scopes = LocalScopeStack::default();
        scopes.push();
        if let Some(receiver) = &init_member.receiver {
            self.declare_local(
                &mut scopes,
                &mut resolved.locals,
                "self".to_string(),
                LocalKind::Parameter,
                receiver.node.to_local_mutability_for_parameter(),
                None,
                receiver.span,
            );
        }
        for param in &init_member.params {
            let declared_type = Some(self.resolve_type_ref(
                &resolved.owner,
                scope_file_id,
                &param.node.ty.node,
            ));
            self.declare_local(
                &mut scopes,
                &mut resolved.locals,
                param.node.name.clone(),
                LocalKind::Parameter,
                LocalMutability::Immutable,
                declared_type,
                param.span,
            );
        }
        self.resolve_block(scope_file_id, &mut scopes, &mut resolved, block);
        resolved
    }

    fn resolve_block(
        &mut self,
        scope_file_id: FileId,
        scopes: &mut LocalScopeStack,
        body: &mut ResolvedBody,
        block: &crate::frontend::ast::Block,
    ) {
        scopes.push();
        for stmt in &block.statements {
            self.resolve_stmt(scope_file_id, scopes, body, &stmt.node);
        }
        if let Some(tail_expr) = &block.tail_expr {
            self.resolve_expr(scope_file_id, scopes, body, tail_expr, false);
        }
        scopes.pop();
    }

    fn resolve_stmt(
        &mut self,
        scope_file_id: FileId,
        scopes: &mut LocalScopeStack,
        body: &mut ResolvedBody,
        stmt: &Stmt,
    ) {
        match stmt {
            Stmt::Let(let_stmt) => {
                self.resolve_let_like_stmt(
                    scope_file_id,
                    scopes,
                    body,
                    &let_stmt.node.pattern,
                    let_stmt.node.ty.as_ref(),
                    let_stmt.node.value.as_deref(),
                    LocalMutability::Immutable,
                );
            }
            Stmt::Var(var_stmt) => {
                self.resolve_let_like_stmt(
                    scope_file_id,
                    scopes,
                    body,
                    &var_stmt.node.pattern,
                    var_stmt.node.ty.as_ref(),
                    var_stmt.node.value.as_deref(),
                    LocalMutability::Mutable,
                );
            }
            Stmt::Expr { expr, .. } => {
                self.resolve_expr(scope_file_id, scopes, body, expr, false);
            }
            Stmt::If(if_stmt) => {
                self.resolve_if_stmt(
                    scope_file_id,
                    scopes,
                    body,
                    &if_stmt.node,
                );
            }
            Stmt::Guard(guard_stmt) => {
                scopes.push();
                self.resolve_clause_list(
                    scope_file_id,
                    scopes,
                    body,
                    &guard_stmt.node.clauses,
                );
                self.resolve_block(
                    scope_file_id,
                    scopes,
                    body,
                    &guard_stmt.node.else_block,
                );
                scopes.pop();
            }
            Stmt::While(while_stmt) => {
                scopes.push();
                self.resolve_clause_list(
                    scope_file_id,
                    scopes,
                    body,
                    &while_stmt.node.clauses,
                );
                self.resolve_block(
                    scope_file_id,
                    scopes,
                    body,
                    &while_stmt.node.body,
                );
                scopes.pop();
            }
            Stmt::For(for_stmt) => {
                self.resolve_for_stmt(
                    scope_file_id,
                    scopes,
                    body,
                    &for_stmt.node,
                );
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.resolve_expr(scope_file_id, scopes, body, expr, false);
                }
            }
            Stmt::Break | Stmt::Continue => {}
        }
    }

    fn resolve_if_stmt(
        &mut self,
        scope_file_id: FileId,
        scopes: &mut LocalScopeStack,
        body: &mut ResolvedBody,
        if_stmt: &IfStmt,
    ) {
        scopes.push();
        self.resolve_clause_list(scope_file_id, scopes, body, &if_stmt.clauses);
        self.resolve_block(scope_file_id, scopes, body, &if_stmt.then_branch);
        scopes.pop();

        if let Some(else_branch) = &if_stmt.else_branch {
            match else_branch {
                IfStmtElse::If(nested) => {
                    self.resolve_if_stmt(
                        scope_file_id,
                        scopes,
                        body,
                        &nested.node,
                    );
                }
                IfStmtElse::Block(block) => {
                    self.resolve_block(scope_file_id, scopes, body, block);
                }
            }
        }
    }

    fn resolve_for_stmt(
        &mut self,
        scope_file_id: FileId,
        scopes: &mut LocalScopeStack,
        body: &mut ResolvedBody,
        for_stmt: &ForStmt,
    ) {
        self.resolve_expr(
            scope_file_id,
            scopes,
            body,
            &for_stmt.iterator,
            false,
        );
        scopes.push();
        self.bind_pattern_into_scope(
            scopes,
            body,
            &for_stmt.pattern,
            PatternBindingHint::Pattern,
            LocalMutability::Immutable,
            None,
        );
        self.resolve_block(scope_file_id, scopes, body, &for_stmt.body);
        scopes.pop();
    }

    fn resolve_let_like_stmt(
        &mut self,
        scope_file_id: FileId,
        scopes: &mut LocalScopeStack,
        body: &mut ResolvedBody,
        pattern: &crate::frontend::ast::Spanned<Pattern>,
        ty: Option<&crate::frontend::ast::Spanned<AstType>>,
        value: Option<&crate::frontend::ast::Spanned<Expr>>,
        mutability: LocalMutability,
    ) {
        if let Some(value) = value {
            self.resolve_expr(scope_file_id, scopes, body, value, false);
        }
        let hint = if matches!(&pattern.node, Pattern::Identifier(_)) {
            PatternBindingHint::Local
        } else {
            PatternBindingHint::Pattern
        };
        let declared_type = ty.map(|annotation| {
            self.resolve_type_ref(&body.owner, scope_file_id, &annotation.node)
        });
        self.bind_pattern_into_scope(
            scopes,
            body,
            pattern,
            hint,
            mutability,
            declared_type,
        );
    }

    fn resolve_clause_list(
        &mut self,
        scope_file_id: FileId,
        scopes: &mut LocalScopeStack,
        body: &mut ResolvedBody,
        clauses: &crate::frontend::ast::ClauseList,
    ) {
        for clause in &clauses.clauses {
            match &clause.node {
                Clause::Expr(expr) => {
                    self.resolve_expr(scope_file_id, scopes, body, expr, false);
                }
                Clause::LetBinding(binding) | Clause::VarBinding(binding) => {
                    self.resolve_expr(
                        scope_file_id,
                        scopes,
                        body,
                        &binding.value,
                        false,
                    );
                    let hint = if matches!(
                        &binding.pattern.node,
                        Pattern::Identifier(_)
                    ) {
                        PatternBindingHint::Local
                    } else {
                        PatternBindingHint::Pattern
                    };
                    let mutability = match &clause.node {
                        Clause::LetBinding(_) => LocalMutability::Immutable,
                        Clause::VarBinding(_) => LocalMutability::Mutable,
                        Clause::Expr(_) => LocalMutability::Immutable,
                    };
                    self.bind_pattern_into_scope(
                        scopes,
                        body,
                        &binding.pattern,
                        hint,
                        mutability,
                        binding.ty.as_ref().map(|annotation| {
                            self.resolve_type_ref(
                                &body.owner,
                                scope_file_id,
                                &annotation.node,
                            )
                        }),
                    );
                }
            }
        }
    }

    fn resolve_expr(
        &mut self,
        scope_file_id: FileId,
        scopes: &mut LocalScopeStack,
        body: &mut ResolvedBody,
        expr: &crate::frontend::ast::Spanned<Expr>,
        is_assignment_target: bool,
    ) {
        if let Some(path) = Self::extract_namespace_path(&expr.node) {
            let resolved =
                self.resolve_identifier_path(scope_file_id, scopes, &path);
            self.record_reference(
                body,
                expr.span,
                path,
                resolved,
                is_assignment_target,
            );
            return;
        }

        match &expr.node {
            Expr::Identifier(name) => {
                let path = vec![name.clone()];
                let resolved =
                    self.resolve_identifier_path(scope_file_id, scopes, &path);
                self.record_reference(
                    body,
                    expr.span,
                    path,
                    resolved,
                    is_assignment_target,
                );
            }
            Expr::SelfValue => {
                let path = vec!["self".to_string()];
                let resolved =
                    self.resolve_identifier_path(scope_file_id, scopes, &path);
                self.record_reference(
                    body,
                    expr.span,
                    path,
                    resolved,
                    is_assignment_target,
                );
            }
            Expr::QualifiedMember { qualifier, .. } => {
                self.resolve_expr(
                    scope_file_id,
                    scopes,
                    body,
                    qualifier,
                    false,
                );
            }
            Expr::Grouped(inner)
            | Expr::Try { expr: inner }
            | Expr::ForceUnwrap { expr: inner }
            | Expr::Spread { expr: inner }
            | Expr::Unary { expr: inner, .. } => {
                self.resolve_expr(
                    scope_file_id,
                    scopes,
                    body,
                    inner,
                    is_assignment_target,
                );
            }
            Expr::ArrayLiteral(elements) => {
                for element in elements {
                    match element {
                        ArrayElement::Expr(expr)
                        | ArrayElement::Spread(expr) => {
                            self.resolve_expr(
                                scope_file_id,
                                scopes,
                                body,
                                expr,
                                false,
                            );
                        }
                    }
                }
            }
            Expr::StructLiteral { ty, fields } => {
                if let TypeExpr::Path(path) = ty {
                    let resolved =
                        self.resolve_top_level_path(scope_file_id, path);
                    self.record_reference(
                        body,
                        expr.span,
                        path.clone(),
                        resolved.unwrap_or(ResolvedBodyRef::Unresolved),
                        false,
                    );
                }
                for field in fields {
                    match field {
                        StructLiteralField::Shorthand { name } => {
                            let path = vec![name.clone()];
                            let resolved = self.resolve_identifier_path(
                                scope_file_id,
                                scopes,
                                &path,
                            );
                            self.record_reference(
                                body, expr.span, path, resolved, false,
                            );
                        }
                        StructLiteralField::Named { value, .. }
                        | StructLiteralField::Spread { value } => {
                            self.resolve_expr(
                                scope_file_id,
                                scopes,
                                body,
                                value,
                                false,
                            );
                        }
                    }
                }
            }
            Expr::Block(block) | Expr::UnsafeBlock(block) => {
                self.resolve_block(scope_file_id, scopes, body, block);
            }
            Expr::If {
                clauses,
                then_branch,
                else_branch,
            } => {
                scopes.push();
                self.resolve_clause_list(scope_file_id, scopes, body, clauses);
                self.resolve_block(scope_file_id, scopes, body, then_branch);
                scopes.pop();
                if let Some(else_branch) = else_branch {
                    self.resolve_expr(
                        scope_file_id,
                        scopes,
                        body,
                        else_branch,
                        false,
                    );
                }
            }
            Expr::Match { subject, arms } => {
                self.resolve_expr(scope_file_id, scopes, body, subject, false);
                for arm in arms {
                    scopes.push();
                    self.bind_pattern_into_scope(
                        scopes,
                        body,
                        &arm.node.pattern,
                        PatternBindingHint::Pattern,
                        LocalMutability::Immutable,
                        None,
                    );
                    match &arm.node.body {
                        MatchArmBody::Expr(expr) => {
                            self.resolve_expr(
                                scope_file_id,
                                scopes,
                                body,
                                expr,
                                false,
                            );
                        }
                        MatchArmBody::Block(block) => {
                            self.resolve_block(
                                scope_file_id,
                                scopes,
                                body,
                                block,
                            );
                        }
                    }
                    scopes.pop();
                }
            }
            Expr::Closure {
                params,
                body: block,
                ..
            } => {
                scopes.push();
                for param in params {
                    self.declare_local(
                        scopes,
                        &mut body.locals,
                        param.name.clone(),
                        LocalKind::Parameter,
                        LocalMutability::Immutable,
                        param.ty.as_ref().map(|annotation| {
                            self.resolve_type_ref(
                                &body.owner,
                                scope_file_id,
                                &annotation.node,
                            )
                        }),
                        expr.span,
                    );
                }
                self.resolve_block(scope_file_id, scopes, body, block);
                scopes.pop();
            }
            Expr::Macro { args, .. } => match args {
                crate::frontend::ast::MacroExprArgs::Paren(args) => {
                    for arg in args {
                        self.resolve_expr(
                            scope_file_id,
                            scopes,
                            body,
                            &arg.value,
                            false,
                        );
                    }
                }
                crate::frontend::ast::MacroExprArgs::Braced(_) => {}
            },
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.resolve_expr(
                    scope_file_id,
                    scopes,
                    body,
                    condition,
                    false,
                );
                self.resolve_expr(
                    scope_file_id,
                    scopes,
                    body,
                    then_expr,
                    false,
                );
                self.resolve_expr(
                    scope_file_id,
                    scopes,
                    body,
                    else_expr,
                    false,
                );
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.resolve_expr(scope_file_id, scopes, body, lhs, false);
                self.resolve_expr(scope_file_id, scopes, body, rhs, false);
            }
            Expr::Assignment { target, value, .. } => {
                self.resolve_expr(scope_file_id, scopes, body, target, true);
                self.resolve_expr(scope_file_id, scopes, body, value, false);
            }
            Expr::MemberAccess { base, .. }
            | Expr::OptionalMemberAccess { base, .. } => {
                self.resolve_expr(
                    scope_file_id,
                    scopes,
                    body,
                    base,
                    is_assignment_target,
                );
            }
            Expr::NamespaceAccess { base, .. } => {
                self.resolve_expr(
                    scope_file_id,
                    scopes,
                    body,
                    base,
                    is_assignment_target,
                );
            }
            Expr::Call {
                callee,
                args,
                trailing_closure,
            } => {
                self.resolve_expr(scope_file_id, scopes, body, callee, false);
                for arg in args {
                    self.resolve_expr(
                        scope_file_id,
                        scopes,
                        body,
                        &arg.value,
                        false,
                    );
                }
                if let Some(trailing) = trailing_closure {
                    self.resolve_expr(
                        scope_file_id,
                        scopes,
                        body,
                        trailing,
                        false,
                    );
                }
            }
            Expr::Index { base, index }
            | Expr::OptionalIndex { base, index } => {
                self.resolve_expr(scope_file_id, scopes, body, base, false);
                self.resolve_expr(scope_file_id, scopes, body, index, false);
            }
            Expr::Cast { expr, .. } => {
                self.resolve_expr(scope_file_id, scopes, body, expr, false);
            }
            Expr::Range { start, end, .. } => {
                if let Some(start) = start {
                    self.resolve_expr(
                        scope_file_id,
                        scopes,
                        body,
                        start,
                        false,
                    );
                }
                if let Some(end) = end {
                    self.resolve_expr(scope_file_id, scopes, body, end, false);
                }
            }
            Expr::IntegerLiteral(_)
            | Expr::FloatLiteral(_)
            | Expr::CharLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::SelfType
            | Expr::ShorthandMember { .. } => {}
        }
    }

    fn resolve_identifier_path(
        &self,
        scope_file_id: FileId,
        scopes: &LocalScopeStack,
        segments: &[String],
    ) -> ResolvedBodyRef {
        if segments.is_empty() {
            return ResolvedBodyRef::Unresolved;
        }

        if let Some(local_id) = scopes.lookup(&segments[0]) {
            if segments.len() == 1 {
                return ResolvedBodyRef::Local(local_id);
            }
            return ResolvedBodyRef::Unresolved;
        }

        self.resolve_top_level_path(scope_file_id, segments)
            .unwrap_or(ResolvedBodyRef::Unresolved)
    }

    fn resolve_top_level_path(
        &self,
        scope_file_id: FileId,
        segments: &[String],
    ) -> Option<ResolvedBodyRef> {
        let first = segments.first()?;

        if let Some(binding) = self
            .imports
            .get(&scope_file_id)
            .and_then(|imports| imports.get(first))
        {
            let full_path = if segments.len() == 1 {
                Some(binding.target_path.clone())
            } else {
                match binding.kind {
                    ImportBindingKind::Scope => {
                        let mut path = binding.target_path.clone();
                        path.extend(segments.iter().skip(1).cloned());
                        Some(path)
                    }
                    ImportBindingKind::Symbol(_) => None,
                }
            };
            if let Some(full_path) = full_path
                && let Some(item_id) =
                    self.item_table.item_id_by_full_path(&full_path)
            {
                return Some(ResolvedBodyRef::Import(item_id));
            }
        }

        let scope = self.graph.scope(scope_file_id)?;
        let mut local_path = scope.scope_path.clone();
        local_path.extend(segments.iter().cloned());
        self.item_table
            .item_id_by_full_path(&local_path)
            .map(ResolvedBodyRef::Item)
    }

    fn record_reference(
        &self,
        body: &mut ResolvedBody,
        span: Span,
        segments: Vec<String>,
        resolved: ResolvedBodyRef,
        is_assignment_target: bool,
    ) {
        if matches!(resolved, ResolvedBodyRef::Unresolved) {
            body.unresolved_references.push(UnresolvedBodyReference {
                span,
                segments: segments.clone(),
            });
        }
        body.references.push(ResolvedBodyReference {
            span,
            segments,
            resolved,
            is_assignment_target,
        });
    }

    fn bind_pattern_into_scope(
        &mut self,
        scopes: &mut LocalScopeStack,
        locals: &mut ResolvedBody,
        pattern: &crate::frontend::ast::Spanned<Pattern>,
        hint: PatternBindingHint,
        mutability: LocalMutability,
        declared_type: Option<ResolvedTypeRef>,
    ) {
        match &pattern.node {
            Pattern::Identifier(name) => {
                let kind = match hint {
                    PatternBindingHint::Local => LocalKind::LocalBinding,
                    PatternBindingHint::Pattern => LocalKind::PatternBinding,
                };
                self.declare_local(
                    scopes,
                    &mut locals.locals,
                    name.clone(),
                    kind,
                    mutability,
                    declared_type,
                    pattern.span,
                );
            }
            Pattern::Tuple(elements) | Pattern::Array { elements, .. } => {
                for element in elements {
                    self.bind_pattern_into_scope(
                        scopes,
                        locals,
                        element,
                        PatternBindingHint::Pattern,
                        mutability,
                        None,
                    );
                }
                if let Pattern::Array { rest, .. } = &pattern.node
                    && let Some(crate::frontend::ast::ArrayPatternRest::Bind(
                        name,
                    )) = rest
                {
                    self.declare_local(
                        scopes,
                        &mut locals.locals,
                        name.clone(),
                        LocalKind::PatternBinding,
                        mutability,
                        None,
                        pattern.span,
                    );
                }
            }
            Pattern::Variant { args, .. } => {
                for arg in args {
                    self.bind_pattern_into_scope(
                        scopes,
                        locals,
                        arg,
                        PatternBindingHint::Pattern,
                        mutability,
                        None,
                    );
                }
            }
            Pattern::Struct { fields, .. } => {
                for field in fields {
                    match &field.pattern {
                        Some(pattern) => {
                            self.bind_pattern_into_scope(
                                scopes,
                                locals,
                                pattern,
                                PatternBindingHint::Pattern,
                                mutability,
                                None,
                            );
                        }
                        None => {
                            self.declare_local(
                                scopes,
                                &mut locals.locals,
                                field.name.clone(),
                                LocalKind::PatternBinding,
                                mutability,
                                None,
                                pattern.span,
                            );
                        }
                    }
                }
            }
            Pattern::Wildcard
            | Pattern::IntegerLiteral(_)
            | Pattern::BooleanLiteral(_)
            | Pattern::CharLiteral(_)
            | Pattern::StringLiteral(_) => {}
        }
    }

    fn declare_local(
        &mut self,
        scopes: &mut LocalScopeStack,
        locals: &mut Vec<ResolvedLocalBinding>,
        name: String,
        kind: LocalKind,
        mutability: LocalMutability,
        declared_type: Option<ResolvedTypeRef>,
        declared_span: Span,
    ) {
        let local_id = LocalId::new(self.next_local_id);
        self.next_local_id = self.next_local_id.saturating_add(1);
        locals.push(ResolvedLocalBinding {
            id: local_id,
            kind,
            mutability,
            name: name.clone(),
            declared_type,
            declared_span,
        });
        scopes.insert(name, local_id);
    }

    fn resolve_type_ref(
        &self,
        owner: &DeclarationOwner,
        scope_file_id: FileId,
        ty: &AstType,
    ) -> ResolvedTypeRef {
        match ty {
            AstType::Named { segments } => ResolvedTypeRef::Named {
                segments: segments.clone(),
                resolved: self.resolve_named_type_path(scope_file_id, segments),
            },
            AstType::GenericApplication { base, args } => {
                ResolvedTypeRef::GenericApplication {
                    base: Box::new(self.resolve_type_ref(
                        owner,
                        scope_file_id,
                        &base.node,
                    )),
                    args: args
                        .iter()
                        .map(|arg| {
                            self.resolve_type_ref(
                                owner,
                                scope_file_id,
                                &arg.node,
                            )
                        })
                        .collect(),
                }
            }
            AstType::SelfType => ResolvedTypeRef::SelfType,
            AstType::Reference(inner) => ResolvedTypeRef::Reference(Box::new(
                self.resolve_type_ref(owner, scope_file_id, &inner.node),
            )),
            AstType::MutableReference(inner) => {
                ResolvedTypeRef::MutableReference(Box::new(
                    self.resolve_type_ref(owner, scope_file_id, &inner.node),
                ))
            }
            AstType::ConstPointer(inner) => {
                ResolvedTypeRef::ConstPointer(Box::new(self.resolve_type_ref(
                    owner,
                    scope_file_id,
                    &inner.node,
                )))
            }
            AstType::MutablePointer(inner) => {
                ResolvedTypeRef::MutablePointer(Box::new(
                    self.resolve_type_ref(owner, scope_file_id, &inner.node),
                ))
            }
            AstType::Array(inner) => ResolvedTypeRef::Array(Box::new(
                self.resolve_type_ref(owner, scope_file_id, &inner.node),
            )),
            AstType::Optional(inner) => ResolvedTypeRef::Optional(Box::new(
                self.resolve_type_ref(owner, scope_file_id, &inner.node),
            )),
            AstType::Result { ok, err } => ResolvedTypeRef::Result {
                ok: Box::new(self.resolve_type_ref(
                    owner,
                    scope_file_id,
                    &ok.node,
                )),
                err: Box::new(self.resolve_type_ref(
                    owner,
                    scope_file_id,
                    &err.node,
                )),
            },
            AstType::Grouped(inner) => ResolvedTypeRef::Grouped(Box::new(
                self.resolve_type_ref(owner, scope_file_id, &inner.node),
            )),
        }
    }

    fn resolve_named_type_path(
        &self,
        scope_file_id: FileId,
        segments: &[String],
    ) -> Option<ResolvedItemRef> {
        let first = segments.first()?;

        if let Some(binding) = self
            .imports
            .get(&scope_file_id)
            .and_then(|imports| imports.get(first))
            && let Some(resolved) =
                self.resolve_named_path_from_import(binding, segments)
        {
            return Some(resolved);
        }

        let scope = self.graph.scope(scope_file_id)?;
        let mut local_full_path = scope.scope_path.clone();
        local_full_path.extend(segments.iter().cloned());
        self.item_ref_by_full_path(local_full_path)
    }

    fn resolve_named_path_from_import(
        &self,
        binding: &crate::frontend::resolver::ResolvedImportBinding,
        segments: &[String],
    ) -> Option<ResolvedItemRef> {
        if segments.len() == 1 {
            return self.item_ref_by_full_path(binding.target_path.clone());
        }

        match binding.kind {
            ImportBindingKind::Scope => {
                let mut full_path = binding.target_path.clone();
                full_path.extend(segments.iter().skip(1).cloned());
                self.item_ref_by_full_path(full_path)
            }
            ImportBindingKind::Symbol(_) => None,
        }
    }

    fn item_ref_by_full_path(
        &self,
        full_path: Vec<String>,
    ) -> Option<ResolvedItemRef> {
        let item_id = self.item_table.item_id_by_full_path(&full_path)?;
        Some(ResolvedItemRef { item_id, full_path })
    }

    fn extract_namespace_path(expr: &Expr) -> Option<Vec<String>> {
        match expr {
            Expr::Identifier(name) => Some(vec![name.clone()]),
            Expr::NamespaceAccess { base, member, .. } => {
                let mut path = Self::extract_namespace_path(&base.node)?;
                path.push(member.clone());
                Some(path)
            }
            _ => None,
        }
    }
}

trait ReceiverMutabilityExt {
    fn to_local_mutability_for_parameter(self) -> LocalMutability;
}

impl ReceiverMutabilityExt for ReceiverKind {
    fn to_local_mutability_for_parameter(self) -> LocalMutability {
        match self {
            ReceiverKind::MutRef => LocalMutability::Mutable,
            ReceiverKind::Owned | ReceiverKind::Ref => {
                LocalMutability::Immutable
            }
        }
    }
}

#[derive(Default)]
struct LocalScopeStack {
    frames: Vec<BTreeMap<String, LocalId>>,
}

impl LocalScopeStack {
    fn push(&mut self) {
        self.frames.push(BTreeMap::new());
    }

    fn pop(&mut self) {
        self.frames.pop();
    }

    fn insert(&mut self, name: String, local_id: LocalId) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name, local_id);
        }
    }

    fn lookup(&self, name: &str) -> Option<LocalId> {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
    }
}

#[derive(Clone, Copy)]
enum PatternBindingHint {
    Local,
    Pattern,
}
