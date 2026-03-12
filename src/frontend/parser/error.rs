use crate::frontend::lexer::{LexerError, Span, TokenKind};
use std::fmt::{Display, Formatter};

/// Structured parser failure surface.
///
/// Parser errors wrap lexer failures and token expectation failures. The
/// `UnsupportedItemStart` variant remains for explicitly gated grammar entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Lex(LexerError),
    UnexpectedToken {
        expected: &'static str,
        found: TokenKind,
        span: Span,
    },
    UnexpectedEof {
        expected: &'static str,
        span: Span,
    },
    UnsupportedItemStart {
        item: &'static str,
        span: Span,
    },
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lex(source) => write!(f, "{source}"),
            Self::UnexpectedToken {
                expected,
                found,
                span,
            } => write!(
                f,
                "unexpected token {:?}; expected {} at byte range {}..{}",
                found, expected, span.start, span.end
            ),
            Self::UnexpectedEof { expected, span } => write!(
                f,
                "unexpected eof; expected {} at byte range {}..{}",
                expected, span.start, span.end
            ),
            Self::UnsupportedItemStart { item, span } => write!(
                f,
                "top-level '{}' parsing is not implemented yet at byte range {}..{}",
                item, span.start, span.end
            ),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Lex(source) => Some(source),
            Self::UnexpectedToken { .. }
            | Self::UnexpectedEof { .. }
            | Self::UnsupportedItemStart { .. } => None,
        }
    }
}

impl From<LexerError> for ParseError {
    fn from(value: LexerError) -> Self {
        Self::Lex(value)
    }
}
