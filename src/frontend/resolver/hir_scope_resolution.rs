use super::body_resolution::{LocalKind, LocalMutability};
use super::hir_item_table::HirItemRef;
use super::local_ids::LocalId;
use crate::frontend::hir::{
    HirArrayElement, HirBodyId, HirClosureParam, HirExprId, HirExprKind,
    HirFile, HirFunctionParam, HirItemId, HirItemKind, HirMatchArm, HirModule,
    HirMutability, HirPatId, HirPatKind, HirStmtId, HirStmtKind,
    HirStructExprField,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

/// File-scoped reference to one lowered HIR expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HirExprRef {
    pub file_id: FileId,
    pub expr_id: HirExprId,
}

impl HirExprRef {
    #[must_use]
    pub const fn new(file_id: FileId, expr_id: HirExprId) -> Self {
        Self { file_id, expr_id }
    }
}

/// File-scoped reference to one lowered HIR pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HirPatRef {
    pub file_id: FileId,
    pub pat_id: HirPatId,
}

impl HirPatRef {
    #[must_use]
    pub const fn new(file_id: FileId, pat_id: HirPatId) -> Self {
        Self { file_id, pat_id }
    }
}

/// Resolved local binding record produced from HIR scope resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirLocalBinding {
    pub id: LocalId,
    pub file_id: FileId,
    pub body_id: HirBodyId,
    pub name: String,
    pub kind: LocalKind,
    pub mutability: LocalMutability,
    pub declared_pat: Option<HirPatId>,
}

/// HIR-local scope resolution failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirScopeResolutionError {
    MissingModule { file_id: FileId },
    MissingItem { item_ref: HirItemRef },
    MissingBody { file_id: FileId, body_id: HirBodyId },
    MissingStmt { file_id: FileId, stmt_id: HirStmtId },
    MissingExpr { file_id: FileId, expr_id: HirExprId },
    MissingPattern { file_id: FileId, pat_id: HirPatId },
}

impl Display for HirScopeResolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingModule { file_id } => {
                write!(f, "missing HIR module for file id {}", file_id.raw())
            }
            Self::MissingItem { item_ref } => write!(
                f,
                "missing HIR item {} in file id {}",
                item_ref.item_id.raw(),
                item_ref.file_id.raw()
            ),
            Self::MissingBody { file_id, body_id } => write!(
                f,
                "missing HIR body {} in file id {}",
                body_id.raw(),
                file_id.raw()
            ),
            Self::MissingStmt { file_id, stmt_id } => write!(
                f,
                "missing HIR stmt {} in file id {}",
                stmt_id.raw(),
                file_id.raw()
            ),
            Self::MissingExpr { file_id, expr_id } => write!(
                f,
                "missing HIR expr {} in file id {}",
                expr_id.raw(),
                file_id.raw()
            ),
            Self::MissingPattern { file_id, pat_id } => write!(
                f,
                "missing HIR pattern {} in file id {}",
                pat_id.raw(),
                file_id.raw()
            ),
        }
    }
}

impl std::error::Error for HirScopeResolutionError {}

/// Deterministic HIR local binding table.
///
/// Tracks lexical local bindings and local references without resolving global
/// item paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirLocalBindingTable {
    bindings_by_id: BTreeMap<LocalId, HirLocalBinding>,
    pat_to_binding: BTreeMap<HirPatRef, LocalId>,
    expr_to_binding: BTreeMap<HirExprRef, LocalId>,
    by_file_id: BTreeMap<FileId, Vec<LocalId>>,
}

impl HirLocalBindingTable {
    /// Resolves HIR lexical scopes and local binding references.
    ///
    /// Handles function parameters, local `let`/`var` bindings, block scopes,
    /// and shadowing.
    ///
    /// # Errors
    ///
    /// Returns an error when a referenced HIR node id is missing from the
    /// corresponding module.
    pub fn collect(
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
    ) -> Result<Self, HirScopeResolutionError> {
        HirScopeResolver::new(hir_modules).collect(hir_files)
    }

    #[must_use]
    pub fn binding(&self, binding_id: LocalId) -> Option<&HirLocalBinding> {
        self.bindings_by_id.get(&binding_id)
    }

