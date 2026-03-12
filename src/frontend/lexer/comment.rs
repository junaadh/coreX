//! Lexer trivia handling for whitespace and comments.
//!
//! This module consumes lexical trivia on top of [`SourceCursor`]:
//! - whitespace
//! - line comments
//! - block comments
//!
//! Comments are classified but not emitted as ordinary tokens here. Doc comment
//! forms remain distinguishable for later attachment/indexing passes.
//!
//! Nested block comments are intentionally unsupported in this surface.

use super::{CommentKind, SourceCursor, Span};
use std::fmt::{Display, Formatter};

/// Source-preserving classified comment.
///
/// - `kind` identifies normal/doc/inner-doc line or block form.
/// - `span` covers the full comment source spelling.
/// - `text` is the original source slice for the comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment<'a> {
    pub kind: CommentKind,
    pub span: Span,
    pub text: &'a str,
}

/// Returns `true` when comment kind is a doc-comment form.
#[must_use]
pub const fn is_doc_comment_kind(kind: CommentKind) -> bool {
    matches!(
        kind,
        CommentKind::DocLine
            | CommentKind::DocBlock
            | CommentKind::InnerDocLine
            | CommentKind::InnerDocBlock
    )
}

/// Returns `true` when comment kind is an outer doc-comment form.
#[must_use]
pub const fn is_outer_doc_comment_kind(kind: CommentKind) -> bool {
    matches!(kind, CommentKind::DocLine | CommentKind::DocBlock)
}

/// Comment-consumption errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentError {
    /// A block comment started with `/*`, `/**`, or `/*!` and reached EOF
    /// before finding a closing `*/`.
    UnterminatedBlockComment { span: Span },
}

impl Display for CommentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnterminatedBlockComment { span } => write!(
                f,
                "unterminated block comment at byte range {}..{}",
                span.start, span.end
            ),
        }
    }
}

impl std::error::Error for CommentError {}

/// Consumes Unicode whitespace trivia.
pub fn skip_whitespace(cursor: &mut SourceCursor<'_>) {
    cursor.eat_while(char::is_whitespace);
}

/// Consumes a line comment when present at current offset.
///
/// Supported prefixes:
/// - `///` => [`CommentKind::DocLine`]
/// - `//!` => [`CommentKind::InnerDocLine`]
/// - `//`  => [`CommentKind::Line`]
///
/// This function consumes comment text through end-of-line/EOF and leaves the
/// terminating newline untouched.
#[must_use]
pub fn consume_line_comment<'a>(
    cursor: &mut SourceCursor<'a>,
) -> Option<Comment<'a>> {
    if !cursor.starts_with("//") {
        return None;
    }

    let start = cursor.mark();
    let kind = if cursor.starts_with("///") {
        let ate = cursor.eat_str("///");
        debug_assert!(ate);
        CommentKind::DocLine
    } else if cursor.starts_with("//!") {
        let ate = cursor.eat_str("//!");
        debug_assert!(ate);
        CommentKind::InnerDocLine
    } else {
        let ate = cursor.eat_str("//");
        debug_assert!(ate);
        CommentKind::Line
    };

    cursor.eat_while(|ch| ch != '\n' && ch != '\r');
    let span = cursor.current_span_from(start);
    let text = cursor.slice_from(start);
    Some(Comment { kind, span, text })
}

/// Consumes a block comment when present at current offset.
///
/// Supported prefixes:
/// - `/**` => [`CommentKind::DocBlock`]
/// - `/*!` => [`CommentKind::InnerDocBlock`]
/// - `/*`  => [`CommentKind::Block`]
///
/// Nested block comments are not supported. Consumption stops at the first
/// `*/`.
///
/// # Errors
/// Returns [`CommentError::UnterminatedBlockComment`] if EOF is reached before
/// a closing `*/`.
pub fn consume_block_comment<'a>(
    cursor: &mut SourceCursor<'a>,
) -> Result<Option<Comment<'a>>, CommentError> {
    if !cursor.starts_with("/*") {
        return Ok(None);
    }

    let start = cursor.mark();
    let kind = if cursor.starts_with("/**") {
        let ate = cursor.eat_str("/**");
        debug_assert!(ate);
        CommentKind::DocBlock
    } else if cursor.starts_with("/*!") {
        let ate = cursor.eat_str("/*!");
        debug_assert!(ate);
        CommentKind::InnerDocBlock
    } else {
        let ate = cursor.eat_str("/*");
        debug_assert!(ate);
        CommentKind::Block
    };

    loop {
        if cursor.is_eof() {
            return Err(CommentError::UnterminatedBlockComment {
                span: cursor.current_span_from(start),
            });
        }
        if cursor.starts_with("*/") {
            let ate = cursor.eat_str("*/");
            debug_assert!(ate);
            break;
        }
        let _ = cursor.bump();
    }

    let span = cursor.current_span_from(start);
    let text = cursor.slice_from(start);
    Ok(Some(Comment { kind, span, text }))
}

