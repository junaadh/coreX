//! Statement-level source AST nodes.

use super::expr::Expr;
use super::pattern::Pattern;
use super::span::Spanned;
use super::ty::Type;

/// Block with explicit statement list and optional tail expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Block {
    pub statements: Vec<Spanned<Stmt>>,
    pub tail_expr: Option<Box<Spanned<Expr>>>,
}

/// Shared clause-list shape used by `if`, `guard`, and `while`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClauseList {
    pub clauses: Vec<Spanned<Clause>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindingClause {
    pub pattern: Spanned<Pattern>,
    pub ty: Option<Spanned<Type>>,
    pub value: Box<Spanned<Expr>>,
}

/// Clause in `if` / `guard` / `while` condition lists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Clause {
    Expr(Box<Spanned<Expr>>),
    LetBinding(BindingClause),
    VarBinding(BindingClause),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LetStmt {
    pub pattern: Spanned<Pattern>,
    pub ty: Option<Spanned<Type>>,
    pub value: Box<Spanned<Expr>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarStmt {
    pub pattern: Spanned<Pattern>,
    pub ty: Option<Spanned<Type>>,
    pub value: Box<Spanned<Expr>>,
}

/// `guard` is statement-only and requires an `else` block.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GuardStmt {
    pub clauses: ClauseList,
    pub else_block: Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhileStmt {
    pub clauses: ClauseList,
    pub body: Block,
}

/// Placeholder shape for `for` statement syntax.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForStmt {
    pub pattern: Spanned<Pattern>,
    pub iterator: Box<Spanned<Expr>>,
    pub body: Block,
}

/// Source statements.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stmt {
    Let(Spanned<LetStmt>),
    Var(Spanned<VarStmt>),
    Expr {
        expr: Box<Spanned<Expr>>,
        has_semi: bool,
    },
    Guard(Spanned<GuardStmt>),
    While(Spanned<WhileStmt>),
    For(Spanned<ForStmt>),
    Return(Option<Box<Spanned<Expr>>>),
    Break,
    Continue,
}