    #[must_use]
    pub fn binding_for_pat(
        &self,
        file_id: FileId,
        pat_id: HirPatId,
    ) -> Option<LocalId> {
        self.pat_to_binding
            .get(&HirPatRef::new(file_id, pat_id))
            .copied()
    }

    #[must_use]
    pub fn binding_for_expr(
        &self,
        file_id: FileId,
        expr_id: HirExprId,
    ) -> Option<LocalId> {
        self.expr_to_binding
            .get(&HirExprRef::new(file_id, expr_id))
            .copied()
    }

    #[must_use]
    pub fn binding_ids_in_file(&self, file_id: FileId) -> &[LocalId] {
        self.by_file_id
            .get(&file_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn iter_bindings(&self) -> impl Iterator<Item = &HirLocalBinding> {
        self.bindings_by_id.values()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings_by_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings_by_id.is_empty()
    }
}

/// Builds a [`HirLocalBindingTable`] from lowered project HIR files/modules.
///
/// # Errors
///
/// Propagates any collection failure from [`HirLocalBindingTable::collect`].
pub fn build_hir_local_binding_table(
    hir_files: &[HirFile],
    hir_modules: &BTreeMap<FileId, HirModule>,
) -> Result<HirLocalBindingTable, HirScopeResolutionError> {
    HirLocalBindingTable::collect(hir_files, hir_modules)
}

struct HirScopeResolver<'a> {
    hir_modules: &'a BTreeMap<FileId, HirModule>,
    bindings_by_id: BTreeMap<LocalId, HirLocalBinding>,
    pat_to_binding: BTreeMap<HirPatRef, LocalId>,
    expr_to_binding: BTreeMap<HirExprRef, LocalId>,
    by_file_id: BTreeMap<FileId, Vec<LocalId>>,
    scope_stack: LocalScopeStack,
    next_local_raw: u32,
}

impl<'a> HirScopeResolver<'a> {
    fn new(hir_modules: &'a BTreeMap<FileId, HirModule>) -> Self {
        Self {
            hir_modules,
            bindings_by_id: BTreeMap::new(),
            pat_to_binding: BTreeMap::new(),
            expr_to_binding: BTreeMap::new(),
            by_file_id: BTreeMap::new(),
            scope_stack: LocalScopeStack::default(),
            next_local_raw: 0,
        }
    }

    fn collect(
        mut self,
        hir_files: &[HirFile],
    ) -> Result<HirLocalBindingTable, HirScopeResolutionError> {
        for hir_file in hir_files {
            let module = self.hir_modules.get(&hir_file.file_id).ok_or(
                HirScopeResolutionError::MissingModule {
                    file_id: hir_file.file_id,
                },
            )?;
            for item_id in &hir_file.root_items {
                self.resolve_item(hir_file.file_id, module, *item_id)?;
            }
        }

        Ok(HirLocalBindingTable {
            bindings_by_id: self.bindings_by_id,
            pat_to_binding: self.pat_to_binding,
            expr_to_binding: self.expr_to_binding,
            by_file_id: self.by_file_id,
        })
    }

    fn resolve_item(
        &mut self,
        file_id: FileId,
        module: &HirModule,
        item_id: HirItemId,
    ) -> Result<(), HirScopeResolutionError> {
        let item_ref = HirItemRef::new(file_id, item_id);
        let item = module
            .items
            .get(&item_id)
            .ok_or(HirScopeResolutionError::MissingItem { item_ref })?;

        match &item.kind {
            HirItemKind::Function(function) => self.resolve_function(
                file_id,
                module,
                function.body,
                &function.signature.params,
            )?,
            HirItemKind::Struct(struct_decl) => {
                for function in &struct_decl.functions {
                    self.resolve_function(
                        file_id,
                        module,
                        function.body,
                        &function.signature.params,
                    )?;
                }
            }
            HirItemKind::Enum(enum_decl) => {
                for function in &enum_decl.functions {
                    self.resolve_function(
                        file_id,
                        module,
                        function.body,
                        &function.signature.params,
                    )?;
                }
            }
            HirItemKind::Protocol(protocol_decl) => {
                for function in &protocol_decl.functions {
                    if let Some(default_body) = function.default_body {
                        self.resolve_function(
                            file_id,
                            module,
                            default_body,
                            &function.signature.params,
                        )?;
                    }
                }
            }
            HirItemKind::Impl(impl_decl) => {
                for function in &impl_decl.functions {
                    self.resolve_function(
                        file_id,
                        module,
                        function.body,
                        &function.signature.params,
                    )?;
                }
            }
            HirItemKind::Extern(_) | HirItemKind::Use(_) => {}
        }

        Ok(())
    }

