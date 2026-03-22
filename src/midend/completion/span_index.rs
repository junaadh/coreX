//! HIR span indexing for efficient cursor-to-node lookup.
//!
//! This module provides efficient O(log n) lookup of HIR nodes at cursor positions
//! by maintaining a sorted index of spans. This is essential for responsive code
//! completion in large files.

use crate::frontend::ast::Span;
use crate::frontend::hir::{
    HirBodyId, HirExprId, HirExprKind, HirFile, HirItemId, HirItemKind,
    HirModule, HirOrigin, HirStmtId, HirStmtKind,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

/// An entry in the span index mapping a span to an HIR node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SpanIndexEntry {
    /// The start offset of the span.
    pub start: usize,

    /// The end offset of the span.
    pub end: usize,

    /// The HIR node identifier.
    pub node_id: HirNodeId,
}

/// Identifier for different kinds of HIR nodes in the span index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HirNodeId {
    /// An expression node.
    Expr(HirExprId),

    /// A statement node.
    Stmt(HirStmtId),

    /// An item node (top-level declaration).
    Item(HirItemId),

    /// A body node (block of statements).
    Body(HirBodyId),
}

/// Index of HIR nodes by their source spans.
///
/// This index enables efficient O(log n) lookup of HIR nodes at cursor positions.
/// The index is built once and can be queried multiple times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirSpanIndex {
    /// Sorted entries by (start, end) for binary search.
    entries: Vec<SpanIndexEntry>,

    /// Map from body IDs to their containing file for cross-referencing.
    bodies_to_files: BTreeMap<HirBodyId, FileId>,
}

impl HirSpanIndex {
    /// Build a span index from a HIR file and module.
    #[must_use]
    pub fn from_hir_file(
        file_id: FileId,
        hir_file: &HirFile,
        hir_module: &HirModule,
    ) -> Self {
        let mut index = Self {
            entries: Vec::new(),
            bodies_to_files: BTreeMap::new(),
        };

        // Index all items in the file
        for item_id in &hir_file.root_items {
            if let Some(item) = hir_module.items.get(item_id) {
                index.index_item(hir_module, *item_id, item);
            }
        }

        // Sort entries for binary search
        index.entries.sort_by_key(|e| (e.start, e.end));

        index
    }

    /// Find the innermost HIR node containing the given offset.
    ///
    /// Returns the most specific (deepest) node that contains the offset.
    /// If multiple nodes contain the offset, returns the smallest one.
    ///
    /// # Arguments
    /// * `offset` - The cursor offset in bytes
    ///
    /// # Returns
    /// * `Some((node_id, origin))` if a node is found
    /// * `None` if no node contains the offset
    #[must_use]
    pub fn find_node_at_offset(
        &self,
        offset: usize,
    ) -> Option<(HirNodeId, HirOrigin)> {
        // Find the last entry that starts at or before the offset
        let idx = self.entries.partition_point(|e| {
            e.start < offset || (e.start == offset && e.end < offset)
        });

        // Search backwards for the smallest containing span
        let mut best_entry: Option<&SpanIndexEntry> = None;

        for i in (0..idx).rev() {
            let entry = &self.entries[i];
            if entry.start <= offset && offset <= entry.end {
                // This node contains the offset
                match &best_entry {
                    Some(best)
                        if entry.end - entry.start < best.end - best.start =>
                    {
                        // This is a smaller (more specific) node
                        best_entry = Some(entry);
                    }
                    None => {
                        best_entry = Some(entry);
                    }
                    _ => {}
                }
            } else if entry.start > offset {
                // We've gone too far back (entries before this start even earlier)
                break;
            }
        }

        best_entry.map(|entry| {
            let origin = self.get_origin_for_node(entry.node_id)?;
            Some((entry.node_id, origin))
        })?
    }

    /// Find the HIR node immediately before the given offset.
    ///
    /// This is useful for completion when the cursor is at the end of a prefix.
    /// For example, in `foo::|`, this would return the path expression node.
    ///
    /// # Arguments
    /// * `offset` - The cursor offset in bytes
    ///
    /// # Returns
    /// * `Some((node_id, origin))` if a node is found
    /// * `None` if no node is before the offset
    #[must_use]
    pub fn find_node_before_offset(
        &self,
        offset: usize,
    ) -> Option<(HirNodeId, HirOrigin)> {
        // Find the last entry that ends before the offset
        let idx = self.entries.partition_point(|e| e.end < offset);

        if idx == 0 {
            return None;
        }

        // Search backwards from idx-1 for the closest node
        for i in (0..idx).rev() {
            let entry = &self.entries[i];
            if entry.end <= offset {
                let origin = self.get_origin_for_node(entry.node_id)?;
                return Some((entry.node_id, origin));
            }
        }

        None
    }

