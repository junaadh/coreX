//! Char and segmented string lexing for the `coreX` frontend.
//!
//! This module lexes:
//! - char literals (`'a'`, `'é'`, `'\\n'`, ...)
//! - string segment tokens for interpolation-aware string mode
//!
//! Strings are UTF-8 source strings. Interpolation starts at `\\(` and later
//! closes via `)` when interpolation depth allows it.
//!
//! The segmented string model is intentional and feeds a later stateful full
//! lexer:
//! - `StringStart`
//! - `StringText`
//! - `InterpolationStart`
//! - `InterpolationEnd`
//! - `StringEnd`

use super::{SourceCursor, Token, TokenKind};

/// Minimal string/interpolation mode shell for full lexer assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringLexMode {
    Normal,
    InString,
    InInterpolation { paren_depth: usize },
}

/// Errors produced by char/string lexical segmentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringLexError {
    UnterminatedChar { span: super::Span },
    UnterminatedString { span: super::Span },
    UnterminatedEscape { span: super::Span },
    EmptyCharLiteral { span: super::Span },
    MultiCharLiteral { span: super::Span },
}

impl std::fmt::Display for StringLexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnterminatedChar { span } => {
                write!(
                    f,
                    "unterminated char literal at byte range {}..{}",
                    span.start, span.end
                )
            }
            Self::UnterminatedString { span } => {
                write!(
                    f,
                    "unterminated string literal at byte range {}..{}",
                    span.start, span.end
                )
            }
            Self::UnterminatedEscape { span } => {
                write!(
                    f,
                    "unterminated escape at byte range {}..{}",
                    span.start, span.end
                )
            }
            Self::EmptyCharLiteral { span } => {
                write!(
                    f,
                    "empty char literal at byte range {}..{}",
                    span.start, span.end
                )
            }
            Self::MultiCharLiteral { span } => {
                write!(
                    f,
                    "multi-char literal at byte range {}..{}",
                    span.start, span.end
                )
            }
        }
    }
}

impl std::error::Error for StringLexError {}

/// Lexes one char literal at the current cursor position.
///
/// Returns `Ok(Some(Char))` when current input starts with `'` and forms a
/// lexically valid single char/escape unit literal.
///
/// Returns `Ok(None)` without consuming input when current input does not
/// start with `'`.
///
/// Returns `Err(StringLexError)` for malformed char literal spellings.
pub fn lex_char_literal(
    cursor: &mut SourceCursor<'_>,
) -> Result<Option<Token>, StringLexError> {
    if cursor.peek() != Some('\'') {
        return Ok(None);
    }

    let start = cursor.mark();
    let _ = cursor.bump();

    if cursor.is_eof() {
        return Err(StringLexError::UnterminatedChar {
            span: cursor.current_span_from(start),
        });
    }
    if cursor.peek() == Some('\'') {
        let _ = cursor.bump();
        return Err(StringLexError::EmptyCharLiteral {
            span: cursor.current_span_from(start),
        });
    }

    if cursor.peek() == Some('\\') {
        let _ = cursor.bump();
        if cursor.is_eof() {
            return Err(StringLexError::UnterminatedEscape {
                span: cursor.current_span_from(start),
            });
        }
        let _ = cursor.bump();
    } else {
        let _ = cursor.bump();
    }

    match cursor.peek() {
        Some('\'') => {
            let _ = cursor.bump();
            Ok(Some(Token::new(
                TokenKind::Char,
                cursor.current_span_from(start),
            )))
        }
        Some(_) => Err(StringLexError::MultiCharLiteral {
            span: cursor.current_span_from(start),
        }),
        None => Err(StringLexError::UnterminatedChar {
            span: cursor.current_span_from(start),
        }),
    }
}

/// Lexes `StringStart` by consuming the opening `"`.
#[must_use]
pub fn lex_string_start(cursor: &mut SourceCursor<'_>) -> Option<Token> {
    let start = cursor.mark();
    if !cursor.eat_if('"') {
        return None;
    }
    Some(Token::new(
        TokenKind::StringStart,
        cursor.current_span_from(start),
    ))
}