    fn resolve_function(
        &mut self,
        file_id: FileId,
        module: &HirModule,
        body_id: HirBodyId,
        params: &[HirFunctionParam],
    ) -> Result<(), HirScopeResolutionError> {
        self.scope_stack.push();
        for param in params {
            self.declare_binding(
                file_id,
                body_id,
                param.name.clone(),
                LocalKind::Parameter,
                LocalMutability::Immutable,
                None,
            );
        }
        self.resolve_body(file_id, module, body_id)?;
        self.scope_stack.pop();
        Ok(())
    }

    fn resolve_body(
        &mut self,
        file_id: FileId,
        module: &HirModule,
        body_id: HirBodyId,
    ) -> Result<(), HirScopeResolutionError> {
        let body = module
            .bodies
            .get(&body_id)
            .ok_or(HirScopeResolutionError::MissingBody { file_id, body_id })?;

        self.scope_stack.push();
        for stmt_id in &body.stmts {
            self.resolve_stmt(file_id, module, body_id, *stmt_id)?;
        }
        if let Some(tail_expr) = body.tail_expr {
            self.resolve_expr(file_id, module, body_id, tail_expr)?;
        }
        self.scope_stack.pop();
        Ok(())
    }

    fn resolve_stmt(
        &mut self,
        file_id: FileId,
        module: &HirModule,
        body_id: HirBodyId,
        stmt_id: HirStmtId,
    ) -> Result<(), HirScopeResolutionError> {
        let stmt = module
            .stmts
            .get(&stmt_id)
            .ok_or(HirScopeResolutionError::MissingStmt { file_id, stmt_id })?;

        match &stmt.kind {
            HirStmtKind::Let(let_stmt) => {
                if let Some(value) = let_stmt.value {
                    self.resolve_expr(file_id, module, body_id, value)?;
                }
                let hint = match module.patterns.get(&let_stmt.pat).ok_or(
                    HirScopeResolutionError::MissingPattern {
                        file_id,
                        pat_id: let_stmt.pat,
                    },
                )? {
                    pat if matches!(pat.kind, HirPatKind::Binding { .. }) => {
                        PatternBindingHint::Local
                    }
                    _ => PatternBindingHint::Pattern,
                };
                self.bind_pattern(
                    file_id,
                    module,
                    body_id,
                    let_stmt.pat,
                    hint,
                    local_mutability(let_stmt.mutability),
                )?;
            }
            HirStmtKind::Expr { expr } | HirStmtKind::Semi { expr } => {
                self.resolve_expr(file_id, module, body_id, *expr)?;
            }
            HirStmtKind::Item { item } => {
                self.resolve_item(file_id, module, *item)?;
            }
        }

        Ok(())
    }