    /// Get the origin (file and span) for a given node ID.
    fn get_origin_for_node(&self, node_id: HirNodeId) -> Option<HirOrigin> {
        self.entries.iter().find(|e| e.node_id == node_id).map(|e| {
            HirOrigin::direct_source(FileId::new(0), Span::new(e.start, e.end))
        })
    }

    /// Index a single HIR item and all its children.
    fn index_item(
        &mut self,
        hir_module: &HirModule,
        item_id: HirItemId,
        item: &crate::frontend::hir::HirItem,
    ) {
        let origin = item.origin.clone();

        // Add the item itself to the index
        self.entries.push(SpanIndexEntry {
            start: origin.span.start,
            end: origin.span.end,
            node_id: HirNodeId::Item(item_id),
        });

        // Index item-specific content based on kind
        match &item.kind {
            HirItemKind::Function(function) => {
                self.index_body(hir_module, function.body);
            }
            HirItemKind::Struct(_struct) => {
                // TODO: Index struct fields
            }
            HirItemKind::Enum(_enum) => {
                // TODO: Index enum variants
            }
            HirItemKind::Protocol(_protocol) => {
                // TODO: Index protocol members
            }
            HirItemKind::Impl(impl_) => {
                // TODO: Index impl functions
            }
            HirItemKind::Extern(_extern_) => {
                // TODO: Index extern functions
            }
            HirItemKind::Use(_use_) => {
                // Use statements don't contain expressions to index
            }
        }
    }

    /// Index a HIR body and all its statements and expressions.
    fn index_body(&mut self, hir_module: &HirModule, body_id: HirBodyId) {
        let body = hir_module.bodies.get(&body_id);
        let body = match body {
            Some(b) => b,
            None => return,
        };

        let origin = body.origin.clone();

        // Add the body itself to the index
        self.entries.push(SpanIndexEntry {
            start: origin.span.start,
            end: origin.span.end,
            node_id: HirNodeId::Body(body_id),
        });

        // Index all statements in the body
        for stmt_id in &body.stmts {
            self.index_stmt(hir_module, *stmt_id);
        }

        // Index the tail expression if present
        if let Some(tail_expr) = body.tail_expr {
            self.index_expr(hir_module, tail_expr);
        }
    }

    /// Index a HIR statement and all its child expressions.
    fn index_stmt(&mut self, hir_module: &HirModule, stmt_id: HirStmtId) {
        let stmt = hir_module.stmts.get(&stmt_id);
        let stmt = match stmt {
            Some(s) => s,
            None => return,
        };

        let origin = stmt.origin.clone();

        // Add the statement itself to the index
        self.entries.push(SpanIndexEntry {
            start: origin.span.start,
            end: origin.span.end,
            node_id: HirNodeId::Stmt(stmt_id),
        });

        // Index statement-specific content
        match &stmt.kind {
            HirStmtKind::Let(let_stmt) => {
                if let Some(value) = let_stmt.value {
                    self.index_expr(hir_module, value);
                }
            }
            HirStmtKind::Expr { expr } => {
                self.index_expr(hir_module, *expr);
            }
            HirStmtKind::Semi { expr } => {
                self.index_expr(hir_module, *expr);
            }
            HirStmtKind::Item { .. } => {
                // Item statements don't contain expressions to index
            }
        }
    }

