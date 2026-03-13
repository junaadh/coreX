//! Numeric literal lexing for the `coreX` frontend.
//!
//! This module lexes integer and float literal spellings and emits
//! source-preserving numeric token kinds.
//!
//! Supported surface includes:
//! - decimal, hex (`0x`), explicit octal (`0o`), and legacy leading-zero
//!   integer forms
//! - `_` separators in numeric spelling
//! - integer suffix spelling such as `87u8` and `87_u8`
//! - decimal floats with optional exponent forms
//!
//! Float/range disambiguation rule:
//! - a `.` starts the float fractional part only when followed by an ASCII
//!   digit (`1.25` is float, `1..3` leaves `..` for punct lexing)
//!
//! Integer suffix handling is lexical only in this module. Semantic validation
//! is deferred to semantic analysis.

use super::{SourceCursor, Token, TokenKind};

/// Lexes one numeric literal at the current cursor position.
///
/// Returns [`TokenKind::Integer`] or [`TokenKind::Float`] when current input
/// begins with an ASCII-digit numeric literal spelling.
///
/// Returns `None` without consuming input when current input does not begin
/// with a number.
#[must_use]
pub fn lex_number(cursor: &mut SourceCursor<'_>) -> Option<Token> {
    let first = cursor.peek()?;
    if !first.is_ascii_digit() {
        return None;
    }

    let start = cursor.mark();

    if cursor.starts_with("0x")
        && consume_prefixed_digits(cursor, "0x", is_hex_digit)
    {
        consume_integer_suffix(cursor);
        return Some(Token::new(
            TokenKind::Integer,
            cursor.current_span_from(start),
        ));
    }

    if cursor.starts_with("0o")
        && consume_prefixed_digits(cursor, "0o", is_octal_digit)
    {
        consume_integer_suffix(cursor);
        return Some(Token::new(
            TokenKind::Integer,
            cursor.current_span_from(start),
        ));
    }

    let _ = consume_digits_with_separators(cursor, is_dec_digit);

    let mut is_float = false;
    if consume_fractional_part(cursor) {
        is_float = true;
    }
    if consume_exponent(cursor) {
        is_float = true;
    }

    if !is_float {
        consume_integer_suffix(cursor);
    }

    let kind = if is_float {
        TokenKind::Float
    } else {
        TokenKind::Integer
    };
    Some(Token::new(kind, cursor.current_span_from(start)))
}

fn consume_prefixed_digits(
    cursor: &mut SourceCursor<'_>,
    prefix: &str,
    is_digit: fn(char) -> bool,
) -> bool {
    if !cursor.starts_with(prefix) {
        return false;
    }

    let mut probe = cursor.clone();
    let consumed_prefix = probe.eat_str(prefix);
    debug_assert!(consumed_prefix);
    if !consume_digits_with_separators(&mut probe, is_digit) {
        return false;
    }
    *cursor = probe;
    true
}

fn consume_fractional_part(cursor: &mut SourceCursor<'_>) -> bool {
    if cursor.peek() != Some('.') {
        return false;
    }
    let Some(next) = cursor.peek_next() else {
        return false;
    };
    if !next.is_ascii_digit() {
        return false;
    }

    let dot = cursor.bump();
    debug_assert_eq!(dot, Some('.'));
    let consumed = consume_digits_with_separators(cursor, is_dec_digit);
    debug_assert!(consumed);
    true
}

fn consume_exponent(cursor: &mut SourceCursor<'_>) -> bool {
    let mut probe = cursor.clone();
    let Some(marker) = probe.peek() else {
        return false;
    };
    if marker != 'e' && marker != 'E' {
        return false;
    }
    let _ = probe.bump();

    if matches!(probe.peek(), Some('+' | '-')) {
        let _ = probe.bump();
    }

    if !consume_digits_with_separators(&mut probe, is_dec_digit) {
        return false;
    }

    *cursor = probe;
    true
}

fn consume_integer_suffix(cursor: &mut SourceCursor<'_>) {
    const SUFFIXES: [&str; 10] = [
        "usize", "isize", "u64", "u32", "u16", "u8", "i64", "i32", "i16", "i8",
    ];

    let mut probe = cursor.clone();
    let _ = probe.eat_if('_');

    for suffix in SUFFIXES {
        if !probe.starts_with(suffix) {
            continue;
        }

        let mut with_suffix = probe.clone();
        let _ = with_suffix.eat_str(suffix);
        let at_boundary = match with_suffix.peek() {
            Some(ch) => !is_ident_continue(ch),
            None => true,
        };

        if at_boundary {
            *cursor = with_suffix;
            return;
        }
    }
}

