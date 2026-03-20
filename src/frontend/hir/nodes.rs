use super::ids::{
    HirBodyId, HirExprId, HirItemId, HirPatId, HirStmtId, HirTypeId,
};
use super::origin::HirOrigin;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirMutability {
    Immutable,
    Mutable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirUnaryOp {
    Negate,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirBinaryOp {
    LogicalOr,
    LogicalAnd,
    NullCoalescing,
    BitOr,
    BitXor,
    BitAnd,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    ShiftLeft,
    ShiftRight,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirAssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    BitXorAssign,
    BitOrAssign,
    BitAndAssign,
    ShlAssign,
    ShrAssign,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirLiteral {
    Integer(String),
    Float(String),
    Char(String),
    Boolean(bool),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HirPath {
    pub segments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirItem {
    pub origin: HirOrigin,
    pub kind: HirItemKind,
}

impl HirItem {
    #[must_use]
    pub const fn new(origin: HirOrigin, kind: HirItemKind) -> Self {
        Self { origin, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirItemKind {
    Function(HirFunction),
    Struct(HirStruct),
    Enum(HirEnum),
    Protocol(HirProtocol),
    Impl(HirImpl),
    Extern(HirExtern),
    Use(HirUse),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirInitOrigin {
    Plain,
    Optional,
    Fallible,
}

/// External label form for function-like parameters in HIR.
///
/// This corresponds to the AST `ParamLabel` and preserves the source-level
/// external label information for later use in call-site checking and
/// signature matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirParamLabel {
    /// No external label (`_ x: T`)
    None,
    /// Explicit external label (`foo x: T`)
    Explicit(String),
    /// External label derived from parameter name (`x: T`)
    FromName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunction {
    pub name: String,
    pub init_origin: Option<HirInitOrigin>,
    pub signature: HirFunctionSignature,
    pub body: HirBodyId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunctionSignature {
    pub generic_params: Vec<String>,
    pub params: Vec<HirFunctionParam>,
    pub return_type: Option<HirTypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunctionParam {
    /// External label for this parameter
    pub external_label: HirParamLabel,
    /// Internal parameter name (used for scope binding)
    pub name: String,
    /// Parameter type
    pub ty: HirTypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirStruct {
    pub name: String,
    pub generic_params: Vec<String>,
    pub fields: Vec<HirStructField>,
    pub functions: Vec<HirFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirStructField {
    pub name: String,
    pub ty: HirTypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirEnum {
    pub name: String,
    pub generic_params: Vec<String>,
    pub variants: Vec<HirEnumVariant>,
    pub functions: Vec<HirFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirEnumVariant {
    pub name: String,
    pub payload: Vec<HirTypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirProtocol {
    pub name: String,
    pub generic_params: Vec<String>,
    pub inherited_types: Vec<HirTypeId>,
    pub functions: Vec<HirProtocolFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirProtocolFunction {
    pub name: String,
    pub init_origin: Option<HirInitOrigin>,
    pub signature: HirFunctionSignature,
    pub default_body: Option<HirBodyId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirImpl {
    pub target: HirTypeId,
    pub conformance: Option<HirTypeId>,
    pub functions: Vec<HirFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirExtern {
    pub library_name: String,
    pub functions: Vec<HirExternFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirExternFunction {
    pub local_name: String,
    pub native_symbol: Option<String>,
    pub signature: HirFunctionSignature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirUse {
    pub tree: HirUseTree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirUseTree {
    Path {
        path: HirPath,
    },
    Glob {
        path: HirPath,
    },
    Alias {
        path: HirPath,
        alias: String,
    },
    Group {
        path: Option<HirPath>,
        items: Vec<HirUseTree>,
    },
    SelfImport,
    SelfAlias {
        alias: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirExpr {
    pub origin: HirOrigin,
    pub kind: HirExprKind,
}

impl HirExpr {
    #[must_use]
    pub const fn new(origin: HirOrigin, kind: HirExprKind) -> Self {
        Self { origin, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirExprKind {
    Literal(HirLiteral),
    Path(HirPath),
    Array {
        elements: Vec<HirArrayElement>,
    },
    Call {
        callee: HirExprId,
        args: Vec<HirCallArg>,
    },
    Block {
        body: HirBodyId,
    },
    If {
        condition: HirExprId,
        then_body: HirBodyId,
        else_expr: Option<HirExprId>,
    },
    While {
        condition: HirExprId,
        body: HirBodyId,
    },
    For {
        pat: HirPatId,
        iterator: HirExprId,
        body: HirBodyId,
    },
    Return {
        value: Option<HirExprId>,
    },
    Assign {
        op: HirAssignOp,
        target: HirExprId,
        value: HirExprId,
    },
    Unary {
        op: HirUnaryOp,
        expr: HirExprId,
    },
    Binary {
        op: HirBinaryOp,
        lhs: HirExprId,
        rhs: HirExprId,
    },
    Field {
        base: HirExprId,
        name: String,
    },
    OptionalField {
        base: HirExprId,
        name: String,
    },
    NamespaceField {
        base: HirExprId,
        name: String,
        turbofish: Vec<HirTypeId>,
    },
    /// Method call like `obj.method(args)` - preserves method call
    /// semantics for type checking to transform based on receiver kind
    MethodCall {
        receiver: HirExprId,
        method_name: String,
        args: Vec<HirCallArg>,
    },
    Index {
        base: HirExprId,
        index: HirExprId,
    },
    OptionalIndex {
        base: HirExprId,
        index: HirExprId,
    },
    Tuple {
        elements: Vec<HirExprId>,
    },
    Struct {
        ty: HirTypeId,
        fields: Vec<HirStructExprField>,
    },
    Match {
        subject: HirExprId,
        arms: Vec<HirMatchArm>,
    },
    Closure {
        params: Vec<HirClosureParam>,
        body: HirBodyId,
        uses_shorthand_params: bool,
        is_unsafe: bool,
    },
    ForceUnwrap {
        expr: HirExprId,
    },
    Cast {
        expr: HirExprId,
        ty: HirTypeId,
        is_optional: bool,
    },
    Range {
        start: Option<HirExprId>,
        end: Option<HirExprId>,
        inclusive: bool,
    },
    Spread {
        expr: HirExprId,
    },
    Try {
        expr: HirExprId,
    },
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirArrayElement {
    Expr(HirExprId),
    Spread(HirExprId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirCallArg {
    pub label: Option<String>,
    pub value: HirExprId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirStructExprField {
    Named { name: String, value: HirExprId },
    Spread { value: HirExprId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMatchArm {
    pub pat: HirPatId,
    pub expr: HirExprId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirClosureParam {
    pub name: String,
    pub ty: Option<HirTypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirStmt {
    pub origin: HirOrigin,
    pub kind: HirStmtKind,
}

impl HirStmt {
    #[must_use]
    pub const fn new(origin: HirOrigin, kind: HirStmtKind) -> Self {
        Self { origin, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirStmtKind {
    Let(HirLetStmt),
    Expr { expr: HirExprId },
    Semi { expr: HirExprId },
    Item { item: HirItemId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirLetStmt {
    pub pat: HirPatId,
    pub ty: Option<HirTypeId>,
    pub value: Option<HirExprId>,
    pub mutability: HirMutability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirType {
    pub origin: HirOrigin,
    pub kind: HirTypeKind,
}

impl HirType {
    #[must_use]
    pub const fn new(origin: HirOrigin, kind: HirTypeKind) -> Self {
        Self { origin, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirTypeKind {
    Path(HirPath),
    Reference {
        mutable: bool,
        inner: HirTypeId,
    },
    Pointer {
        mutable: bool,
        inner: HirTypeId,
    },
    Optional {
        inner: HirTypeId,
    },
    Result {
        ok: HirTypeId,
        err: HirTypeId,
    },
    GenericApplication {
        base: HirTypeId,
        args: Vec<HirTypeId>,
    },
    SelfType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirPat {
    pub origin: HirOrigin,
    pub kind: HirPatKind,
}

impl HirPat {
    #[must_use]
    pub const fn new(origin: HirOrigin, kind: HirPatKind) -> Self {
        Self { origin, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirPatKind {
    Binding {
        name: String,
    },
    Wildcard,
    Tuple {
        elements: Vec<HirPatId>,
    },
    Struct {
        path: HirPath,
        fields: Vec<HirStructPatField>,
        has_rest: bool,
    },
    EnumVariant {
        path: HirPath,
        shorthand: bool,
        args: Vec<HirPatId>,
        has_rest: bool,
    },
    Literal(HirLiteral),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirStructPatField {
    pub name: String,
    pub pat: Option<HirPatId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirBody {
    pub origin: HirOrigin,
    pub stmts: Vec<HirStmtId>,
    pub tail_expr: Option<HirExprId>,
}

impl HirBody {
    #[must_use]
    pub const fn new(
        origin: HirOrigin,
        stmts: Vec<HirStmtId>,
        tail_expr: Option<HirExprId>,
    ) -> Self {
        Self {
            origin,
            stmts,
            tail_expr,
        }
    }
}