/// Consumes either a line or block comment at current offset.
///
/// Returns `Ok(None)` when current offset is not at a comment start.
///
/// # Errors
/// Returns [`CommentError`] for malformed comment forms such as unterminated
/// block comments.
pub fn consume_comment<'a>(
    cursor: &mut SourceCursor<'a>,
) -> Result<Option<Comment<'a>>, CommentError> {
    if cursor.starts_with("//") {
        return Ok(consume_line_comment(cursor));
    }
    if cursor.starts_with("/*") {
        return consume_block_comment(cursor);
    }
    Ok(None)
}

/// Consumes a doc comment (`///`, `/**`, `//!`, `/*!`) when present.
///
/// Returns `Ok(None)` without consuming input when current offset is not at a
/// doc comment start (including normal `//` and `/*` comments).
///
/// # Errors
/// Returns [`CommentError`] for malformed doc block comments.
pub fn consume_doc_comment<'a>(
    cursor: &mut SourceCursor<'a>,
) -> Result<Option<Comment<'a>>, CommentError> {
    if cursor.starts_with("///") || cursor.starts_with("//!") {
        return Ok(consume_line_comment(cursor));
    }
    if cursor.starts_with("/**") || cursor.starts_with("/*!") {
        return consume_block_comment(cursor);
    }
    Ok(None)
}

/// Consumes an outer doc comment (`///`, `/**`) when present.
///
/// Returns `Ok(None)` without consuming input when current offset is not at an
/// outer-doc comment start.
///
/// # Errors
/// Returns [`CommentError`] for malformed outer doc block comments.
pub fn consume_outer_doc_comment<'a>(
    cursor: &mut SourceCursor<'a>,
) -> Result<Option<Comment<'a>>, CommentError> {
    if cursor.starts_with("///") {
        return Ok(consume_line_comment(cursor));
    }
    if cursor.starts_with("/**") {
        return consume_block_comment(cursor);
    }
    Ok(None)
}

/// Collects all doc comments from the provided source text in source order.
///
/// This helper is parser-oriented: ordinary comments remain ignored, while
/// doc comments preserve original span/text.
///
/// # Errors
/// Returns [`CommentError`] for malformed block comment forms.
pub fn collect_doc_comments<'a>(
    source: &'a str,
) -> Result<Vec<Comment<'a>>, CommentError> {
    let mut cursor = SourceCursor::new(source);
    let mut docs = Vec::new();

    while !cursor.is_eof() {
        skip_whitespace(&mut cursor);

        if let Some(comment) = consume_doc_comment(&mut cursor)? {
            docs.push(comment);
            continue;
        }

        if consume_comment(&mut cursor)?.is_some() {
            continue;
        }

        let _ = cursor.bump();
    }

    Ok(docs)
}

