//! Identifier-like lexing for the `coreX` frontend.
//!
//! This module lexes:
//! - ordinary identifiers
//! - reserved keywords (via existing keyword classification helpers)
//! - closure shorthand params (`$0`, `$1`, ...)
//! - lifetime names (`'a`, `'static`)
//!
//! Builtin primitive type names remain ordinary identifiers lexically.
//! They are recognized as builtins in later semantic analysis.

use super::{SourceCursor, Span, Token, TokenKind, classify_keyword_token};

/// Lexes one identifier-like token at the current cursor position.
///
/// This function returns:
/// - keyword token kinds for reserved keyword spellings
/// - [`TokenKind::Ident`] for non-keyword identifiers
/// - [`TokenKind::ClosureShorthandParam`] for `$` followed by one or more
///   ASCII digits
///
/// Returns `None` without consuming input when current position is not
/// identifier-like for this module.
#[must_use]
pub fn lex_ident_like(cursor: &mut SourceCursor<'_>) -> Option<Token> {
    if let Some(token) = lex_closure_shorthand_param(cursor) {
        return Some(token);
    }

    let first = cursor.peek()?;
    if !is_ident_start(first) {
        return None;
    }

    let start = cursor.mark();
    let _ = cursor.bump();
    cursor.eat_while(is_ident_continue);

    let spelling = cursor.slice_from(start);
    let kind = classify_keyword_token(spelling).unwrap_or(TokenKind::Ident);
    Some(Token::new(kind, cursor.current_span_from(start)))
}

/// Lexes a lifetime token like `'a` or `'static`.
///
/// Returns `None` without consuming input when current position does not
/// start with `'` followed by an identifier.
#[must_use]
pub fn lex_lifetime(cursor: &mut SourceCursor<'_>) -> Option<Token> {
    if cursor.peek() != Some('\'') {
        return None;
    }

    let start = cursor.mark();
    let _ = cursor.bump(); // consume '

    // Lifetime must be followed by an identifier start
    let next = cursor.peek()?;
    if !is_ident_start(next) {
        // Not a valid lifetime - backtrack and return None
        cursor.reset(start);
        return None;
    }

    // Lex the identifier part of the lifetime
    let ident_start = cursor.mark();
    let _ = cursor.bump();
    cursor.eat_while(is_ident_continue);

    let lifetime_name = cursor.slice_from(ident_start);
    // Lifetimes cannot be keywords
    if classify_keyword_token(lifetime_name).is_some() {
        cursor.reset(start);
        return None;
    }

    Some(Token::new(
        TokenKind::Lifetime,
        cursor.current_span_from(start),
    ))
}

