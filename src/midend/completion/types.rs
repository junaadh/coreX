//! Core type definitions for semantic completion.

use crate::frontend::resolver::ItemId;
use crate::frontend::semantic::Type;

/// Semantic completion context derived from HIR analysis.
///
/// Unlike traditional completion that uses string heuristics (e.g., scanning
/// for "TypeName." patterns), these contexts are derived from the compiler's
/// semantic understanding of the code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionContext {
    /// Completion at global scope or in an expression position with no receiver.
    Global,

    /// Completion after `::` in a path expression.
    ///
    /// The `scope_item` is the ItemId of the scope/module being accessed,
    /// if it can be resolved.
    PathAccess { scope_item: Option<ItemId> },

    /// Completion after `::` on a type name for associated members.
    ///
    /// Example: `MyEnum::` or `MyStruct::`
    ///
    /// The `base_type` is the resolved type of the base expression.
    AssociatedAccess { base_type: Type },

    /// Completion after `.` on an instance expression.
    ///
    /// Example: `value.`
    ///
    /// The `receiver_type` is the inferred type of the receiver expression.
    MemberAccess { receiver_type: Type },

    /// Completion specifically for enum cases/variants.
    ///
    /// This can be triggered either explicitly (`EnumName::`) or implicitly
    /// when the type system knows an enum type is expected.
    EnumCaseAccess { enum_type: Type },
}

/// A semantic completion candidate derived from compiler analysis.
///
/// Unlike traditional completion that suggests based on text similarity,
/// these candidates are grounded in the compiler's semantic model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCandidate {
    /// The label to display in the completion UI.
    pub label: String,

    /// The kind of completion item.
    pub kind: CompletionKind,

    /// Detail information about the completion (e.g., type signature).
    pub detail: Option<String>,

    /// Documentation for the completion item (if available).
    pub documentation: Option<String>,

    /// Additional metadata about the completion.
    pub metadata: CompletionMetadata,
}

/// The kind of completion item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompletionKind {
    /// A local variable binding.
    Local,

    /// A function or method.
    Function,

    /// A struct type.
    Struct,

    /// An enum type.
    Enum,

    /// An enum variant/case.
    EnumVariant,

    /// A protocol/trait type.
    Protocol,

    /// A struct field.
    Field,

    /// A module/scope.
    Scope,

    /// A type parameter.
    TypeParameter,

    /// An associated type.
    AssociatedType,

    /// A protocol property.
    Property,
}

/// Additional metadata about a completion candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionMetadata {
    /// The ItemId if this corresponds to a declared item.
    pub item_id: Option<ItemId>,

    /// Whether this is deprecated.
    pub deprecated: bool,

    /// The type of the completion (for variables, fields, etc.).
    pub ty: Option<Type>,
}

/// Completion data containing all candidates for a given context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionData {
    /// The completion context.
    pub context: CompletionContext,

    /// All completion candidates for this context.
    pub candidates: Vec<CompletionCandidate>,
}