fn consume_digits_with_separators(
    cursor: &mut SourceCursor<'_>,
    is_digit: fn(char) -> bool,
) -> bool {
    let Some(first) = cursor.peek() else {
        return false;
    };
    if !is_digit(first) {
        return false;
    }

    let _ = cursor.bump();
    loop {
        let Some(ch) = cursor.peek() else {
            break;
        };
        if is_digit(ch) {
            let _ = cursor.bump();
            continue;
        }
        if ch == '_' {
            let Some(next) = cursor.peek_next() else {
                break;
            };
            if is_digit(next) {
                let _ = cursor.bump();
                continue;
            }
        }
        break;
    }
    true
}

fn is_dec_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

fn is_hex_digit(ch: char) -> bool {
    ch.is_ascii_hexdigit()
}

fn is_octal_digit(ch: char) -> bool {
    matches!(ch, '0'..='7')
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_decimal_integer() {
        let mut cursor = SourceCursor::new("123");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_decimal_integer_with_separators() {
        let mut cursor = SourceCursor::new("1_000");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_hex_integer() {
        let mut cursor = SourceCursor::new("0x7A");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_hex_integer_with_separators() {
        let mut cursor = SourceCursor::new("0xFF_FF");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_explicit_octal_integer() {
        let mut cursor = SourceCursor::new("0o65");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_legacy_octal_integer() {
        let mut cursor = SourceCursor::new("044");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_integer_with_suffix() {
        let mut cursor = SourceCursor::new("87u8");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_integer_with_underscore_suffix() {
        let mut cursor = SourceCursor::new("87_u8");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_simple_float() {
        let mut cursor = SourceCursor::new("1.25");
        let token = lex_number(&mut cursor).expect("float");
        assert_eq!(token.kind, TokenKind::Float);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_float_with_exponent() {
        let mut cursor = SourceCursor::new("1e9");
        let token = lex_number(&mut cursor).expect("float");
        assert_eq!(token.kind, TokenKind::Float);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_float_with_fraction_and_exponent() {
        let mut cursor = SourceCursor::new("1.0e-3");
        let token = lex_number(&mut cursor).expect("float");
        assert_eq!(token.kind, TokenKind::Float);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_float_with_uppercase_exponent_and_sign() {
        let mut cursor = SourceCursor::new("2E+10");
        let token = lex_number(&mut cursor).expect("float");
        assert_eq!(token.kind, TokenKind::Float);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_float_with_separators() {
        let mut cursor = SourceCursor::new("1_000.25");
        let token = lex_number(&mut cursor).expect("float");
        assert_eq!(token.kind, TokenKind::Float);
        assert!(cursor.is_eof());
    }

    #[test]
    fn does_not_consume_range_start_as_float() {
        let mut cursor = SourceCursor::new("1..3");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 1);
        assert_eq!(cursor.remaining(), "..3");
    }

    #[test]
    fn does_not_consume_inclusive_range_start_as_float() {
        let mut cursor = SourceCursor::new("1..=3");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 1);
        assert_eq!(cursor.remaining(), "..=3");
    }

    #[test]
    fn returns_none_without_consuming_for_non_number_start() {
        for input in ["abc", ".5", "@"] {
            let mut cursor = SourceCursor::new(input);
            let start = cursor.offset();
            let token = lex_number(&mut cursor);
            assert!(token.is_none(), "input: {input}");
            assert_eq!(cursor.offset(), start, "input: {input}");
        }
    }

    #[test]
    fn returned_span_matches_consumed_bytes() {
        let mut integer = SourceCursor::new("87_u8+");
        let int_token = lex_number(&mut integer).expect("integer");
        assert_eq!(int_token.kind, TokenKind::Integer);
        assert_eq!(int_token.span.start, 0);
        assert_eq!(int_token.span.end, 5);

        let mut float = SourceCursor::new("1.0e-3)");
        let float_token = lex_number(&mut float).expect("float");
        assert_eq!(float_token.kind, TokenKind::Float);
        assert_eq!(float_token.span.start, 0);
        assert_eq!(float_token.span.end, 6);
    }

    #[test]
    fn integer_suffix_requires_identifier_boundary() {
        let mut cursor = SourceCursor::new("87_u8abc");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 2);
        assert_eq!(cursor.remaining(), "_u8abc");
    }

    #[test]
    fn integer_suffix_consumes_when_boundary_exists() {
        let mut cursor = SourceCursor::new("87_u8,");
        let token = lex_number(&mut cursor).expect("integer");
        assert_eq!(token.kind, TokenKind::Integer);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 5);
        assert_eq!(cursor.remaining(), ",");
    }
}
