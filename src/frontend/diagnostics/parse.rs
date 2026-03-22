use crate::frontend::diagnostics::{Diagnostic, DiagnosticLabel, FileSpan};
use crate::frontend::lexer::{CommentError, LexerError, Span, StringLexError};
use crate::frontend::parser::ParseError;
use crate::frontend::source::FileId;

/// Converts a parser error into a file-aware frontend diagnostic.
#[must_use]
pub fn diagnostic_from_parse_error(
    file_id: FileId,
    error: &ParseError,
) -> Diagnostic {
    match error {
        ParseError::UnexpectedToken {
            expected,
            found,
            span,
        } => Diagnostic::error("unexpected token")
            .with_label(DiagnosticLabel::primary(
                FileSpan::new(file_id, *span),
                format!("expected {expected}, found {found:?}"),
            ))
            .with_note("while parsing source"),
        ParseError::UnexpectedEof { expected, span } => {
            Diagnostic::error("unexpected end of file")
                .with_label(DiagnosticLabel::primary(
                    FileSpan::new(file_id, *span),
                    format!("expected {expected} before end of file"),
                ))
                .with_note("input ended while parsing source")
        }
        ParseError::Lex(lex_error) => {
            // Treat lexer errors as recoverable warnings during the pipeline
            // so that scopes can proceed and diagnostics can be emitted
            // without failing entire compilation.
            let mut diagnostic = Diagnostic::warning("lexing failed");
            let span = span_from_lexer_error(lex_error);
            diagnostic = diagnostic.with_label(DiagnosticLabel::primary_span(
                FileSpan::new(file_id, span),
            ));
            diagnostic.with_note(format!("{lex_error}"))
        }
        other @ ParseError::UnsupportedItemStart { .. } => {
            let mut diagnostic =
                Diagnostic::error("parse failed").with_note(format!("{other}"));
            let span = span_from_parse_error(other);
            diagnostic = diagnostic.with_label(DiagnosticLabel::primary_span(
                FileSpan::new(file_id, span),
            ));
            diagnostic
        }
    }
}

/// Converts a file-aware parser error into a diagnostic.
#[must_use]
pub fn diagnostic_from_file_parse_error(
    error: &crate::frontend::FileParseError,
) -> Diagnostic {
    diagnostic_from_parse_error(error.file_id, &error.error)
}

fn span_from_parse_error(error: &ParseError) -> Span {
    match error {
        ParseError::UnexpectedToken { span, .. }
        | ParseError::UnexpectedEof { span, .. }
        | ParseError::UnsupportedItemStart { span, .. } => *span,
        ParseError::Lex(lex_error) => span_from_lexer_error(lex_error),
    }
}

fn span_from_lexer_error(error: &LexerError) -> Span {
    match error {
        LexerError::UnexpectedCharacter { span, .. } => *span,
        LexerError::Comment(comment_error) => {
            span_from_comment_error(comment_error)
        }
        LexerError::String(string_error) => {
            span_from_string_error(string_error)
        }
    }
}

fn span_from_comment_error(error: &CommentError) -> Span {
    match error {
        CommentError::UnterminatedBlockComment { span } => *span,
    }
}

fn span_from_string_error(error: &StringLexError) -> Span {
    match error {
        StringLexError::UnterminatedChar { span }
        | StringLexError::UnterminatedString { span }
        | StringLexError::UnterminatedEscape { span }
        | StringLexError::EmptyCharLiteral { span }
        | StringLexError::MultiCharLiteral { span } => *span,
    }
}