/// Lexes one token while already in string mode.
///
/// Emits exactly one of:
/// - `StringText` for ordinary text chunks
/// - `InterpolationStart` for `\\(`
/// - `StringEnd` for closing `"`
///
/// Returns `Ok(None)` only when no string-segment token begins at the current
/// offset.
///
/// Returns `Err(StringLexError::UnterminatedString)` when EOF is reached before
/// a closing quote boundary can be emitted.
pub fn lex_string_segment(
    cursor: &mut SourceCursor<'_>,
) -> Result<Option<Token>, StringLexError> {
    let start = cursor.mark();

    if cursor.is_eof() {
        return Err(StringLexError::UnterminatedString {
            span: cursor.current_span_from(start),
        });
    }

    if cursor.starts_with("\\(") {
        let _ = cursor.eat_str("\\(");
        return Ok(Some(Token::new(
            TokenKind::InterpolationStart,
            cursor.current_span_from(start),
        )));
    }

    if cursor.eat_if('"') {
        return Ok(Some(Token::new(
            TokenKind::StringEnd,
            cursor.current_span_from(start),
        )));
    }

    while !cursor.is_eof() {
        if cursor.starts_with("\\(") || cursor.peek() == Some('"') {
            break;
        }
        if cursor.peek() == Some('\\') {
            let _ = cursor.bump();
            if cursor.is_eof() {
                return Err(StringLexError::UnterminatedEscape {
                    span: cursor.current_span_from(start),
                });
            }
            let _ = cursor.bump();
            continue;
        }
        let _ = cursor.bump();
    }

    if cursor.is_eof() {
        return Err(StringLexError::UnterminatedString {
            span: cursor.current_span_from(start),
        });
    }

    if cursor.offset() == start {
        return Ok(None);
    }

    Ok(Some(Token::new(
        TokenKind::StringText,
        cursor.current_span_from(start),
    )))
}