/// Consumes all whitespace/comments trivia at current offset.
///
/// This repeatedly skips whitespace and comments until first non-trivia
/// character or EOF.
///
/// Doc comments are also skipped here; comment-kind distinctions remain
/// available through [`consume_comment`].
///
/// # Errors
/// Returns [`CommentError`] for malformed comments such as unterminated block
/// comments.
pub fn skip_trivia(cursor: &mut SourceCursor<'_>) -> Result<(), CommentError> {
    loop {
        let before = cursor.offset();
        skip_whitespace(cursor);
        let consumed_comment = consume_comment(cursor)?.is_some();
        if !consumed_comment && cursor.offset() == before {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_whitespace_consumes_unicode_whitespace() {
        let mut cursor = SourceCursor::new(" \t\u{2003}\nabc");
        skip_whitespace(&mut cursor);
        assert_eq!(cursor.peek(), Some('a'));
    }

    #[test]
    fn consume_normal_line_comment() {
        let mut cursor = SourceCursor::new("// hello");
        let comment = consume_line_comment(&mut cursor).expect("line comment");
        assert_eq!(comment.kind, CommentKind::Line);
        assert_eq!(comment.text, "// hello");
        assert_eq!(comment.span.start, 0);
        assert_eq!(comment.span.end, 8);
        assert!(cursor.is_eof());
    }

    #[test]
    fn consume_doc_line_comment() {
        let mut cursor = SourceCursor::new("/// docs");
        let comment = consume_line_comment(&mut cursor).expect("doc line");
        assert_eq!(comment.kind, CommentKind::DocLine);
        assert_eq!(comment.text, "/// docs");
    }

    #[test]
    fn consume_inner_doc_line_comment() {
        let mut cursor = SourceCursor::new("//! docs");
        let comment =
            consume_line_comment(&mut cursor).expect("inner doc line");
        assert_eq!(comment.kind, CommentKind::InnerDocLine);
        assert_eq!(comment.text, "//! docs");
    }

    #[test]
    fn consume_normal_block_comment() {
        let mut cursor = SourceCursor::new("/* hello */x");
        let comment = consume_block_comment(&mut cursor)
            .expect("block parse")
            .expect("block comment");
        assert_eq!(comment.kind, CommentKind::Block);
        assert_eq!(comment.text, "/* hello */");
        assert_eq!(comment.span.start, 0);
        assert_eq!(comment.span.end, comment.text.len());
        assert_eq!(cursor.peek(), Some('x'));
    }

    #[test]
    fn consume_doc_block_comment() {
        let mut cursor = SourceCursor::new("/** docs */");
        let comment = consume_block_comment(&mut cursor)
            .expect("block parse")
            .expect("doc block");
        assert_eq!(comment.kind, CommentKind::DocBlock);
        assert_eq!(comment.text, "/** docs */");
    }

    #[test]
    fn consume_inner_doc_block_comment() {
        let mut cursor = SourceCursor::new("/*! docs */");
        let comment = consume_block_comment(&mut cursor)
            .expect("block parse")
            .expect("inner doc block");
        assert_eq!(comment.kind, CommentKind::InnerDocBlock);
        assert_eq!(comment.text, "/*! docs */");
    }

    #[test]
    fn consume_block_comment_stops_at_first_closer() {
        let mut cursor = SourceCursor::new("/* a /* b */ tail */x");
        let comment = consume_block_comment(&mut cursor)
            .expect("block parse")
            .expect("block comment");
        assert_eq!(comment.kind, CommentKind::Block);
        assert_eq!(comment.text, "/* a /* b */");
        assert_eq!(cursor.remaining(), " tail */x");
    }

    #[test]
    fn unterminated_block_comment_reports_error() {
        let mut cursor = SourceCursor::new("/* unclosed");
        let err = consume_block_comment(&mut cursor).expect_err("expected err");
        match err {
            CommentError::UnterminatedBlockComment { span } => {
                assert_eq!(span.start, 0);
                assert_eq!(span.end, "/* unclosed".len());
            }
        }
    }

    #[test]
    fn consume_comment_returns_none_when_not_at_comment() {
        let mut cursor = SourceCursor::new("abc");
        let got = consume_comment(&mut cursor).expect("no error");
        assert!(got.is_none());
        assert_eq!(cursor.offset(), 0);
    }

    #[test]
    fn skip_trivia_skips_whitespace_and_comments() {
        let mut cursor = SourceCursor::new(" \t// x\n/* y */  abc");
        skip_trivia(&mut cursor).expect("skip trivia");
        assert_eq!(cursor.peek(), Some('a'));
        assert_eq!(cursor.remaining(), "abc");
    }

    #[test]
    fn line_comment_does_not_overconsume_newline() {
        let mut cursor = SourceCursor::new("// hi\nx");
        let comment = consume_line_comment(&mut cursor).expect("line comment");
        assert_eq!(comment.text, "// hi");
        assert_eq!(cursor.peek(), Some('\n'));
    }

    #[test]
    fn consume_outer_doc_line_comment_distinct_from_normal_comments() {
        let mut doc_cursor = SourceCursor::new("/// docs");
        let doc = consume_outer_doc_comment(&mut doc_cursor)
            .expect("no error")
            .expect("expected outer doc");
        assert_eq!(doc.kind, CommentKind::DocLine);
        assert_eq!(doc.text, "/// docs");

        let mut normal_cursor = SourceCursor::new("// normal");
        let normal =
            consume_outer_doc_comment(&mut normal_cursor).expect("no error");
        assert!(normal.is_none());
        assert_eq!(normal_cursor.offset(), 0);
    }

    #[test]
    fn consume_outer_doc_block_comment_distinct_from_normal_comments() {
        let mut doc_cursor = SourceCursor::new("/** docs */");
        let doc = consume_outer_doc_comment(&mut doc_cursor)
            .expect("no error")
            .expect("expected outer doc block");
        assert_eq!(doc.kind, CommentKind::DocBlock);
        assert_eq!(doc.text, "/** docs */");

        let mut normal_cursor = SourceCursor::new("/* normal */");
        let normal =
            consume_outer_doc_comment(&mut normal_cursor).expect("no error");
        assert!(normal.is_none());
        assert_eq!(normal_cursor.offset(), 0);
    }
}
