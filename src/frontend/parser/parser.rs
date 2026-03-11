use crate::frontend::ast::{self, Item, Spanned, UseItem, UseTree};
use crate::frontend::lexer::{Lexer, Span, Token, TokenKind};

use super::error::ParseError;

/// Handwritten parser over a buffered lexer token stream.
///
/// `Parser` owns the token buffer and a cursor index into that buffer. It is a
/// source-oriented parser that runs before semantic analysis.
#[derive(Debug, Clone)]
pub struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
}

impl<'a> Parser<'a> {
    /// Creates a parser by lexing the full source into a token buffer.
    pub fn new(source: &'a str) -> Result<Self, ParseError> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token()?;
            let kind = token.kind;
            tokens.push(token);
            if kind == TokenKind::Eof {
                break;
            }
        }

        Ok(Self {
            source,
            tokens,
            cursor: 0,
        })
    }

    /// Parses a full source file until EOF.
    pub fn parse_file(&mut self) -> Result<ast::File, ParseError> {
        let mut items = Vec::new();
        while !self.is_eof() {
            items.push(self.parse_item()?);
        }
        Ok(ast::File { items })
    }

    fn parse_item(&mut self) -> Result<Spanned<Item>, ParseError> {
        let token = *self.peek();
        match token.kind {
            TokenKind::KwUse => self.parse_use_item(),
            TokenKind::KwFn => Err(ParseError::UnsupportedItemStart {
                item: "function declaration",
                span: token.span,
            }),
            TokenKind::KwStruct => Err(ParseError::UnsupportedItemStart {
                item: "struct declaration",
                span: token.span,
            }),
            TokenKind::KwEnum => Err(ParseError::UnsupportedItemStart {
                item: "enum declaration",
                span: token.span,
            }),
            TokenKind::KwImpl => Err(ParseError::UnsupportedItemStart {
                item: "impl declaration",
                span: token.span,
            }),
            TokenKind::KwProtocol => Err(ParseError::UnsupportedItemStart {
                item: "protocol declaration",
                span: token.span,
            }),
            TokenKind::KwExtern => Err(ParseError::UnsupportedItemStart {
                item: "extern block",
                span: token.span,
            }),
            TokenKind::Eof => Err(ParseError::UnexpectedEof {
                expected: "top-level item",
                span: token.span,
            }),
            _ => Err(ParseError::UnexpectedToken {
                expected: "top-level item",
                found: token.kind,
                span: token.span,
            }),
        }
    }

    fn parse_use_item(&mut self) -> Result<Spanned<Item>, ParseError> {
        let use_kw = self.expect(TokenKind::KwUse)?;
        let tree = self.parse_use_tree()?;
        let semi = self.expect(TokenKind::Semi)?;
        let span = Span::new(use_kw.span.start, semi.span.end);
        let use_item = Spanned::new(UseItem { tree }, span);
        Ok(Spanned::new(Item::Use(use_item), span))
    }

    fn parse_use_tree(&mut self) -> Result<Spanned<UseTree>, ParseError> {
        if self.at(TokenKind::LBrace) {
            let start = self.bump().span.start;
            let mut entries = Vec::new();

            if !self.at(TokenKind::RBrace) {
                loop {
                    entries.push(self.parse_use_tree()?);
                    if self.eat(TokenKind::Comma).is_some() {
                        if self.at(TokenKind::RBrace) {
                            break;
                        }
                        continue;
                    }
                    break;
                }
            }

            let end = self.expect(TokenKind::RBrace)?.span.end;
            let span = Span::new(start, end);
            return Ok(Spanned::new(UseTree::Group(entries), span));
        }

        if self.at(TokenKind::KwSelfValue) {
            let tok = self.bump();
            return Ok(Spanned::new(UseTree::SelfValue, tok.span));
        }

        let (head, head_span) = self.expect_identifier_text()?;
        if self.eat(TokenKind::ColonColon).is_some() {
            let tail = self.parse_use_tree()?;
            let span = Span::new(head_span.start, tail.span.end);
            return Ok(Spanned::new(
                UseTree::Path {
                    head,
                    tail: Box::new(tail),
                },
                span,
            ));
        }

        Ok(Spanned::new(UseTree::Name(head), head_span))
    }

    fn expect_identifier_text(&mut self) -> Result<(String, Span), ParseError> {
        let token = *self.peek();
        match token.kind {
            TokenKind::Ident => {
                let ident = self.slice(token.span).to_owned();
                let _ = self.bump();
                Ok((ident, token.span))
            }
            TokenKind::Eof => Err(ParseError::UnexpectedEof {
                expected: "identifier",
                span: token.span,
            }),
            _ => Err(ParseError::UnexpectedToken {
                expected: "identifier",
                found: token.kind,
                span: token.span,
            }),
        }
    }

    fn slice(&self, span: Span) -> &str {
        debug_assert!(span.start <= span.end);
        debug_assert!(span.end <= self.source.len());
        debug_assert!(self.source.is_char_boundary(span.start));
        debug_assert!(self.source.is_char_boundary(span.end));
        &self.source[span.start..span.end]
    }

    fn peek(&self) -> &Token {
        self.peek_nth(0)
    }

    fn peek_nth(&self, n: usize) -> &Token {
        let idx = self.cursor.saturating_add(n);
        if idx < self.tokens.len() {
            &self.tokens[idx]
        } else {
            self.tokens
                .last()
                .expect("parser token buffer must contain EOF")
        }
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek().kind == kind
    }

    fn eat(&mut self, kind: TokenKind) -> Option<Token> {
        if self.at(kind) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn bump(&mut self) -> Token {
        let token = *self.peek();
        if self.cursor + 1 < self.tokens.len() {
            self.cursor += 1;
        }
        token
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if self.at(kind) {
            return Ok(self.bump());
        }

        let token = *self.peek();
        if token.kind == TokenKind::Eof {
            return Err(ParseError::UnexpectedEof {
                expected: expected_for_token(kind),
                span: token.span,
            });
        }

        Err(ParseError::UnexpectedToken {
            expected: expected_for_token(kind),
            found: token.kind,
            span: token.span,
        })
    }

    fn is_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }
}

