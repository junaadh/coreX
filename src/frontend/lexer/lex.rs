//! Full lexer driver for the `coreX` frontend.
//!
//! This module assembles the existing lexical sublayers into a mode-aware
//! token stream driver:
//! - trivia/comment skipping
//! - char/string segmented lexing
//! - punctuation/operator lexing
//! - identifier-like lexing
//! - numeric lexing
//!
//! `Lexer` emits one token per `next_token()` call and manages transitions
//! across normal, string, and interpolation modes.

use super::{
    classify_keyword_token, lex_char_literal, lex_ident_like,
    lex_interpolation_end, lex_lifetime, lex_number, lex_punct_or_operator,
    lex_string_segment, lex_string_start, skip_trivia, CommentError,
    SourceCursor, Span, StringLexError, StringLexMode, Token, TokenKind,
};

/// Errors produced by integrated tokenization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexerError {
    Comment(CommentError),
    String(StringLexError),
    UnexpectedCharacter { span: Span, ch: char },
}

impl std::fmt::Display for LexerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Comment(source) => write!(f, "{source}"),
            Self::String(source) => write!(f, "{source}"),
            Self::UnexpectedCharacter { span, ch } => write!(
                f,
                "unexpected character '{}' at byte range {}..{}",
                ch, span.start, span.end
            ),
        }
    }
}

impl std::error::Error for LexerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Comment(source) => Some(source),
            Self::String(source) => Some(source),
            Self::UnexpectedCharacter { .. } => None,
        }
    }
}

impl From<CommentError> for LexerError {
    fn from(value: CommentError) -> Self {
        Self::Comment(value)
    }
}

impl From<StringLexError> for LexerError {
    fn from(value: StringLexError) -> Self {
        Self::String(value)
    }
}

/// Mode-aware token driver over immutable source text.
///
/// `Lexer` owns the source cursor and tracks current lexing mode:
/// - normal tokenization
/// - segmented string mode
/// - interpolation mode with tracked parenthesis depth
#[derive(Debug, Clone)]
pub struct Lexer<'a> {
    cursor: SourceCursor<'a>,
    mode: StringLexMode,
    mode_stack: Vec<StringLexMode>,
}