    fn resolve_expr(
        &mut self,
        file_id: FileId,
        module: &HirModule,
        body_id: HirBodyId,
        expr_id: HirExprId,
    ) -> Result<(), HirScopeResolutionError> {
        let expr = module
            .exprs
            .get(&expr_id)
            .ok_or(HirScopeResolutionError::MissingExpr { file_id, expr_id })?;

        match &expr.kind {
            HirExprKind::Path(path) => {
                if path.segments.len() == 1
                    && let Some(binding_id) =
                        self.scope_stack.lookup(&path.segments[0])
                {
                    self.expr_to_binding
                        .insert(HirExprRef::new(file_id, expr_id), binding_id);
                }
            }
            HirExprKind::Array { elements } => {
                for element in elements {
                    match element {
                        HirArrayElement::Expr(inner)
                        | HirArrayElement::Spread(inner) => {
                            self.resolve_expr(
                                file_id, module, body_id, *inner,
                            )?;
                        }
                    }
                }
            }
            HirExprKind::Call { callee, args } => {
                self.resolve_expr(file_id, module, body_id, *callee)?;
                for arg in args {
                    self.resolve_expr(file_id, module, body_id, arg.value)?;
                }
            }
            HirExprKind::Block { body } => {
                self.resolve_body(file_id, module, *body)?;
            }
            HirExprKind::If {
                condition,
                then_body,
                else_expr,
            } => {
                self.resolve_expr(file_id, module, body_id, *condition)?;
                self.resolve_body(file_id, module, *then_body)?;
                if let Some(else_expr) = else_expr {
                    self.resolve_expr(file_id, module, body_id, *else_expr)?;
                }
            }
            HirExprKind::While { condition, body } => {
                self.resolve_expr(file_id, module, body_id, *condition)?;
                self.resolve_body(file_id, module, *body)?;
            }
            HirExprKind::For {
                pat,
                iterator,
                body,
            } => {
                self.resolve_expr(file_id, module, body_id, *iterator)?;
                self.scope_stack.push();
                self.bind_pattern(
                    file_id,
                    module,
                    body_id,
                    *pat,
                    PatternBindingHint::Pattern,
                    LocalMutability::Immutable,
                )?;
                self.resolve_body(file_id, module, *body)?;
                self.scope_stack.pop();
            }
            HirExprKind::Return { value } => {
                if let Some(value) = value {
                    self.resolve_expr(file_id, module, body_id, *value)?;
                }
            }
            HirExprKind::Assign { target, value, .. } => {
                self.resolve_expr(file_id, module, body_id, *target)?;
                self.resolve_expr(file_id, module, body_id, *value)?;
            }
            HirExprKind::Unary { expr, .. }
            | HirExprKind::ForceUnwrap { expr }
            | HirExprKind::Spread { expr }
            | HirExprKind::Try { expr } => {
                self.resolve_expr(file_id, module, body_id, *expr)?;
            }
            HirExprKind::Binary { lhs, rhs, .. } => {
                self.resolve_expr(file_id, module, body_id, *lhs)?;
                self.resolve_expr(file_id, module, body_id, *rhs)?;
            }
            HirExprKind::Field { base, .. }
            | HirExprKind::OptionalField { base, .. }
            | HirExprKind::NamespaceField { base, .. } => {
                self.resolve_expr(file_id, module, body_id, *base)?;
            }
            HirExprKind::Index { base, index }
            | HirExprKind::OptionalIndex { base, index } => {
                self.resolve_expr(file_id, module, body_id, *base)?;
                self.resolve_expr(file_id, module, body_id, *index)?;
            }
            HirExprKind::Tuple { elements } => {
                for element in elements {
                    self.resolve_expr(file_id, module, body_id, *element)?;
                }
            }
            HirExprKind::Struct { fields, .. } => {
                for field in fields {
                    match field {
                        HirStructExprField::Named { value, .. }
                        | HirStructExprField::Spread { value } => {
                            self.resolve_expr(
                                file_id, module, body_id, *value,
                            )?;
                        }
                    }
                }
            }
            HirExprKind::Match { subject, arms } => {
                self.resolve_expr(file_id, module, body_id, *subject)?;
                for arm in arms {
                    self.resolve_match_arm(file_id, module, body_id, arm)?;
                }
            }
            HirExprKind::Closure { params, body, .. } => {
                self.scope_stack.push();
                for param in params {
                    self.declare_closure_param(file_id, *body, param);
                }
                self.resolve_body(file_id, module, *body)?;
                self.scope_stack.pop();
            }
            HirExprKind::Cast { expr, .. } => {
                self.resolve_expr(file_id, module, body_id, *expr)?;
            }
            HirExprKind::Range { start, end, .. } => {
                if let Some(start) = start {
                    self.resolve_expr(file_id, module, body_id, *start)?;
                }
                if let Some(end) = end {
                    self.resolve_expr(file_id, module, body_id, *end)?;
                }
            }
            HirExprKind::MethodCall { receiver, args, .. } => {
                self.resolve_expr(file_id, module, body_id, *receiver)?;
                for arg in args {
                    self.resolve_expr(file_id, module, body_id, arg.value)?;
                }
            }
            HirExprKind::Literal(_)
            | HirExprKind::Break
            | HirExprKind::Continue => {}
        }

        Ok(())
    }