/// Parses a whole source file using default parser construction.
pub fn parse_source_file(source: &str) -> Result<ast::File, ParseError> {
    let mut parser = Parser::new(source)?;
    parser.parse_file()
}

fn expected_for_token(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::KwUse => "'use'",
        TokenKind::KwFn => "'fn'",
        TokenKind::KwStruct => "'struct'",
        TokenKind::KwEnum => "'enum'",
        TokenKind::KwImpl => "'impl'",
        TokenKind::KwProtocol => "'protocol'",
        TokenKind::KwExtern => "'extern'",
        TokenKind::Semi => "';'",
        TokenKind::LBrace => "'{'",
        TokenKind::RBrace => "'}'",
        TokenKind::LParen => "'('",
        TokenKind::RParen => "')'",
        TokenKind::ColonColon => "'::'",
        TokenKind::Ident => "identifier",
        TokenKind::Eof => "end-of-file",
        _ => "expected token",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexer::TokenKind;

    #[test]
    fn parser_new_lexes_tokens_successfully() {
        let parser = Parser::new("let x = 1;").expect("parser creation");
        assert!(!parser.tokens.is_empty());
        assert_eq!(parser.tokens[0].kind, TokenKind::KwLet);
    }

    #[test]
    fn expect_consumes_matching_token() {
        let mut parser = Parser::new("use foo;").expect("parser creation");
        let first = parser.expect(TokenKind::KwUse).expect("consume use");
        assert_eq!(first.kind, TokenKind::KwUse);
        assert_eq!(parser.peek().kind, TokenKind::Ident);
    }

    #[test]
    fn expect_reports_unexpected_token() {
        let mut parser = Parser::new("use foo;").expect("parser creation");
        let err = parser.expect(TokenKind::KwFn).expect_err("expected error");
        match err {
            ParseError::UnexpectedToken { found, span, .. } => {
                assert_eq!(found, TokenKind::KwUse);
                assert_eq!(span.start, 0);
            }
            _ => panic!("unexpected parse error shape"),
        }
    }

    #[test]
    fn expect_reports_unexpected_eof() {
        let mut parser = Parser::new("").expect("parser creation");
        let err = parser.expect(TokenKind::KwUse).expect_err("expected eof");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }

    #[test]
    fn parse_empty_file() {
        let mut parser = Parser::new("   \n\t ").expect("parser creation");
        let file = parser.parse_file().expect("parse file");
        assert!(file.items.is_empty());
    }

    #[test]
    fn parse_file_dispatches_top_level_fn_start() {
        let mut parser = Parser::new("fn demo() {}").expect("parser creation");
        let err = parser.parse_file().expect_err("expected unsupported");
        assert!(matches!(
            err,
            ParseError::UnsupportedItemStart {
                item: "function declaration",
                ..
            }
        ));
    }

    #[test]
    fn parse_file_dispatches_top_level_struct_start() {
        let mut parser =
            Parser::new("struct Demo {}").expect("parser creation");
        let err = parser.parse_file().expect_err("expected unsupported");
        assert!(matches!(
            err,
            ParseError::UnsupportedItemStart {
                item: "struct declaration",
                ..
            }
        ));
    }

    #[test]
    fn lexer_error_propagates_through_parser_new_or_parse() {
        let err = Parser::new("\"abc").expect_err("expected lexer error");
        assert!(matches!(err, ParseError::Lex(_)));
    }

    #[test]
    fn parse_simple_use_item_happy_path() {
        let mut parser = Parser::new("use core::fmt;").expect("parser");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 1);
        match &file.items[0].node {
            Item::Use(use_item) => match &use_item.node.tree.node {
                UseTree::Path { head, .. } => assert_eq!(head, "core"),
                _ => panic!("expected path use tree"),
            },
            _ => panic!("expected use item"),
        }
    }
}