impl<'a> Lexer<'a> {
    /// Creates a lexer at offset `0` in normal mode.
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        Self {
            cursor: SourceCursor::new(source),
            mode: StringLexMode::Normal,
            mode_stack: Vec::new(),
        }
    }

    /// Emits the next token according to current lexer mode.
    ///
    /// Mode behavior:
    /// - `Normal`: skips trivia, emits ordinary tokens, and emits repeatable
    ///   `Eof` at end-of-input
    /// - `InString`: emits `StringText` / `InterpolationStart` / `StringEnd`
    /// - `InInterpolation`: skips trivia, tracks nested parentheses, emits
    ///   `InterpolationEnd` when depth reaches close boundary
    ///
    /// # Errors
    ///
    /// Returns `LexerError` when comment or string lexing fails, or when an
    /// unexpected character is encountered in the current mode.
    pub fn next_token(&mut self) -> Result<Token, LexerError> {
        match self.mode {
            StringLexMode::Normal => self.next_token_normal(),
            StringLexMode::InString => self.next_token_in_string(),
            StringLexMode::InInterpolation { paren_depth } => {
                self.next_token_in_interpolation(paren_depth)
            }
        }
    }

    /// Lexes all tokens through the first emitted `Eof`.
    ///
    /// # Errors
    ///
    /// Returns `LexerError` from [`Self::next_token`] if lexing fails before
    /// reaching EOF.
    pub fn lex_all(mut self) -> Result<Vec<Token>, LexerError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let kind = token.kind;
            tokens.push(token);
            if kind == TokenKind::Eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn next_token_normal(&mut self) -> Result<Token, LexerError> {
        skip_trivia(&mut self.cursor)?;
        if self.cursor.is_eof() {
            let offset = self.cursor.offset();
            return Ok(Token::new(TokenKind::Eof, Span::new(offset, offset)));
        }

        if let Some(token) = self.lex_non_string_token()? {
            return Ok(token);
        }

        Err(self.unexpected_character())
    }

    fn next_token_in_string(&mut self) -> Result<Token, LexerError> {
        let token = lex_string_segment(&mut self.cursor)?
            .ok_or_else(|| self.unexpected_character())?;

        match token.kind {
            TokenKind::StringEnd => self.exit_string_mode(),
            TokenKind::InterpolationStart => {
                self.mode = StringLexMode::InInterpolation { paren_depth: 0 };
            }
            _ => {}
        }

        Ok(token)
    }

    fn next_token_in_interpolation(
        &mut self,
        paren_depth: usize,
    ) -> Result<Token, LexerError> {
        skip_trivia(&mut self.cursor)?;

        if self.cursor.is_eof() {
            let offset = self.cursor.offset();
            return Err(LexerError::String(
                StringLexError::UnterminatedString {
                    span: Span::new(offset, offset),
                },
            ));
        }

        if let Some(token) =
            lex_interpolation_end(&mut self.cursor, paren_depth)
        {
            self.mode = StringLexMode::InString;
            return Ok(token);
        }

        let token = self
            .lex_non_string_token()?
            .ok_or_else(|| self.unexpected_character())?;

        if let StringLexMode::InInterpolation { paren_depth } = self.mode {
            self.mode = match token.kind {
                TokenKind::LParen => StringLexMode::InInterpolation {
                    paren_depth: paren_depth + 1,
                },
                TokenKind::RParen if paren_depth > 0 => {
                    StringLexMode::InInterpolation {
                        paren_depth: paren_depth - 1,
                    }
                }
                _ => StringLexMode::InInterpolation { paren_depth },
            };
        }

        Ok(token)
    }

    fn lex_non_string_token(&mut self) -> Result<Option<Token>, LexerError> {
        if let Some(token) = lex_string_start(&mut self.cursor) {
            self.enter_string_mode();
            return Ok(Some(token));
        }

        if let Some(token) = lex_char_literal(&mut self.cursor)? {
            return Ok(Some(token));
        }

        if let Some(token) = lex_number(&mut self.cursor) {
            return Ok(Some(token));
        }

        if let Some(token) = lex_lifetime(&mut self.cursor) {
            return Ok(Some(token));
        }

        if let Some(token) = lex_ident_like(&mut self.cursor) {
            return Ok(Some(token));
        }

        if let Some(token) = lex_punct_or_operator(&mut self.cursor) {
            return Ok(Some(token));
        }

        Ok(None)
    }

    fn enter_string_mode(&mut self) {
        self.mode_stack.push(self.mode);
        self.mode = StringLexMode::InString;
    }

    fn exit_string_mode(&mut self) {
        self.mode = self.mode_stack.pop().unwrap_or(StringLexMode::Normal);
    }

    fn unexpected_character(&self) -> LexerError {
        let start = self.cursor.offset();
        let ch = self.cursor.peek().unwrap_or('\0');
        let end = start + ch.len_utf8();
        LexerError::UnexpectedCharacter {
            span: Span::new(start, end),
            ch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(tokens: &[Token]) -> Vec<TokenKind> {
        tokens.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_simple_token_sequence() {
        let lexer = Lexer::new("let x = 1;");
        let tokens = lexer.lex_all().expect("lex all");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::KwLet,
                TokenKind::Ident,
                TokenKind::Eq,
                TokenKind::Integer,
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn skips_trivia_between_tokens() {
        let lexer = Lexer::new("let /*c*/ x // line\n = 1 ;");
        let tokens = lexer.lex_all().expect("lex all");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::KwLet,
                TokenKind::Ident,
                TokenKind::Eq,
                TokenKind::Integer,
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_string_as_segmented_tokens() {
        let lexer = Lexer::new("\"abc\"");
        let tokens = lexer.lex_all().expect("lex all");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::StringStart,
                TokenKind::StringText,
                TokenKind::StringEnd,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_string_with_interpolation() {
        let lexer = Lexer::new("\"a\\(x)b\"");
        let tokens = lexer.lex_all().expect("lex all");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::StringStart,
                TokenKind::StringText,
                TokenKind::InterpolationStart,
                TokenKind::Ident,
                TokenKind::InterpolationEnd,
                TokenKind::StringText,
                TokenKind::StringEnd,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_nested_parentheses_inside_interpolation() {
        let lexer = Lexer::new("\"\\(foo(bar))\"");
        let tokens = lexer.lex_all().expect("lex all");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::StringStart,
                TokenKind::InterpolationStart,
                TokenKind::Ident,
                TokenKind::LParen,
                TokenKind::Ident,
                TokenKind::RParen,
                TokenKind::InterpolationEnd,
                TokenKind::StringEnd,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_char_then_punct_then_ident() {
        let lexer = Lexer::new("'a'+b");
        let tokens = lexer.lex_all().expect("lex all");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Char,
                TokenKind::Plus,
                TokenKind::Ident,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_numbers_before_punctuation_boundaries() {
        let float_tokens = Lexer::new("1.25").lex_all().expect("lex float");
        assert_eq!(
            kinds(&float_tokens),
            vec![TokenKind::Float, TokenKind::Eof]
        );

        let range_tokens =
            Lexer::new("1..3").lex_all().expect("lex range-like");
        assert_eq!(
            kinds(&range_tokens),
            vec![
                TokenKind::Integer,
                TokenKind::DotDot,
                TokenKind::Integer,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn returns_unexpected_character_for_unknown_input() {
        let mut lexer = Lexer::new("~");
        let err = lexer.next_token().expect_err("expected error");
        assert!(matches!(
            err,
            LexerError::UnexpectedCharacter { ch: '~', .. }
        ));
    }

    #[test]
    fn unterminated_string_propagates_error() {
        let mut lexer = Lexer::new("\"abc");
        let first = lexer.next_token().expect("string start");
        assert_eq!(first.kind, TokenKind::StringStart);
        let err = lexer.next_token().expect_err("expected error");
        assert!(matches!(
            err,
            LexerError::String(StringLexError::UnterminatedString { .. })
        ));
    }

    #[test]
    fn unterminated_block_comment_propagates_error() {
        let mut lexer = Lexer::new("/* x");
        let err = lexer.next_token().expect_err("expected error");
        assert!(matches!(err, LexerError::Comment(_)));
    }

    #[test]
    fn eof_token_is_repeatable() {
        let mut lexer = Lexer::new("");
        let first = lexer.next_token().expect("eof");
        let second = lexer.next_token().expect("eof");
        assert_eq!(first.kind, TokenKind::Eof);
        assert_eq!(second.kind, TokenKind::Eof);
    }
}