/// Lexes `InterpolationEnd` for `)` when interpolation depth allows closing.
///
/// Contract:
/// - emits `InterpolationEnd` only when `paren_depth == 0` and current input
///   starts with `)`
/// - otherwise returns `None` without consumption
#[must_use]
pub fn lex_interpolation_end(
    cursor: &mut SourceCursor<'_>,
    paren_depth: usize,
) -> Option<Token> {
    if paren_depth != 0 {
        return None;
    }

    let start = cursor.mark();
    if !cursor.eat_if(')') {
        return None;
    }
    Some(Token::new(
        TokenKind::InterpolationEnd,
        cursor.current_span_from(start),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_simple_char_literal() {
        let mut cursor = SourceCursor::new("'a'");
        let token = lex_char_literal(&mut cursor)
            .expect("char parse")
            .expect("char token");
        assert_eq!(token.kind, TokenKind::Char);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_utf8_char_literal() {
        let mut cursor = SourceCursor::new("'é'");
        let token = lex_char_literal(&mut cursor)
            .expect("char parse")
            .expect("char token");
        assert_eq!(token.kind, TokenKind::Char);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_escaped_char_literal() {
        let mut cursor = SourceCursor::new("'\\n'");
        let token = lex_char_literal(&mut cursor)
            .expect("char parse")
            .expect("char token");
        assert_eq!(token.kind, TokenKind::Char);
        assert!(cursor.is_eof());
    }

    #[test]
    fn rejects_empty_char_literal() {
        let mut cursor = SourceCursor::new("''");
        let err = lex_char_literal(&mut cursor).expect_err("expected error");
        assert!(matches!(err, StringLexError::EmptyCharLiteral { .. }));
    }

    #[test]
    fn rejects_multi_char_literal() {
        let mut cursor = SourceCursor::new("'ab'");
        let err = lex_char_literal(&mut cursor).expect_err("expected error");
        assert!(matches!(err, StringLexError::MultiCharLiteral { .. }));
    }

    #[test]
    fn rejects_unterminated_char_literal() {
        let mut cursor = SourceCursor::new("'a");
        let err = lex_char_literal(&mut cursor).expect_err("expected error");
        assert!(matches!(err, StringLexError::UnterminatedChar { .. }));
    }

    #[test]
    fn lex_string_start_token() {
        let mut cursor = SourceCursor::new("\"abc");
        let token = lex_string_start(&mut cursor).expect("string start");
        assert_eq!(token.kind, TokenKind::StringStart);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 1);
        assert_eq!(cursor.remaining(), "abc");
    }

    #[test]
    fn lex_plain_string_segments() {
        let mut cursor = SourceCursor::new("\"abc\"");
        let start = lex_string_start(&mut cursor).expect("start");
        assert_eq!(start.kind, TokenKind::StringStart);

        let text = lex_string_segment(&mut cursor)
            .expect("segment parse")
            .expect("text token");
        assert_eq!(text.kind, TokenKind::StringText);

        let end = lex_string_segment(&mut cursor)
            .expect("segment parse")
            .expect("end token");
        assert_eq!(end.kind, TokenKind::StringEnd);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_string_with_escape_does_not_terminate_early() {
        let mut cursor = SourceCursor::new("a\\\"b\"");
        let text = lex_string_segment(&mut cursor)
            .expect("segment parse")
            .expect("text token");
        assert_eq!(text.kind, TokenKind::StringText);
        assert_eq!(cursor.remaining(), "\"");

        let end = lex_string_segment(&mut cursor)
            .expect("segment parse")
            .expect("end token");
        assert_eq!(end.kind, TokenKind::StringEnd);
    }

    #[test]
    fn lex_string_interpolation_start() {
        let mut cursor = SourceCursor::new("\\(x");
        let token = lex_string_segment(&mut cursor)
            .expect("segment parse")
            .expect("interpolation token");
        assert_eq!(token.kind, TokenKind::InterpolationStart);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 2);
        assert_eq!(cursor.remaining(), "x");
    }

    #[test]
    fn lex_string_text_stops_before_interpolation() {
        let mut cursor = SourceCursor::new("abc\\(x");
        let token = lex_string_segment(&mut cursor)
            .expect("segment parse")
            .expect("text token");
        assert_eq!(token.kind, TokenKind::StringText);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 3);
        assert_eq!(cursor.remaining(), "\\(x");
    }

    #[test]
    fn lex_string_text_stops_before_string_end() {
        let mut cursor = SourceCursor::new("abc\"");
        let token = lex_string_segment(&mut cursor)
            .expect("segment parse")
            .expect("text token");
        assert_eq!(token.kind, TokenKind::StringText);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 3);
        assert_eq!(cursor.remaining(), "\"");
    }

    #[test]
    fn unterminated_string_reports_error() {
        let mut cursor = SourceCursor::new("abc");
        let err = lex_string_segment(&mut cursor).expect_err("expected error");
        assert!(matches!(err, StringLexError::UnterminatedString { .. }));
    }

    #[test]
    fn lex_interpolation_end_when_depth_is_zero() {
        let mut cursor = SourceCursor::new(")");
        let token = lex_interpolation_end(&mut cursor, 0).expect("end token");
        assert_eq!(token.kind, TokenKind::InterpolationEnd);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 1);
        assert!(cursor.is_eof());
    }

    #[test]
    fn does_not_lex_interpolation_end_when_depth_is_nonzero() {
        let mut cursor = SourceCursor::new(")");
        let start = cursor.offset();
        let token = lex_interpolation_end(&mut cursor, 1);
        assert!(token.is_none());
        assert_eq!(cursor.offset(), start);
    }

    #[test]
    fn string_and_char_tokens_have_exact_spans() {
        let mut char_cursor = SourceCursor::new("'é'x");
        let char_token = lex_char_literal(&mut char_cursor)
            .expect("char parse")
            .expect("char token");
        assert_eq!(char_token.kind, TokenKind::Char);
        assert_eq!(char_token.span.start, 0);
        assert_eq!(char_token.span.end, 4);

        let mut string_cursor = SourceCursor::new("\"a\\(x");
        let start = lex_string_start(&mut string_cursor).expect("start");
        assert_eq!(start.kind, TokenKind::StringStart);
        assert_eq!(start.span.start, 0);
        assert_eq!(start.span.end, 1);

        let text = lex_string_segment(&mut string_cursor)
            .expect("segment parse")
            .expect("text token");
        assert_eq!(text.kind, TokenKind::StringText);
        assert_eq!(text.span.start, 1);
        assert_eq!(text.span.end, 2);

        let interp = lex_string_segment(&mut string_cursor)
            .expect("segment parse")
            .expect("interp start");
        assert_eq!(interp.kind, TokenKind::InterpolationStart);
        assert_eq!(interp.span.start, 2);
        assert_eq!(interp.span.end, 4);
    }
}
