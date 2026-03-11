//! Token definitions and keyword classification for the `coreX` lexer.

/// Byte span in the original source buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// One lexed token with a source span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    #[must_use]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Comment forms recognized by the lexer.
///
/// Comments are classified lexically and intended to be kept as trivia rather
/// than ordinary parser tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommentKind {
    Line,
    DocLine,
    InnerDocLine,
    Block,
    DocBlock,
    InnerDocBlock,
}

/// Reserved keyword set for `coreX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Keyword {
    Use,
    Struct,
    Enum,
    Impl,
    Protocol,
    Fn,
    Init,
    Extern,
    Let,
    Var,
    If,
    Else,
    Guard,
    While,
    For,
    Match,
    Return,
    Break,
    Continue,
    SelfValue,
    SelfType,
    Pub,
    Async,
    In,
    Where,
    True,
    False,
    Mut,
    Type,
    Get,
    Set,
    Try,
}

impl Keyword {
    #[must_use]
    pub const fn as_token_kind(self) -> TokenKind {
        match self {
            Self::Use => TokenKind::KwUse,
            Self::Struct => TokenKind::KwStruct,
            Self::Enum => TokenKind::KwEnum,
            Self::Impl => TokenKind::KwImpl,
            Self::Protocol => TokenKind::KwProtocol,
            Self::Fn => TokenKind::KwFn,
            Self::Init => TokenKind::KwInit,
            Self::Extern => TokenKind::KwExtern,
            Self::Let => TokenKind::KwLet,
            Self::Var => TokenKind::KwVar,
            Self::If => TokenKind::KwIf,
            Self::Else => TokenKind::KwElse,
            Self::Guard => TokenKind::KwGuard,
            Self::While => TokenKind::KwWhile,
            Self::For => TokenKind::KwFor,
            Self::Match => TokenKind::KwMatch,
            Self::Return => TokenKind::KwReturn,
            Self::Break => TokenKind::KwBreak,
            Self::Continue => TokenKind::KwContinue,
            Self::SelfValue => TokenKind::KwSelfValue,
            Self::SelfType => TokenKind::KwSelfType,
            Self::Pub => TokenKind::KwPub,
            Self::Async => TokenKind::KwAsync,
            Self::In => TokenKind::KwIn,
            Self::Where => TokenKind::KwWhere,
            Self::True => TokenKind::KwTrue,
            Self::False => TokenKind::KwFalse,
            Self::Mut => TokenKind::KwMut,
            Self::Type => TokenKind::KwType,
            Self::Get => TokenKind::KwGet,
            Self::Set => TokenKind::KwSet,
            Self::Try => TokenKind::KwTry,
        }
    }
}

/// Classifies an identifier spelling as a reserved keyword when applicable.
///
/// Builtin primitive type names are intentionally *not* classified here and
/// remain ordinary identifiers lexically.
#[must_use]
pub fn classify_keyword(ident: &str) -> Option<Keyword> {
    match ident {
        "use" => Some(Keyword::Use),
        "struct" => Some(Keyword::Struct),
        "enum" => Some(Keyword::Enum),
        "impl" => Some(Keyword::Impl),
        "protocol" => Some(Keyword::Protocol),
        "fn" => Some(Keyword::Fn),
        "init" => Some(Keyword::Init),
        "extern" => Some(Keyword::Extern),
        "let" => Some(Keyword::Let),
        "var" => Some(Keyword::Var),
        "if" => Some(Keyword::If),
        "else" => Some(Keyword::Else),
        "guard" => Some(Keyword::Guard),
        "while" => Some(Keyword::While),
        "for" => Some(Keyword::For),
        "match" => Some(Keyword::Match),
        "return" => Some(Keyword::Return),
        "break" => Some(Keyword::Break),
        "continue" => Some(Keyword::Continue),
        "self" => Some(Keyword::SelfValue),
        "Self" => Some(Keyword::SelfType),
        "pub" => Some(Keyword::Pub),
        "async" => Some(Keyword::Async),
        "in" => Some(Keyword::In),
        "where" => Some(Keyword::Where),
        "true" => Some(Keyword::True),
        "false" => Some(Keyword::False),
        "mut" => Some(Keyword::Mut),
        "type" => Some(Keyword::Type),
        "get" => Some(Keyword::Get),
        "set" => Some(Keyword::Set),
        "try" => Some(Keyword::Try),
        _ => None,
    }
}

/// Classifies an identifier spelling directly into a keyword token kind.
#[must_use]
pub fn classify_keyword_token(ident: &str) -> Option<TokenKind> {
    classify_keyword(ident).map(Keyword::as_token_kind)
}

/// Lexical token kinds for the `coreX` frontend.
///
/// Notes:
/// - Primitive type names remain `Ident`, not dedicated keyword tokens.
/// - Some variants are segmented string/interpolation markers.
/// - `DotDot` / `DotDotEq` are lexical; range vs spread meaning is parser
///   contextual.
/// - `At` always lexes the same; attribute vs macro interpretation is parser
///   contextual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // Identifier-like
    Ident,
    /// `$0`, `$1`, ... shorthand closure parameter token.
    ClosureShorthandParam,

    // Literals / interpolation segments
    /// Integer literal with source-preserving spelling (base/separators/suffix).
    Integer,
    /// Float literal with source-preserving spelling (fraction/exponent/separators).
    Float,
    /// Char literal with source-preserving spelling.
    Char,
    StringStart,
    StringText,
    StringEnd,
    InterpolationStart,
    InterpolationEnd,

    // Keywords
    KwUse,
    KwStruct,
    KwEnum,
    KwImpl,
    KwProtocol,
    KwFn,
    KwInit,
    KwExtern,
    KwLet,
    KwVar,
    KwIf,
    KwElse,
    KwGuard,
    KwWhile,
    KwFor,
    KwMatch,
    KwReturn,
    KwBreak,
    KwContinue,
    KwSelfValue,
    KwSelfType,
    KwPub,
    KwAsync,
    KwIn,
    KwWhere,
    KwTrue,
    KwFalse,
    KwMut,
    KwType,
    KwGet,
    KwSet,
    KwTry,

    // Punctuation / operators
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    Dot,
    ColonColon,
    Arrow,
    FatArrow,
    Eq,
    EqEq,
    Bang,
    BangEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    AmpAmp,
    PipePipe,
    Question,
    At,
    /// `..`
    DotDot,
    /// `..=`
    DotDotEq,

    Eof,
}
