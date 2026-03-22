//! HIR node lookup at cursor position.

use crate::frontend::hir::{HirExprId, HirOrigin};
use crate::frontend::semantic::Type;

/// Information about an HIR node at a given position.
#[derive(Debug, Clone)]
pub struct HirNodeContext {
    /// The HIR node kind.
    pub kind: HirNodeKind,

    /// The origin/span of the node.
    pub origin: HirOrigin,

    /// For expressions: the expression ID and type (if known).
    pub expr_info: Option<(HirExprId, Option<Type>)>,
}

/// The kind of HIR node found during completion context lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirNodeKind {
    /// A path expression (e.g., `foo`, `foo::bar`, `Type::method`).
    Path,

    /// A field access expression (e.g., `expr.field`).
    FieldAccess,

    /// A namespace field access (e.g., `Type::associated`).
    NamespaceAccess,

    /// A method call (e.g., `expr.method()`).
    MethodCall,

    /// Any other expression.
    OtherExpr,

    /// Not in an expression (e.g., at top level or in a statement position).
    NonExpr,
}

/// Find the HIR node at or before the given offset.
///
/// This function searches through the HIR to find the node that contains
/// or is immediately before the cursor position. This is used to determine
/// the semantic context for code completion.
///
/// # Arguments
/// * `hir_file` - The HIR file to search
/// * `offset` - The cursor offset in bytes
///
/// # Returns
/// * `Some(HirNodeContext)` if a relevant node is found
/// * `None` if no relevant node could be determined
///
/// # Note
/// This is currently a placeholder implementation. A production version
/// would build an index for efficient span-based lookup.
pub fn find_hir_node_at_offset(
    _hir_file: &crate::frontend::hir::HirFile,
    _offset: usize,
) -> Option<HirNodeContext> {
    // TODO: Implement efficient span-based HIR node lookup
    //
    // The implementation should:
    // 1. Build an index from spans to HIR node IDs
    // 2. Use binary search to find nodes containing the offset
    // 3. Return the innermost node (most specific context)
    //
    // For now, return None to indicate no specific context
    None
}
