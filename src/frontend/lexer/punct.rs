//! Punctuation and operator lexing for the `coreX` frontend.
//!
//! This module lexes only punctuation/operator tokens. It does not lex
//! identifiers, keywords, numbers, strings, or chars.
//!
//! Longest-match ordering is explicit and critical for correctness, including:
//! - `..=` before `..` before `.`
//! - `<<=` before `<<` before `<`
//! - `>>=` before `>>` before `>`
//! - `::` before `:`
//! - `->` before `-`
//! - `=>` before `=`
//! - `==` before `=`
//! - `!=` before `!`
//! - `<=` before `<`
//! - `>=` before `>`
//! - `+=` before `+`
//! - `-=` before `-`
//! - `*=` before `*`
//! - `/=` before `/`
//! - `%=` before `%`
//! - `^=` before `^`
//! - `|=` before `|`
//! - `&=` before `&`
//! - `&&` before `&`
//! - `||` before `|`

use super::{SourceCursor, Token, TokenKind};

/// Lexes one punctuation/operator token at the current cursor position.
///
/// Returns `Some(Token)` when current input starts with a supported
/// punctuation/operator spelling and consumes the matching bytes.
///
/// Returns `None` without consuming input when current input does not begin
/// with a supported punctuation/operator token.
#[must_use]
pub fn lex_punct_or_operator(cursor: &mut SourceCursor<'_>) -> Option<Token> {
    let start = cursor.mark();

    let kind = if cursor.eat_str("..=") {
        TokenKind::DotDotEq
    } else if cursor.eat_str("..") {
        TokenKind::DotDot
    } else if cursor.eat_str("<<=") {
        TokenKind::ShlEq
    } else if cursor.eat_str(">>=") {
        TokenKind::ShrEq
    } else if cursor.eat_str("<<") {
        TokenKind::Shl
    } else if cursor.eat_str(">>") {
        TokenKind::Shr
    } else if cursor.eat_str("::") {
        TokenKind::ColonColon
    } else if cursor.eat_str("->") {
        TokenKind::Arrow
    } else if cursor.eat_str("=>") {
        TokenKind::FatArrow
    } else if cursor.eat_str("==") {
        TokenKind::EqEq
    } else if cursor.eat_str("!=") {
        TokenKind::BangEq
    } else if cursor.eat_str("<=") {
        TokenKind::Le
    } else if cursor.eat_str(">=") {
        TokenKind::Ge
    } else if cursor.eat_str("&&") {
        TokenKind::AmpAmp
    } else if cursor.eat_str("||") {
        TokenKind::PipePipe
    } else if cursor.eat_str("+=") {
        TokenKind::PlusEq
    } else if cursor.eat_str("-=") {
        TokenKind::MinusEq
    } else if cursor.eat_str("*=") {
        TokenKind::StarEq
    } else if cursor.eat_str("/=") {
        TokenKind::SlashEq
    } else if cursor.eat_str("%=") {
        TokenKind::PercentEq
    } else if cursor.eat_str("^=") {
        TokenKind::CaretEq
    } else if cursor.eat_str("|=") {
        TokenKind::PipeEq
    } else if cursor.eat_str("&=") {
        TokenKind::AmpEq
    } else if cursor.eat_if('(') {
        TokenKind::LParen
    } else if cursor.eat_if(')') {
        TokenKind::RParen
    } else if cursor.eat_if('{') {
        TokenKind::LBrace
    } else if cursor.eat_if('}') {
        TokenKind::RBrace
    } else if cursor.eat_if('[') {
        TokenKind::LBracket
    } else if cursor.eat_if(']') {
        TokenKind::RBracket
    } else if cursor.eat_if(',') {
        TokenKind::Comma
    } else if cursor.eat_if(';') {
        TokenKind::Semi
    } else if cursor.eat_if(':') {
        TokenKind::Colon
    } else if cursor.eat_if('.') {
        TokenKind::Dot
    } else if cursor.eat_if('=') {
        TokenKind::Eq
    } else if cursor.eat_if('+') {
        TokenKind::Plus
    } else if cursor.eat_if('-') {
        TokenKind::Minus
    } else if cursor.eat_if('*') {
        TokenKind::Star
    } else if cursor.eat_if('/') {
        TokenKind::Slash
    } else if cursor.eat_if('%') {
        TokenKind::Percent
    } else if cursor.eat_if('^') {
        TokenKind::Caret
    } else if cursor.eat_if('!') {
        TokenKind::Bang
    } else if cursor.eat_if('<') {
        TokenKind::Lt
    } else if cursor.eat_if('>') {
        TokenKind::Gt
    } else if cursor.eat_if('&') {
        TokenKind::Amp
    } else if cursor.eat_if('|') {
        TokenKind::Pipe
    } else if cursor.eat_if('?') {
        TokenKind::Question
    } else if cursor.eat_if('@') {
        TokenKind::At
    } else {
        return None;
    };

    Some(Token::new(kind, cursor.current_span_from(start)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_one(input: &str) -> Option<Token> {
        let mut cursor = SourceCursor::new(input);
        lex_punct_or_operator(&mut cursor)
    }

    #[test]
    fn lex_single_char_punctuators() {
        let cases = [
            ("(", TokenKind::LParen),
            (")", TokenKind::RParen),
            ("{", TokenKind::LBrace),
            ("}", TokenKind::RBrace),
            ("[", TokenKind::LBracket),
            ("]", TokenKind::RBracket),
            (",", TokenKind::Comma),
            (";", TokenKind::Semi),
            (":", TokenKind::Colon),
            (".", TokenKind::Dot),
            ("=", TokenKind::Eq),
            ("+", TokenKind::Plus),
            ("-", TokenKind::Minus),
            ("*", TokenKind::Star),
            ("/", TokenKind::Slash),
            ("%", TokenKind::Percent),
            ("^", TokenKind::Caret),
            ("!", TokenKind::Bang),
            ("<", TokenKind::Lt),
            (">", TokenKind::Gt),
            ("&", TokenKind::Amp),
            ("|", TokenKind::Pipe),
            ("?", TokenKind::Question),
            ("@", TokenKind::At),
        ];

        for (input, expected) in cases {
            let token = lex_one(input).expect("expected punct token");
            assert_eq!(token.kind, expected, "input: {input}");
            assert_eq!(token.span.start, 0, "input: {input}");
            assert_eq!(token.span.end, input.len(), "input: {input}");
        }
    }

    #[test]
    fn lex_multi_char_punctuators() {
        let cases = [
            ("..=", TokenKind::DotDotEq),
            ("..", TokenKind::DotDot),
            ("::", TokenKind::ColonColon),
            ("->", TokenKind::Arrow),
            ("=>", TokenKind::FatArrow),
            ("==", TokenKind::EqEq),
            ("!=", TokenKind::BangEq),
            ("<=", TokenKind::Le),
            (">=", TokenKind::Ge),
            ("&&", TokenKind::AmpAmp),
            ("||", TokenKind::PipePipe),
            ("+=", TokenKind::PlusEq),
            ("-=", TokenKind::MinusEq),
            ("*=", TokenKind::StarEq),
            ("/=", TokenKind::SlashEq),
            ("%=", TokenKind::PercentEq),
            ("^=", TokenKind::CaretEq),
            ("|=", TokenKind::PipeEq),
            ("&=", TokenKind::AmpEq),
            ("<<", TokenKind::Shl),
            (">>", TokenKind::Shr),
            ("<<=", TokenKind::ShlEq),
            (">>=", TokenKind::ShrEq),
        ];

        for (input, expected) in cases {
            let token = lex_one(input).expect("expected punct token");
            assert_eq!(token.kind, expected, "input: {input}");
            assert_eq!(token.span.start, 0, "input: {input}");
            assert_eq!(token.span.end, input.len(), "input: {input}");
        }
    }

    #[test]
    fn longest_match_prefers_dotdoteq_over_dotdot() {
        let mut cursor = SourceCursor::new("..=");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::DotDotEq);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_dotdot_over_dot() {
        let mut cursor = SourceCursor::new("..x");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::DotDot);
        assert_eq!(cursor.peek(), Some('x'));
    }

    #[test]
    fn longest_match_prefers_coloncolon_over_colon() {
        let mut cursor = SourceCursor::new("::");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::ColonColon);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_arrow_over_minus() {
        let mut cursor = SourceCursor::new("->");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::Arrow);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_fatarrow_over_eq() {
        let mut cursor = SourceCursor::new("=>");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::FatArrow);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_eqeq_over_eq() {
        let mut cursor = SourceCursor::new("==");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::EqEq);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_bangeq_over_bang() {
        let mut cursor = SourceCursor::new("!=");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::BangEq);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_le_over_lt() {
        let mut cursor = SourceCursor::new("<=");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::Le);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_ge_over_gt() {
        let mut cursor = SourceCursor::new(">=");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::Ge);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_andand_over_amp() {
        let mut cursor = SourceCursor::new("&&");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::AmpAmp);
        assert!(cursor.is_eof());
    }

    #[test]
    fn lex_new_single_char_bitwise_punctuators() {
        let caret = lex_one("^").expect("caret");
        assert_eq!(caret.kind, TokenKind::Caret);
        assert_eq!(caret.span.end, 1);

        let pipe = lex_one("|").expect("pipe");
        assert_eq!(pipe.kind, TokenKind::Pipe);
        assert_eq!(pipe.span.end, 1);
    }

    #[test]
    fn lex_new_multi_char_assignment_and_shift_punctuators() {
        let cases = [
            ("+=", TokenKind::PlusEq),
            ("-=", TokenKind::MinusEq),
            ("*=", TokenKind::StarEq),
            ("/=", TokenKind::SlashEq),
            ("%=", TokenKind::PercentEq),
            ("^=", TokenKind::CaretEq),
            ("|=", TokenKind::PipeEq),
            ("&=", TokenKind::AmpEq),
            ("<<", TokenKind::Shl),
            (">>", TokenKind::Shr),
            ("<<=", TokenKind::ShlEq),
            (">>=", TokenKind::ShrEq),
        ];

        for (input, expected) in cases {
            let token = lex_one(input).expect("expected token");
            assert_eq!(token.kind, expected, "input: {input}");
            assert_eq!(token.span.end, input.len(), "input: {input}");
        }
    }

    #[test]
    fn longest_match_prefers_shleq_over_shl() {
        let mut cursor = SourceCursor::new("<<=");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::ShlEq);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_shreq_over_shr() {
        let mut cursor = SourceCursor::new(">>=");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::ShrEq);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_shl_over_lt() {
        let mut cursor = SourceCursor::new("<<x");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::Shl);
        assert_eq!(cursor.peek(), Some('x'));
    }

    #[test]
    fn longest_match_prefers_shr_over_gt() {
        let mut cursor = SourceCursor::new(">>x");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::Shr);
        assert_eq!(cursor.peek(), Some('x'));
    }

    #[test]
    fn longest_match_prefers_pluseq_over_plus() {
        let mut cursor = SourceCursor::new("+=");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::PlusEq);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_pipeeq_over_pipe() {
        let mut cursor = SourceCursor::new("|=");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::PipeEq);
        assert!(cursor.is_eof());
    }

    #[test]
    fn longest_match_prefers_ampeq_over_amp() {
        let mut cursor = SourceCursor::new("&=");
        let token = lex_punct_or_operator(&mut cursor).expect("token");
        assert_eq!(token.kind, TokenKind::AmpEq);
        assert!(cursor.is_eof());
    }

    #[test]
    fn returns_none_without_consuming_for_non_punct() {
        let mut cursor = SourceCursor::new("abc");
        let start = cursor.offset();
        let token = lex_punct_or_operator(&mut cursor);
        assert!(token.is_none());
        assert_eq!(cursor.offset(), start);
    }

    #[test]
    fn returned_span_matches_consumed_bytes() {
        let mut multi = SourceCursor::new("..=x");
        let multi_token = lex_punct_or_operator(&mut multi).expect("multi");
        assert_eq!(multi_token.kind, TokenKind::DotDotEq);
        assert_eq!(multi_token.span.start, 0);
        assert_eq!(multi_token.span.end, 3);
        assert_eq!(multi.offset(), 3);

        let mut single = SourceCursor::new("@x");
        let single_token = lex_punct_or_operator(&mut single).expect("single");
        assert_eq!(single_token.kind, TokenKind::At);
        assert_eq!(single_token.span.start, 0);
        assert_eq!(single_token.span.end, 1);
        assert_eq!(single.offset(), 1);

        let mut shl_eq = SourceCursor::new("<<=x");
        let shl_eq_token = lex_punct_or_operator(&mut shl_eq).expect("shl_eq");
        assert_eq!(shl_eq_token.kind, TokenKind::ShlEq);
        assert_eq!(shl_eq_token.span.start, 0);
        assert_eq!(shl_eq_token.span.end, 3);

        let mut pipe_eq = SourceCursor::new("|=x");
        let pipe_eq_token =
            lex_punct_or_operator(&mut pipe_eq).expect("pipe_eq");
        assert_eq!(pipe_eq_token.kind, TokenKind::PipeEq);
        assert_eq!(pipe_eq_token.span.start, 0);
        assert_eq!(pipe_eq_token.span.end, 2);

        let mut caret = SourceCursor::new("^x");
        let caret_token = lex_punct_or_operator(&mut caret).expect("caret");
        assert_eq!(caret_token.kind, TokenKind::Caret);
        assert_eq!(caret_token.span.start, 0);
        assert_eq!(caret_token.span.end, 1);
    }
}
