use super::ids::{
    HirBodyId, HirExprId, HirItemId, HirPatId, HirStmtId, HirTypeId,
};
use super::nodes::{HirBody, HirExpr, HirItem, HirPat, HirStmt, HirType};
use std::collections::BTreeMap;

/// Deterministic storage for all lowered HIR nodes in one file lowering unit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HirModule {
    pub items: BTreeMap<HirItemId, HirItem>,
    pub exprs: BTreeMap<HirExprId, HirExpr>,
    pub stmts: BTreeMap<HirStmtId, HirStmt>,
    pub types: BTreeMap<HirTypeId, HirType>,
    pub patterns: BTreeMap<HirPatId, HirPat>,
    pub bodies: BTreeMap<HirBodyId, HirBody>,
    next_item_raw: u32,
    next_expr_raw: u32,
    next_stmt_raw: u32,
    next_type_raw: u32,
    next_pat_raw: u32,
    next_body_raw: u32,
}

impl HirModule {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates an item node and returns its stable id.
    pub fn alloc_item(&mut self, item: HirItem) -> HirItemId {
        let id = HirItemId::new(next_raw_id(&mut self.next_item_raw));
        self.items.insert(id, item);
        id
    }

    /// Allocates an expression node and returns its stable id.
    pub fn alloc_expr(&mut self, expr: HirExpr) -> HirExprId {
        let id = HirExprId::new(next_raw_id(&mut self.next_expr_raw));
        self.exprs.insert(id, expr);
        id
    }

    /// Allocates a statement node and returns its stable id.
    pub fn alloc_stmt(&mut self, stmt: HirStmt) -> HirStmtId {
        let id = HirStmtId::new(next_raw_id(&mut self.next_stmt_raw));
        self.stmts.insert(id, stmt);
        id
    }

    /// Allocates a type node and returns its stable id.
    pub fn alloc_type(&mut self, ty: HirType) -> HirTypeId {
        let id = HirTypeId::new(next_raw_id(&mut self.next_type_raw));
        self.types.insert(id, ty);
        id
    }

    /// Allocates a pattern node and returns its stable id.
    pub fn alloc_pat(&mut self, pat: HirPat) -> HirPatId {
        let id = HirPatId::new(next_raw_id(&mut self.next_pat_raw));
        self.patterns.insert(id, pat);
        id
    }

    /// Allocates a body node and returns its stable id.
    pub fn alloc_body(&mut self, body: HirBody) -> HirBodyId {
        let id = HirBodyId::new(next_raw_id(&mut self.next_body_raw));
        self.bodies.insert(id, body);
        id
    }
}

fn next_raw_id(next: &mut u32) -> u32 {
    let id = *next;
    *next = next.checked_add(1).expect("HIR id overflow");
    id
}
