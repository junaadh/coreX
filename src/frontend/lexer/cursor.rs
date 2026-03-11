//! UTF-8-safe source traversal utilities for the lexer.
//!
//! `SourceCursor` provides byte-offset based character traversal over `&str`
//! input. This layer is intentionally separate from tokenization: it does not
//! emit tokens or classify lexical categories.
//!
//! It provides safe primitives for peeking, consuming, prefix matching, and
//! source-slice/span capture.

use super::Span;

/// UTF-8-safe source cursor over immutable source text.
///
/// - `source` is immutable source text.
/// - `offset` is the current byte offset into `source`.
/// - movement is char-aware and advances by UTF-8 byte length.
#[derive(Debug, Clone)]
pub struct SourceCursor<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> SourceCursor<'a> {
    /// Creates a new cursor at byte offset `0`.
    #[must_use]
    pub const fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    /// Returns the full source buffer.
    #[must_use]
    pub const fn source(&self) -> &'a str {
        self.source
    }

    /// Returns the current byte offset.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns `true` when the cursor has reached end-of-input.
    #[must_use]
    pub fn is_eof(&self) -> bool {
        self.offset >= self.source.len()
    }

    /// Returns the remaining source slice from current offset to EOF.
    #[must_use]
    pub fn remaining(&self) -> &'a str {
        debug_assert!(self.source.is_char_boundary(self.offset));
        &self.source[self.offset..]
    }

    /// Returns the current character without consuming it.
    #[must_use]
    pub fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    /// Returns the next character after the current one without consuming.
    ///
    /// This is UTF-8 aware and does not assume one-byte characters.
    #[must_use]
    pub fn peek_next(&self) -> Option<char> {
        self.peek_nth(1)
    }

    /// Returns the `n`th character from current cursor position.
    #[must_use]
    pub fn peek_nth(&self, n: usize) -> Option<char> {
        self.remaining().chars().nth(n)
    }

    /// Consumes and returns the current character.
    ///
    /// On success, advances offset by `char.len_utf8()`. Returns `None` at EOF.
    pub fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    /// Consumes `ch` when it is the current character.
    pub fn eat_if(&mut self, ch: char) -> bool {
        self.eat_if_predicate(|candidate| candidate == ch)
    }

    /// Consumes current character when `pred` returns `true`.
    pub fn eat_if_predicate(
        &mut self,
        pred: impl FnOnce(char) -> bool,
    ) -> bool {
        let Some(ch) = self.peek() else {
            return false;
        };
        if pred(ch) {
            let consumed = self.bump();
            debug_assert_eq!(consumed, Some(ch));
            true
        } else {
            false
        }
    }

    #[must_use]
    pub fn starts_with(&self, s: &str) -> bool {
        self.remaining().starts_with(s)
    }

    /// Consumes `s` if it matches the remaining input exactly.
    ///
    /// Returns `true` on match and advances by `s.len()` bytes.
    pub fn eat_str(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.offset += s.len();
            debug_assert!(self.source.is_char_boundary(self.offset));
            true
        } else {
            false
        }
    }

    #[must_use]
    pub const fn mark(&self) -> usize {
        self.offset
    }

    /// Returns source slice from `start` up to current cursor offset.
    ///
    /// `start` should come from `mark()` on this cursor instance.
    #[must_use]
    pub fn slice_from(&self, start: usize) -> &'a str {
        debug_assert!(start <= self.offset);
        debug_assert!(self.source.is_char_boundary(start));
        debug_assert!(self.source.is_char_boundary(self.offset));
        &self.source[start..self.offset]
    }

    /// Builds a span from prior mark to current cursor offset.
    #[must_use]
    pub fn current_span_from(&self, start: usize) -> Span {
        Span::new(start, self.offset)
    }

    /// Consumes characters while `pred` returns `true`.
    pub fn eat_while(&mut self, pred: impl Fn(char) -> bool) {
        while let Some(ch) = self.peek() {
            if !pred(ch) {
                break;
            }
            let consumed = self.bump();
            debug_assert_eq!(consumed, Some(ch));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cursor_starts_at_zero() {
        let cursor = SourceCursor::new("abc");
        assert_eq!(cursor.offset(), 0);
        assert!(!cursor.is_eof());
    }

    #[test]
    fn peek_and_bump_ascii() {
        let mut cursor = SourceCursor::new("abc");

        assert_eq!(cursor.peek(), Some('a'));
        assert_eq!(cursor.bump(), Some('a'));
        assert_eq!(cursor.offset(), 1);

        assert_eq!(cursor.peek(), Some('b'));
        assert_eq!(cursor.bump(), Some('b'));
        assert_eq!(cursor.offset(), 2);

        assert_eq!(cursor.peek(), Some('c'));
        assert_eq!(cursor.bump(), Some('c'));
        assert_eq!(cursor.offset(), 3);
        assert!(cursor.is_eof());
    }

    #[test]
    fn peek_and_bump_utf8() {
        let mut cursor = SourceCursor::new("aé中");

        assert_eq!(cursor.peek(), Some('a'));
        assert_eq!(cursor.bump(), Some('a'));
        assert_eq!(cursor.offset(), 1);

        assert_eq!(cursor.peek(), Some('é'));
        assert_eq!(cursor.bump(), Some('é'));
        assert_eq!(cursor.offset(), 3);

        assert_eq!(cursor.peek(), Some('中'));
        assert_eq!(cursor.bump(), Some('中'));
        assert_eq!(cursor.offset(), 6);
        assert!(cursor.is_eof());
    }

    #[test]
    fn peek_next_works() {
        let cursor = SourceCursor::new("aé中");
        assert_eq!(cursor.peek(), Some('a'));
        assert_eq!(cursor.peek_next(), Some('é'));
        assert_eq!(cursor.peek_nth(2), Some('中'));

        let mut cursor = SourceCursor::new("éx");
        assert_eq!(cursor.peek_next(), Some('x'));
        assert_eq!(cursor.bump(), Some('é'));
        assert_eq!(cursor.peek_next(), None);
    }

    #[test]
    fn eat_if_consumes_matching_char() {
        let mut cursor = SourceCursor::new("abc");
        assert!(cursor.eat_if('a'));
        assert_eq!(cursor.offset(), 1);
        assert!(!cursor.eat_if('z'));
        assert_eq!(cursor.offset(), 1);
        assert!(cursor.eat_if_predicate(|ch| ch == 'b'));
        assert_eq!(cursor.offset(), 2);
    }

    #[test]
    fn starts_with_and_eat_str_work() {
        let mut cursor = SourceCursor::new("..=::..");
        assert!(cursor.starts_with("..="));
        assert!(cursor.eat_str("..="));
        assert_eq!(cursor.offset(), 3);
        assert!(cursor.starts_with("::"));
        assert!(cursor.eat_str("::"));
        assert_eq!(cursor.offset(), 5);
        assert!(cursor.starts_with(".."));
        assert!(cursor.eat_str(".."));
        assert!(cursor.is_eof());
    }

    #[test]
    fn mark_and_slice_from_capture_text() {
        let mut cursor = SourceCursor::new("hello é");
        let start = cursor.mark();
        cursor.eat_while(|ch| ch != ' ');
        assert_eq!(cursor.slice_from(start), "hello");
        assert!(cursor.eat_if(' '));
        let mid = cursor.mark();
        assert_eq!(cursor.bump(), Some('é'));
        assert_eq!(cursor.slice_from(mid), "é");
    }

    #[test]
    fn current_span_from_uses_byte_offsets() {
        let mut cursor = SourceCursor::new("aé");
        let start = cursor.mark();
        assert_eq!(cursor.bump(), Some('a'));
        assert_eq!(cursor.bump(), Some('é'));
        let span = cursor.current_span_from(start);
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 3);
    }

    #[test]
    fn eof_behavior_is_stable() {
        let mut cursor = SourceCursor::new("");
        assert!(cursor.is_eof());
        assert_eq!(cursor.peek(), None);
        assert_eq!(cursor.peek_next(), None);
        assert_eq!(cursor.bump(), None);
        assert_eq!(cursor.bump(), None);
        assert!(!cursor.eat_if('x'));
        assert!(!cursor.eat_str("abc"));
        assert!(cursor.is_eof());
        assert_eq!(cursor.offset(), 0);
    }
}