    fn resolve_match_arm(
        &mut self,
        file_id: FileId,
        module: &HirModule,
        body_id: HirBodyId,
        arm: &HirMatchArm,
    ) -> Result<(), HirScopeResolutionError> {
        self.scope_stack.push();
        self.bind_pattern(
            file_id,
            module,
            body_id,
            arm.pat,
            PatternBindingHint::Pattern,
            LocalMutability::Immutable,
        )?;
        self.resolve_expr(file_id, module, body_id, arm.expr)?;
        self.scope_stack.pop();
        Ok(())
    }

    fn declare_closure_param(
        &mut self,
        file_id: FileId,
        body_id: HirBodyId,
        param: &HirClosureParam,
    ) {
        self.declare_binding(
            file_id,
            body_id,
            param.name.clone(),
            LocalKind::Parameter,
            LocalMutability::Immutable,
            None,
        );
    }

    fn bind_pattern(
        &mut self,
        file_id: FileId,
        module: &HirModule,
        body_id: HirBodyId,
        pat_id: HirPatId,
        hint: PatternBindingHint,
        mutability: LocalMutability,
    ) -> Result<(), HirScopeResolutionError> {
        let pattern = module.patterns.get(&pat_id).ok_or(
            HirScopeResolutionError::MissingPattern { file_id, pat_id },
        )?;

        match &pattern.kind {
            HirPatKind::Binding { name } => {
                let kind = match hint {
                    PatternBindingHint::Local => LocalKind::LocalBinding,
                    PatternBindingHint::Pattern => LocalKind::PatternBinding,
                };
                self.declare_binding(
                    file_id,
                    body_id,
                    name.clone(),
                    kind,
                    mutability,
                    Some(pat_id),
                );
            }
            HirPatKind::Tuple { elements } => {
                for element in elements {
                    self.bind_pattern(
                        file_id,
                        module,
                        body_id,
                        *element,
                        PatternBindingHint::Pattern,
                        mutability,
                    )?;
                }
            }
            HirPatKind::Struct { fields, .. } => {
                for field in fields {
                    if let Some(field_pat) = field.pat {
                        self.bind_pattern(
                            file_id,
                            module,
                            body_id,
                            field_pat,
                            PatternBindingHint::Pattern,
                            mutability,
                        )?;
                    } else {
                        self.declare_binding(
                            file_id,
                            body_id,
                            field.name.clone(),
                            LocalKind::PatternBinding,
                            mutability,
                            None,
                        );
                    }
                }
            }
            HirPatKind::EnumVariant { args, .. } => {
                for arg in args {
                    self.bind_pattern(
                        file_id,
                        module,
                        body_id,
                        *arg,
                        PatternBindingHint::Pattern,
                        mutability,
                    )?;
                }
            }
            HirPatKind::Wildcard | HirPatKind::Literal(_) => {}
        }

        Ok(())
    }

    fn declare_binding(
        &mut self,
        file_id: FileId,
        body_id: HirBodyId,
        name: String,
        kind: LocalKind,
        mutability: LocalMutability,
        declared_pat: Option<HirPatId>,
    ) {
        let binding_id = LocalId::new(self.next_local_raw);
        self.next_local_raw = self
            .next_local_raw
            .checked_add(1)
            .expect("HIR local binding id overflow");

        self.scope_stack.insert(name.clone(), binding_id);
        self.by_file_id.entry(file_id).or_default().push(binding_id);
        self.bindings_by_id.insert(
            binding_id,
            HirLocalBinding {
                id: binding_id,
                file_id,
                body_id,
                name,
                kind,
                mutability,
                declared_pat,
            },
        );
        if let Some(pat_id) = declared_pat {
            self.pat_to_binding
                .insert(HirPatRef::new(file_id, pat_id), binding_id);
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

    fn insert(&mut self, name: String, binding_id: LocalId) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name, binding_id);
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

const fn local_mutability(mutability: HirMutability) -> LocalMutability {
    match mutability {
        HirMutability::Immutable => LocalMutability::Immutable,
        HirMutability::Mutable => LocalMutability::Mutable,
    }
}