    /// Index a HIR expression and all its child expressions.
    fn index_expr(&mut self, hir_module: &HirModule, expr_id: HirExprId) {
        let expr = hir_module.exprs.get(&expr_id);
        let expr = match expr {
            Some(e) => e,
            None => return,
        };

        let origin = expr.origin.clone();

        // Add the expression itself to the index
        self.entries.push(SpanIndexEntry {
            start: origin.span.start,
            end: origin.span.end,
            node_id: HirNodeId::Expr(expr_id),
        });

        // Recursively index child expressions based on kind
        match &expr.kind {
            HirExprKind::Literal(_) => {}
            HirExprKind::Path(_) => {}
            HirExprKind::Array { elements } => {
                for element in elements {
                    match element {
                        crate::frontend::hir::HirArrayElement::Expr(e) => {
                            self.index_expr(hir_module, *e);
                        }
                        crate::frontend::hir::HirArrayElement::Spread(e) => {
                            self.index_expr(hir_module, *e);
                        }
                    }
                }
            }
            HirExprKind::Call { callee, args } => {
                self.index_expr(hir_module, *callee);
                for arg in args {
                    self.index_expr(hir_module, arg.value);
                }
            }
            HirExprKind::Block { body } => {
                self.index_body(hir_module, *body);
            }
            HirExprKind::If {
                condition,
                then_body,
                else_expr,
            } => {
                self.index_expr(hir_module, *condition);
                self.index_body(hir_module, *then_body);
                if let Some(else_e) = else_expr {
                    self.index_expr(hir_module, *else_e);
                }
            }
            HirExprKind::While { condition, body } => {
                self.index_expr(hir_module, *condition);
                self.index_body(hir_module, *body);
            }
            HirExprKind::For {
                pat: _,
                iterator,
                body,
            } => {
                self.index_expr(hir_module, *iterator);
                self.index_body(hir_module, *body);
            }
            HirExprKind::Return { value } => {
                if let Some(v) = value {
                    self.index_expr(hir_module, *v);
                }
            }
            HirExprKind::Assign { target, value, .. } => {
                self.index_expr(hir_module, *target);
                self.index_expr(hir_module, *value);
            }
            HirExprKind::Unary { expr, .. } => {
                self.index_expr(hir_module, *expr);
            }
            HirExprKind::Binary { lhs, rhs, .. } => {
                self.index_expr(hir_module, *lhs);
                self.index_expr(hir_module, *rhs);
            }
            HirExprKind::Field { base, name: _ } => {
                self.index_expr(hir_module, *base);
            }
            HirExprKind::OptionalField { base, name: _ } => {
                self.index_expr(hir_module, *base);
            }
            HirExprKind::NamespaceField {
                base,
                name: _,
                turbofish: _,
            } => {
                self.index_expr(hir_module, *base);
            }
            HirExprKind::MethodCall { receiver, args, .. } => {
                self.index_expr(hir_module, *receiver);
                for arg in args {
                    self.index_expr(hir_module, arg.value);
                }
            }
            HirExprKind::Index { base, index } => {
                self.index_expr(hir_module, *base);
                self.index_expr(hir_module, *index);
            }
            HirExprKind::OptionalIndex { base, index } => {
                self.index_expr(hir_module, *base);
                self.index_expr(hir_module, *index);
            }
            HirExprKind::Tuple { elements } => {
                for e in elements {
                    self.index_expr(hir_module, *e);
                }
            }
            HirExprKind::Struct { ty: _, fields } => {
                for field in fields {
                    match field {
                        crate::frontend::hir::HirStructExprField::Named {
                            value,
                            ..
                        } => {
                            self.index_expr(hir_module, *value);
                        }
                        crate::frontend::hir::HirStructExprField::Spread {
                            value,
                        } => {
                            self.index_expr(hir_module, *value);
                        }
                    }
                }
            }
            HirExprKind::Match { subject, arms } => {
                self.index_expr(hir_module, *subject);
                for arm in arms {
                    self.index_expr(hir_module, arm.expr);
                }
            }
            HirExprKind::Closure { body, .. } => {
                self.index_body(hir_module, *body);
            }
            HirExprKind::ForceUnwrap { expr } => {
                self.index_expr(hir_module, *expr);
            }
            HirExprKind::Cast { expr, ty: _, .. } => {
                self.index_expr(hir_module, *expr);
            }
            HirExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.index_expr(hir_module, *s);
                }
                if let Some(e) = end {
                    self.index_expr(hir_module, *e);
                }
            }
            HirExprKind::Spread { expr } => {
                self.index_expr(hir_module, *expr);
            }
            HirExprKind::Try { expr } => {
                self.index_expr(hir_module, *expr);
            }
            HirExprKind::Break | HirExprKind::Continue => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_index_entry_ordering() {
        let entry1 = SpanIndexEntry {
            start: 10,
            end: 20,
            node_id: HirNodeId::Expr(HirExprId::new(0)),
        };
        let entry2 = SpanIndexEntry {
            start: 10,
            end: 30,
            node_id: HirNodeId::Expr(HirExprId::new(1)),
        };
        let entry3 = SpanIndexEntry {
            start: 15,
            end: 25,
            node_id: HirNodeId::Expr(HirExprId::new(2)),
        };

        assert!(entry1 < entry2); // Same start, smaller end
        assert!(entry1 < entry3); // Earlier start
        assert!(entry2 < entry3); // Earlier start
    }

    #[test]
    fn test_empty_index() {
        let index = HirSpanIndex {
            entries: vec![],
            bodies_to_files: BTreeMap::new(),
        };

        assert!(index.find_node_at_offset(100).is_none());
        assert!(index.find_node_before_offset(100).is_none());
    }

    #[test]
    fn test_find_node_at_offset() {
        let mut entries = vec![
            SpanIndexEntry {
                start: 0,
                end: 10,
                node_id: HirNodeId::Expr(HirExprId::new(0)),
            },
            SpanIndexEntry {
                start: 5,
                end: 15, // Nested inside first
                node_id: HirNodeId::Expr(HirExprId::new(1)),
            },
            SpanIndexEntry {
                start: 20,
                end: 30,
                node_id: HirNodeId::Expr(HirExprId::new(2)),
            },
        ];

        entries.sort_by_key(|e| (e.start, e.end));

        let index = HirSpanIndex {
            entries,
            bodies_to_files: BTreeMap::new(),
        };

        // Test finding nodes at various offsets
        assert!(index.find_node_at_offset(100).is_none()); // Beyond all nodes
        assert!(index.find_node_at_offset(25).is_some()); // In third node
        assert!(index.find_node_at_offset(12).is_some()); // In nested node
    }
}
