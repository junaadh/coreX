//! Expression-level source AST nodes.

use super::decl::MacroBlock;
use super::pattern::Pattern;
use super::span::Spanned;
use super::stmt::{Block, ClauseList};
use super::ty::Type;

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
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

/// Assignment operator spelling preserved in source AST.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssignOp {
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

/// Source string literal preserving interpolation boundaries.
///
/// `coreX` strings are UTF-8.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StringLiteral {
    pub parts: Vec<StringPart>,
}

/// Interpolation-aware string parts.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum StringPart {
    Text(String),
    Interpolation(Box<Spanned<Expr>>),
}

/// Struct-literal type expression (`Name` or `Self` in source grammar).
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeExpr {
    Path(Vec<String>),
    SelfType,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArrayElement {
    Expr(Box<Spanned<Expr>>),
    Spread(Box<Spanned<Expr>>),
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum StructLiteralField {
    Shorthand {
        name: String,
    },
    Named {
        name: String,
        value: Box<Spanned<Expr>>,
    },
    Spread {
        value: Box<Spanned<Expr>>,
    },
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MatchArmBody {
    Expr(Box<Spanned<Expr>>),
    Block(Block),
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchArm {
    pub pattern: Spanned<Pattern>,
    pub body: MatchArmBody,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallArg {
    pub label: Option<String>,
    pub value: Box<Spanned<Expr>>,
}

/// Closure parameter declaration.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClosureParam {
    pub name: String,
    pub ty: Option<Spanned<Type>>,
}

/// Macro expression argument forms.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MacroExprArgs {
    Paren(Vec<CallArg>),
    Braced(MacroBlock),
}

/// Source expressions.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    /// Integer literal with source-preserving spelling.
    IntegerLiteral(String),
    /// Float literal with source-preserving spelling.
    FloatLiteral(String),
    /// Char literal with source-preserving spelling.
    CharLiteral(String),
    BooleanLiteral(bool),
    StringLiteral(StringLiteral),
    Identifier(String),
    SelfValue,
    SelfType,

    /// `.member` shorthand member / enum-case syntax.
    ///
    /// Semantic interpretation depends on surrounding type/value context.
    ShorthandMember {
        name: String,
    },
    /// `Type.member` source form used for qualified enum-case/member syntax.
    ///
    /// Qualifier is represented as expression AST to avoid over-constraining
    /// path shapes at source AST stage.
    QualifiedMember {
        qualifier: Box<Spanned<Expr>>,
        member: String,
    },

    Grouped(Box<Spanned<Expr>>),
    ArrayLiteral(Vec<ArrayElement>),
    StructLiteral {
        ty: TypeExpr,
        fields: Vec<StructLiteralField>,
    },
    Block(Block),
    UnsafeBlock(Block),
    If {
        clauses: ClauseList,
        then_branch: Block,
        /// `else` branch expression; plain `else { ... }` is represented as
        /// `Expr::Block`.
        else_branch: Option<Box<Spanned<Expr>>>,
    },
    Match {
        subject: Box<Spanned<Expr>>,
        arms: Vec<Spanned<MatchArm>>,
    },
    Closure {
        params: Vec<ClosureParam>,
        body: Block,
        uses_shorthand_params: bool,
        is_unsafe: bool,
    },
    Macro {
        name: String,
        args: MacroExprArgs,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Spanned<Expr>>,
    },
    Ternary {
        condition: Box<Spanned<Expr>>,
        then_expr: Box<Spanned<Expr>>,
        else_expr: Box<Spanned<Expr>>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    Assignment {
        /// Assignment spelling (`=`, `+=`, `<<=`, ...) preserved in source AST.
        op: AssignOp,
        target: Box<Spanned<Expr>>,
        value: Box<Spanned<Expr>>,
    },
    MemberAccess {
        base: Box<Spanned<Expr>>,
        member: String,
    },
    /// Method call like `obj.method(args)` - will be desugared to
    /// `Type::method(&obj, args)` based on receiver type
    MethodCall {
        receiver: Box<Spanned<Expr>>,
        method_name: String,
        args: Vec<CallArg>,
        trailing_closure: Option<Box<Spanned<Expr>>>,
    },
    /// Constructor call like `Point(x, y)` or `Point(1, 2)` - will be desugared
    /// to `TypeName::init(x, y)` or `TypeName::init(1, 2)`
    ConstructorCall {
        type_name: String,
        args: Vec<CallArg>,
    },
    OptionalMemberAccess {
        base: Box<Spanned<Expr>>,
        member: String,
    },
    NamespaceAccess {
        base: Box<Spanned<Expr>>,
        member: String,
        turbofish: Vec<Spanned<Type>>,
    },
    Call {
        callee: Box<Spanned<Expr>>,
        args: Vec<CallArg>,
        trailing_closure: Option<Box<Spanned<Expr>>>,
    },
    Index {
        base: Box<Spanned<Expr>>,
        index: Box<Spanned<Expr>>,
    },
    OptionalIndex {
        base: Box<Spanned<Expr>>,
        index: Box<Spanned<Expr>>,
    },
    ForceUnwrap {
        expr: Box<Spanned<Expr>>,
    },
    Cast {
        expr: Box<Spanned<Expr>>,
        ty: Spanned<Type>,
        is_optional: bool,
    },
    /// Structural range expression (`..`, `..=`, open-ended forms).
    Range {
        start: Option<Box<Spanned<Expr>>>,
        end: Option<Box<Spanned<Expr>>>,
        inclusive: bool,
    },
    /// Structural spread expression `..expr`.
    ///
    /// Parser enforces valid spread-supporting contexts (for example array and
    /// struct literals).
    Spread {
        expr: Box<Spanned<Expr>>,
    },
    Try {
        expr: Box<Spanned<Expr>>,
    },
    /// Tuple literal expression `(a, b, c)`.
    Tuple(Vec<Spanned<Expr>>),
}