fn lex_closure_shorthand_param(cursor: &mut SourceCursor<'_>) -> Option<Token> {
    if cursor.peek() != Some('$') {
        return None;
    }
    let next = cursor.peek_next()?;
    if !next.is_ascii_digit() {
        return None;
    }

    let start = cursor.mark();
    let _ = cursor.bump();
    cursor.eat_while(|ch| ch.is_ascii_digit());
    Some(Token::new(
        TokenKind::ClosureShorthandParam,
        cursor.current_span_from(start),
    ))
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_plain_identifier() {
        let mut cursor = SourceCursor::new("hello");
        let token = lex_ident_like(&mut cursor).expect("identifier");
        assert_eq!(token.kind, TokenKind::Ident);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 5);
    }

    #[test]
    fn lex_identifier_with_underscore_and_digits() {
        let mut cursor = SourceCursor::new("_foo123");
        let token = lex_ident_like(&mut cursor).expect("identifier");
        assert_eq!(token.kind, TokenKind::Ident);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 7);
    }

    #[test]
    fn classifies_reserved_keywords() {
        let cases = [
            ("macro", TokenKind::KwMacro),
            ("rule", TokenKind::KwRule),
            ("reflect", TokenKind::KwReflect),
            ("fn", TokenKind::KwFn),
            ("scope", TokenKind::KwScope),
            ("struct", TokenKind::KwStruct),
            ("self", TokenKind::KwSelfValue),
            ("Self", TokenKind::KwSelfType),
            ("try", TokenKind::KwTry),
            ("unsafe", TokenKind::KwUnsafe),
            ("await", TokenKind::KwAwait),
            ("as", TokenKind::KwAs),
            ("return", TokenKind::KwReturn),
        ];

        for (input, expected) in cases {
            let mut cursor = SourceCursor::new(input);
            let token = lex_ident_like(&mut cursor).expect("keyword");
            assert_eq!(token.kind, expected, "input: {input}");
            assert_eq!(token.span.start, 0, "input: {input}");
            assert_eq!(token.span.end, input.len(), "input: {input}");
        }
    }

    #[test]
    fn builtin_primitive_type_names_remain_identifiers() {
        for input in ["u8", "i32", "f64", "bool", "char", "string", "void"] {
            let mut cursor = SourceCursor::new(input);
            let token = lex_ident_like(&mut cursor).expect("identifier");
            assert_eq!(token.kind, TokenKind::Ident, "input: {input}");
            assert_eq!(token.span.start, 0, "input: {input}");
            assert_eq!(token.span.end, input.len(), "input: {input}");
        }
    }

    #[test]
    fn lex_closure_shorthand_param_single_digit() {
        let mut cursor = SourceCursor::new("$0");
        let token =
            lex_closure_shorthand_param(&mut cursor).expect("shorthand");
        assert_eq!(token.kind, TokenKind::ClosureShorthandParam);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_closure_shorthand_param_multi_digit() {
        let mut cursor = SourceCursor::new("$12");
        let token =
            lex_closure_shorthand_param(&mut cursor).expect("shorthand");
        assert_eq!(token.kind, TokenKind::ClosureShorthandParam);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 3);
        assert!(cursor.is_eof());
    }

    #[test]
    fn closure_shorthand_stops_before_identifier_tail() {
        let mut cursor = SourceCursor::new("$0abc");
        let token =
            lex_closure_shorthand_param(&mut cursor).expect("shorthand");
        assert_eq!(token.kind, TokenKind::ClosureShorthandParam);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 2);
        assert_eq!(cursor.remaining(), "abc");
    }

    #[test]
    fn returns_none_without_consuming_for_non_ident_start() {
        for input in ["1abc", "(", ".."] {
            let mut cursor = SourceCursor::new(input);
            let start = cursor.offset();
            let token = lex_ident_like(&mut cursor);
            assert!(token.is_none(), "input: {input}");
            assert_eq!(cursor.offset(), start, "input: {input}");
        }
    }

    #[test]
    fn returns_none_without_consuming_for_invalid_dollar_forms() {
        for input in ["$", "$x"] {
            let mut cursor = SourceCursor::new(input);
            let start = cursor.offset();
            let token = lex_ident_like(&mut cursor);
            assert!(token.is_none(), "input: {input}");
            assert_eq!(cursor.offset(), start, "input: {input}");
        }
    }

    #[test]
    fn returned_span_matches_consumed_bytes() {
        let mut ident = SourceCursor::new("hello1 ");
        let ident_token = lex_ident_like(&mut ident).expect("identifier");
        assert_eq!(ident_token.kind, TokenKind::Ident);
        assert_eq!(ident_token.span.start, 0);
        assert_eq!(ident_token.span.end, 6);

        let mut keyword = SourceCursor::new("return;");
        let keyword_token = lex_ident_like(&mut keyword).expect("keyword");
        assert_eq!(keyword_token.kind, TokenKind::KwReturn);
        assert_eq!(keyword_token.span.start, 0);
        assert_eq!(keyword_token.span.end, 6);

        let mut shorthand = SourceCursor::new("$12x");
        let shorthand_token =
            lex_closure_shorthand_param(&mut shorthand).expect("shorthand");
        assert_eq!(shorthand_token.kind, TokenKind::ClosureShorthandParam);
        assert_eq!(shorthand_token.span.start, 0);
        assert_eq!(shorthand_token.span.end, 3);
    }

    #[test]
    fn identifier_does_not_classify_prefix_keyword() {
        let mut cursor = SourceCursor::new("fnName");
        let token = lex_ident_like(&mut cursor).expect("identifier");
        assert_eq!(token.kind, TokenKind::Ident);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_simple_lifetime() {
        let mut cursor = SourceCursor::new("'a");
        let token = lex_lifetime(&mut cursor).expect("lifetime");
        assert_eq!(token.kind, TokenKind::Lifetime);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 2);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_lifetime_longer_name() {
        let mut cursor = SourceCursor::new("'static");
        let token = lex_lifetime(&mut cursor).expect("lifetime");
        assert_eq!(token.kind, TokenKind::Lifetime);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 7);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_lifetime_with_underscore() {
        let mut cursor = SourceCursor::new("'_");
        let token = lex_lifetime(&mut cursor).expect("lifetime");
        assert_eq!(token.kind, TokenKind::Lifetime);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 2);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_lifetime_followed_by_type() {
        let mut cursor = SourceCursor::new("'a T");
        let token = lex_lifetime(&mut cursor).expect("lifetime");
        assert_eq!(token.kind, TokenKind::Lifetime);
        assert_eq!(token.span.start, 0);
        assert_eq!(token.span.end, 2);
        assert_eq!(cursor.remaining(), " T");
    }

    #[test]
    fn lifetime_does_not_lex_keyword() {
        for keyword in ["'fn", "'struct", "'self"] {
            let mut cursor = SourceCursor::new(keyword);
            let token = lex_lifetime(&mut cursor);
            assert!(token.is_none(), "input: {keyword}");
            assert_eq!(cursor.offset(), 0, "input: {keyword}");
        }
    }

    #[test]
    fn lifetime_does_not_lex_empty() {
        let mut cursor = SourceCursor::new("''");
        let token = lex_lifetime(&mut cursor);
        assert!(token.is_none());
        assert_eq!(cursor.offset(), 0);
    }

    #[test]
    fn lifetime_does_not_lex_number() {
        let mut cursor = SourceCursor::new("'1");
        let token = lex_lifetime(&mut cursor);
        assert!(token.is_none());
        assert_eq!(cursor.offset(), 0);
    }
}
