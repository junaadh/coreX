//! Expression-level source AST nodes.

use super::pattern::Pattern;
use super::span::Spanned;
use super::stmt::{Block, ClauseList};
use super::ty::Type;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    LogicalOr,
    LogicalAnd,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

/// Source string literal preserving interpolation boundaries.
///
/// `coreX` strings are UTF-8.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StringLiteral {
    pub parts: Vec<StringPart>,
}

/// Interpolation-aware string parts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StringPart {
    Text(String),
    Interpolation(Box<Spanned<Expr>>),
}

/// Struct-literal type expression (`Name` or `Self` in source grammar).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeExpr {
    Path(Vec<String>),
    SelfType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArrayElement {
    Expr(Box<Spanned<Expr>>),
    Spread(Box<Spanned<Expr>>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MatchArmBody {
    Expr(Box<Spanned<Expr>>),
    Block(Block),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchArm {
    pub pattern: Spanned<Pattern>,
    pub body: MatchArmBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallArg {
    pub label: Option<String>,
    pub value: Box<Spanned<Expr>>,
}

/// Closure parameter declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClosureParam {
    pub name: String,
    pub ty: Option<Spanned<Type>>,
}

/// Macro expression argument forms.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MacroExprArgs {
    Paren(Vec<CallArg>),
    Braced(Block),
}

/// Source expressions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    If {
        clauses: ClauseList,
        then_branch: Block,
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
    },
    Macro {
        name: String,
        args: MacroExprArgs,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Spanned<Expr>>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    Assignment {
        target: Box<Spanned<Expr>>,
        value: Box<Spanned<Expr>>,
    },
    MemberAccess {
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
}
