use crate::frontend::ast::{
    self, AccessorRequirement, ArrayElement, ArrayPatternRest,
    AssociatedTypeDecl, Attribute, AttributeArgs, BindingKind, CallArg,
    DocComment, DocCommentKind, EnumCase, EnumCaseParam, EnumDecl, EnumMember,
    Expr, ExternBlock, ExternFunctionDecl, ExternMember, FunctionDecl,
    GenericParam, ImplDecl, ImplMember, InitDecl, InitKind, Item, LetStmt,
    MacroBlock, MacroClause, MacroClauseKind, MacroDecl, MacroExprArgs,
    MacroInputKind, MacroParam, MatchArm, MatchArmBody, Modifier, ParamDecl,
    ParamLabel, Pattern, ProtocolDecl, ProtocolFunctionMember,
    ProtocolInitMember, ProtocolMember, ProtocolPropertyRequirement,
    ReceiverKind, ScopeDecl, Spanned, StringLiteral, StringPart, StructDecl,
    StructField, StructLiteralField, StructMember, StructPatternField, Type,
    TypeExpr, UseItem, UsePath, UseTree, VarStmt, Visibility, WhileStmt,
};
use crate::frontend::lexer::{
    CommentKind, Lexer, LexerError, Span, Token, TokenKind,
    collect_doc_comments,
};

use super::error::ParseError;

type ReceiverAndParams =
    (Option<Spanned<ReceiverKind>>, Vec<Spanned<ParamDecl>>);
type BlockContentsParse =
    (Vec<Spanned<ast::Stmt>>, Option<Box<Spanned<Expr>>>, usize);

/// Handwritten parser over a buffered lexer token stream.
///
/// `Parser` owns the token buffer and a cursor index into that buffer. It is a
/// source-oriented parser that runs before semantic analysis.
#[derive(Debug, Clone)]
pub(crate) struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    doc_comments: Vec<Spanned<DocComment>>,
    doc_cursor: usize,
    last_token_end: usize,
    pub(crate) diagnostics: crate::frontend::DiagnosticsBag,
    recovery_enabled: bool,
    recovery_file_id: Option<crate::frontend::source::FileId>,
}

impl<'a> Parser<'a> {
    /// Creates a parser by lexing the full source into a token buffer.
    fn new(source: &'a str) -> Result<Self, ParseError> {
        let doc_comments = collect_doc_comments(source)
            .map_err(LexerError::from)?
            .into_iter()
            .map(|comment| {
                let kind = match comment.kind {
                    CommentKind::DocLine => DocCommentKind::OuterLine,
                    CommentKind::DocBlock => DocCommentKind::OuterBlock,
                    CommentKind::InnerDocLine => DocCommentKind::InnerLine,
                    CommentKind::InnerDocBlock => DocCommentKind::InnerBlock,
                    _ => unreachable!("collect_doc_comments only returns docs"),
                };
                Spanned::new(
                    DocComment {
                        kind,
                        span: comment.span,
                        text: comment.text.to_owned(),
                    },
                    comment.span,
                )
            })
            .collect();

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
            doc_comments,
            doc_cursor: 0,
            last_token_end: 0,
            diagnostics: crate::frontend::DiagnosticsBag::new(),
            recovery_enabled: false,
            recovery_file_id: None,
        })
    }

    /// Parses a full source file until EOF.
    fn parse_file(&mut self) -> Result<ast::File, ParseError> {
        let mut items = Vec::new();
        while !self.is_eof() {
            items.push(self.parse_item()?);
        }
        Ok(ast::File { items })
    }

    /// Parses a full source file while collecting diagnostics and recovering at
    /// conservative top-level boundaries.
    fn parse_file_with_recovery(&mut self) -> ast::File {
        let enabled_here = !self.recovery_enabled;
        if enabled_here {
            self.enable_recovery_with_file_id(
                crate::frontend::source::FileId::new(0),
            );
        }
        let mut items = Vec::new();

        while !self.is_eof() {
            let checkpoint = self.cursor;
            if let Some(item) = self.parse_item_recovering() {
                items.push(item);
            }

            if self.is_eof() {
                break;
            }
            if self.cursor == checkpoint {
                let _ = self.bump();
            }
        }

        if enabled_here {
            self.recovery_enabled = false;
            self.recovery_file_id = None;
        }
        ast::File { items }
    }

    fn current_recovery_file_id(&self) -> crate::frontend::source::FileId {
        self.recovery_file_id
            .unwrap_or(crate::frontend::source::FileId::new(0))
    }

    pub(crate) fn enable_recovery_with_file_id(
        &mut self,
        file_id: crate::frontend::source::FileId,
    ) {
        self.recovery_enabled = true;
        self.recovery_file_id = Some(file_id);
        self.diagnostics = crate::frontend::DiagnosticsBag::new();
    }

    fn parse_item(&mut self) -> Result<Spanned<Item>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let visibility = self.parse_optional_visibility()?;
        let modifiers = self.parse_modifiers();
        let token = *self.peek();
        match token.kind {
            TokenKind::KwUse => {
                if !docs.is_empty()
                    || !attributes.is_empty()
                    || !modifiers.is_empty()
                {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'use' declaration prefix",
                        found: token.kind,
                        span: token.span,
                    });
                }
                self.parse_use_item_with_visibility(start, visibility)
            }
            TokenKind::KwScope => {
                if !docs.is_empty()
                    || !attributes.is_empty()
                    || !modifiers.is_empty()
                {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'scope' declaration prefix",
                        found: token.kind,
                        span: token.span,
                    });
                }
                self.parse_scope_item_with_visibility(start, visibility)
            }
            TokenKind::KwFn => {
                let function = self.parse_function_decl_with_prefix(
                    start, docs, attributes, visibility, modifiers,
                )?;
                let span = function.span;
                Ok(Spanned::new(Item::Function(function), span))
            }
            TokenKind::KwStruct => {
                let decl = self.parse_struct_decl_with_prefix(
                    start, docs, attributes, visibility, modifiers,
                )?;
                let span = decl.span;
                Ok(Spanned::new(Item::Struct(decl), span))
            }
            TokenKind::KwEnum => {
                let decl = self.parse_enum_decl_with_prefix(
                    start, docs, attributes, visibility, modifiers,
                )?;
                let span = decl.span;
                Ok(Spanned::new(Item::Enum(decl), span))
            }
            TokenKind::KwImpl => {
                if visibility.is_some() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'impl'",
                        found: token.kind,
                        span: token.span,
                    });
                }
                if modifiers
                    .iter()
                    .any(|modifier| !matches!(modifier, Modifier::Unsafe))
                {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'unsafe impl' or 'impl'",
                        found: token.kind,
                        span: token.span,
                    });
                }
                let decl = self.parse_impl_decl_with_prefix(
                    start, docs, attributes, modifiers,
                )?;
                let span = decl.span;
                Ok(Spanned::new(Item::Impl(decl), span))
            }
            TokenKind::KwProtocol => {
                let decl = self.parse_protocol_decl_with_prefix(
                    start, docs, attributes, visibility, modifiers,
                )?;
                let span = decl.span;
                Ok(Spanned::new(Item::Protocol(decl), span))
            }
            TokenKind::KwExtern => {
                if !modifiers.is_empty() || visibility.is_some() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'extern'",
                        found: token.kind,
                        span: token.span,
                    });
                }

                let extern_block = self
                    .parse_extern_block_with_prefix(start, docs, attributes)?;
                let span = extern_block.span;
                Ok(Spanned::new(Item::ExternBlock(extern_block), span))
            }
            TokenKind::KwMacro => {
                if !modifiers.is_empty() || visibility.is_some() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'macro'",
                        found: token.kind,
                        span: token.span,
                    });
                }
                let macro_decl =
                    self.parse_macro_decl_with_prefix(start, docs, attributes)?;
                let span = macro_decl.span;
                Ok(Spanned::new(Item::Macro(macro_decl), span))
            }
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

    fn parse_item_recovering(&mut self) -> Option<Spanned<Item>> {
        match self.parse_item() {
            Ok(item) => Some(item),
            Err(error) => {
                self.record_parse_error(&error);
                let checkpoint = self.cursor;
                self.synchronize_to_top_level_item();
                if self.cursor == checkpoint && !self.is_eof() {
                    let _ = self.bump();
                }
                None
            }
        }
    }

    fn parse_stmt_recovering(&mut self) -> Option<Spanned<ast::Stmt>> {
        match self.parse_stmt() {
            Ok(stmt) => Some(stmt),
            Err(error) => {
                self.record_parse_error(&error);
                let checkpoint = self.cursor;
                self.synchronize_to_statement_boundary();
                if self.cursor == checkpoint && !self.is_eof() {
                    let _ = self.bump();
                }
                None
            }
        }
    }

    fn record_parse_error(&mut self, error: &ParseError) {
        let diagnostic = crate::frontend::diagnostic_from_parse_error(
            self.current_recovery_file_id(),
            error,
        );
        self.diagnostics.push(diagnostic);
    }

    fn synchronize_to_top_level_item(&mut self) {
        loop {
            if self.is_eof() {
                return;
            }

            let kind = self.peek().kind;
            if kind == TokenKind::Semi {
                let _ = self.bump();
                return;
            }
            if Self::can_start_top_level_item(kind) {
                return;
            }
            if kind == TokenKind::RBrace {
                while self.at(TokenKind::RBrace) && !self.is_eof() {
                    let _ = self.bump();
                }
                continue;
            }

            let _ = self.bump();
        }
    }

    fn synchronize_to_statement_boundary(&mut self) {
        loop {
            if self.is_eof() {
                return;
            }

            let kind = self.peek().kind;
            if kind == TokenKind::Semi {
                let _ = self.bump();
                return;
            }
            if kind == TokenKind::RBrace {
                return;
            }
            if Self::can_start_stmt(kind)
                || Self::can_start_expr_statement(kind)
            {
                return;
            }

            let _ = self.bump();
        }
    }

    fn parse_use_item_with_visibility(
        &mut self,
        start: usize,
        visibility: Option<Visibility>,
    ) -> Result<Spanned<Item>, ParseError> {
        let use_kw = self.expect(TokenKind::KwUse)?;
        let tree = self.parse_use_tree()?;
        let semi = self.expect(TokenKind::Semi)?;
        let span_start = if visibility.is_some() {
            start
        } else {
            use_kw.span.start
        };
        let span = Span::new(span_start, semi.span.end);
        let use_item = Spanned::new(UseItem { visibility, tree }, span);
        Ok(Spanned::new(Item::Use(use_item), span))
    }

    fn parse_scope_item_with_visibility(
        &mut self,
        start: usize,
        visibility: Option<Visibility>,
    ) -> Result<Spanned<Item>, ParseError> {
        let scope_kw = self.expect(TokenKind::KwScope)?;
        let (name, _) = self.expect_identifier_text()?;
        let semi = self.expect(TokenKind::Semi)?;
        let span_start = if visibility.is_some() {
            start
        } else {
            scope_kw.span.start
        };
        let span = Span::new(span_start, semi.span.end);
        let scope_decl = Spanned::new(ScopeDecl { visibility, name }, span);
        Ok(Spanned::new(Item::Scope(scope_decl), span))
    }

    fn parse_use_tree(&mut self) -> Result<Spanned<UseTree>, ParseError> {
        self.parse_use_tree_with_self(false)
    }

    fn parse_use_tree_with_self(
        &mut self,
        allow_self_import: bool,
    ) -> Result<Spanned<UseTree>, ParseError> {
        if allow_self_import && self.at(TokenKind::KwSelfValue) {
            let self_tok = self.bump();
            if self.eat(TokenKind::KwAs).is_some() {
                let (alias, alias_span) = self.expect_identifier_text()?;
                return Ok(Spanned::new(
                    UseTree::SelfAlias { alias },
                    Span::new(self_tok.span.start, alias_span.end),
                ));
            }
            return Ok(Spanned::new(UseTree::SelfImport, self_tok.span));
        }

        let (path, path_span) = self.parse_use_path()?;

        if self.eat(TokenKind::KwAs).is_some() {
            let (alias, alias_span) = self.expect_identifier_text()?;
            let span = Span::new(path_span.start, alias_span.end);
            return Ok(Spanned::new(UseTree::Alias { path, alias }, span));
        }

        if self.eat(TokenKind::ColonColon).is_some() {
            if self.eat(TokenKind::Star).is_some() {
                let end = self.last_token_end;
                return Ok(Spanned::new(
                    UseTree::Glob { path },
                    Span::new(path_span.start, end),
                ));
            }
            if self.at(TokenKind::LBrace) {
                return self.parse_use_group_with_prefix(path, path_span.start);
            }
            return Err(ParseError::UnexpectedToken {
                expected: "'*' or '{'",
                found: self.peek().kind,
                span: self.peek().span,
            });
        }

        Ok(Spanned::new(UseTree::Path { path }, path_span))
    }

    fn parse_use_group_with_prefix(
        &mut self,
        path: UsePath,
        span_start: usize,
    ) -> Result<Spanned<UseTree>, ParseError> {
        self.expect(TokenKind::LBrace)?;
        let items = self.parse_use_group_items()?;
        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            UseTree::Group {
                path: Some(path),
                items,
            },
            Span::new(span_start, rbrace.span.end),
        ))
    }

    fn parse_use_group_items(
        &mut self,
    ) -> Result<Vec<Spanned<UseTree>>, ParseError> {
        if self.at(TokenKind::RBrace) {
            return Err(ParseError::UnexpectedToken {
                expected: "use group item",
                found: self.peek().kind,
                span: self.peek().span,
            });
        }

        let mut items = Vec::new();
        loop {
            items.push(self.parse_use_tree_with_self(true)?);
            if self.eat(TokenKind::Comma).is_some() {
                if self.at(TokenKind::RBrace) {
                    break;
                }
                continue;
            }
            break;
        }

        Ok(items)
    }

    fn parse_use_path(&mut self) -> Result<(UsePath, Span), ParseError> {
        let start = self.peek().span.start;
        let mut segments = Vec::new();
        segments.push(self.parse_use_path_segment(true)?);

        while self.at(TokenKind::ColonColon)
            && Self::can_start_use_path_segment(self.peek_nth(1).kind)
        {
            let _ = self.bump();
            segments.push(self.parse_use_path_segment(false)?);
        }

        let end = self.last_token_end;
        Ok((UsePath { segments }, Span::new(start, end)))
    }

    fn can_start_use_path_segment(kind: TokenKind) -> bool {
        matches!(kind, TokenKind::Ident | TokenKind::KwScope)
    }

    fn parse_use_path_segment(
        &mut self,
        _is_root: bool,
    ) -> Result<String, ParseError> {
        match self.peek().kind {
            TokenKind::Ident => {
                self.expect_identifier_text().map(|(text, _)| text)
            }
            TokenKind::KwScope => {
                let tok = self.bump();
                Ok(self.source[tok.span.start..tok.span.end].to_owned())
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "path segment",
                found: self.peek().kind,
                span: self.peek().span,
            }),
        }
    }

    fn parse_optional_visibility(
        &mut self,
    ) -> Result<Option<Visibility>, ParseError> {
        if !self.at(TokenKind::KwPub) {
            return Ok(None);
        }
        let pub_tok = self.bump();

        if self.eat(TokenKind::LParen).is_none() {
            return Ok(Some(Visibility::Public));
        }

        let vis = if self.at(TokenKind::Ident) {
            let (segment, _) = self.expect_identifier_text()?;
            match segment.as_str() {
                "super" => Visibility::PublicSuper,
                "project" => Visibility::PublicProject,
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'super' or 'project'",
                        found: TokenKind::Ident,
                        span: Span::new(pub_tok.span.end, self.last_token_end),
                    });
                }
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                expected: "'super' or 'project'",
                found: self.peek().kind,
                span: self.peek().span,
            });
        };

        self.expect(TokenKind::RParen)?;
        Ok(Some(vis))
    }

    fn parse_attributes(
        &mut self,
    ) -> Result<Vec<Spanned<Attribute>>, ParseError> {
        let mut attributes = Vec::new();
        while self.at(TokenKind::At) {
            attributes.push(self.parse_attribute()?);
        }
        Ok(attributes)
    }

    fn parse_outer_doc_comments(&mut self) -> Vec<Spanned<DocComment>> {
        let mut docs = Vec::new();
        let start = self.last_token_end;
        let end = self.peek().span.start;

        while self.doc_cursor < self.doc_comments.len() {
            let comment = self.doc_comments[self.doc_cursor].clone();
            if comment.span.end <= start {
                self.doc_cursor += 1;
                continue;
            }
            if comment.span.start < start {
                self.doc_cursor += 1;
                continue;
            }
            if comment.span.end > end {
                break;
            }

            if matches!(
                comment.node.kind,
                DocCommentKind::OuterLine | DocCommentKind::OuterBlock
            ) {
                docs.push(comment);
            }
            self.doc_cursor += 1;
        }

        docs
    }

    fn parse_attribute(&mut self) -> Result<Spanned<Attribute>, ParseError> {
        let at = self.expect(TokenKind::At)?;
        let (name, name_span) = self.expect_identifier_text()?;

        let (args, span_end) = if self.at(TokenKind::LParen) {
            let (raw, end) = self
                .capture_delimited_raw(TokenKind::LParen, TokenKind::RParen)?;
            (AttributeArgs::Paren { raw }, end)
        } else if self.at(TokenKind::LBrace) {
            let (raw, end) = self
                .capture_delimited_raw(TokenKind::LBrace, TokenKind::RBrace)?;
            (AttributeArgs::Braced { raw }, end)
        } else {
            (AttributeArgs::None, name_span.end)
        };

        Ok(Spanned::new(
            Attribute { name, args },
            Span::new(at.span.start, span_end),
        ))
    }

    fn capture_delimited_raw(
        &mut self,
        open: TokenKind,
        close: TokenKind,
    ) -> Result<(String, usize), ParseError> {
        let open_tok = self.expect(open)?;
        let raw_start = open_tok.span.end;
        let mut depth = 1usize;

        loop {
            let token = *self.peek();
            if token.kind == TokenKind::Eof {
                return Err(ParseError::UnexpectedEof {
                    expected: expected_for_token(close),
                    span: token.span,
                });
            }

            let token = self.bump();
            if token.kind == open {
                depth += 1;
            } else if token.kind == close {
                depth -= 1;
                if depth == 0 {
                    let raw =
                        self.source[raw_start..token.span.start].to_owned();
                    return Ok((raw, token.span.end));
                }
            }
        }
    }

    fn capture_delimited_tokens(
        &mut self,
        open: TokenKind,
        close: TokenKind,
    ) -> Result<(Vec<Token>, usize), ParseError> {
        let _open_tok = self.expect(open)?;
        let mut depth = 1usize;
        let mut captured = Vec::new();

        loop {
            let token = *self.peek();
            if token.kind == TokenKind::Eof {
                return Err(ParseError::UnexpectedEof {
                    expected: expected_for_token(close),
                    span: token.span,
                });
            }

            let token = self.bump();
            if token.kind == open {
                depth += 1;
                captured.push(token);
            } else if token.kind == close {
                depth -= 1;
                if depth == 0 {
                    return Ok((captured, token.span.end));
                }
                captured.push(token);
            } else {
                captured.push(token);
            }
        }
    }

    fn parse_macro_block_tokens(
        &mut self,
    ) -> Result<(MacroBlock, usize), ParseError> {
        let open = self.peek().span;
        let (tokens, end) = self
            .capture_delimited_tokens(TokenKind::LBrace, TokenKind::RBrace)?;
        let close_start = end.saturating_sub(1);
        let span = Span::new(open.end, close_start);
        Ok((MacroBlock { tokens, span }, end))
    }

    fn parse_modifiers(&mut self) -> Vec<Modifier> {
        let mut modifiers = Vec::new();
        loop {
            let modifier = match self.peek().kind {
                TokenKind::KwAsync => Some(Modifier::Async),
                TokenKind::KwUnsafe => Some(Modifier::Unsafe),
                _ => None,
            };

            if let Some(modifier) = modifier {
                let _ = self.bump();
                modifiers.push(modifier);
            } else {
                break;
            }
        }
        modifiers
    }

    fn parse_optional_generic_params(
        &mut self,
    ) -> Result<Vec<Spanned<GenericParam>>, ParseError> {
        if !self.at(TokenKind::Lt) {
            return Ok(Vec::new());
        }

        self.expect(TokenKind::Lt)?;
        let mut params = Vec::new();
        if !self.at(TokenKind::Gt) {
            loop {
                let (name, span) = self.expect_identifier_text()?;
                params.push(Spanned::new(GenericParam { name }, span));
                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::Gt) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::Gt)?;
        Ok(params)
    }

    /// Parses a `struct` declaration and its member container.
    #[cfg(test)]
    pub(crate) fn parse_struct_decl(
        &mut self,
    ) -> Result<Spanned<StructDecl>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let visibility = self.parse_optional_visibility()?;
        let modifiers = self.parse_modifiers();
        self.parse_struct_decl_with_prefix(
            start, docs, attributes, visibility, modifiers,
        )
    }

    fn parse_struct_decl_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
        visibility: Option<Visibility>,
        modifiers: Vec<Modifier>,
    ) -> Result<Spanned<StructDecl>, ParseError> {
        self.expect(TokenKind::KwStruct)?;
        let (name, _) = self.expect_identifier_text()?;
        let generic_params = self.parse_optional_generic_params()?;
        self.expect(TokenKind::LBrace)?;
        let mut members = Vec::new();

        while !self.at(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::UnexpectedEof {
                    expected: "'}'",
                    span: self.peek().span,
                });
            }
            members.push(self.parse_struct_member()?);
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            StructDecl {
                docs,
                attributes,
                visibility,
                modifiers,
                name,
                generic_params,
                members,
            },
            Span::new(start, rbrace.span.end),
        ))
    }

    fn parse_struct_member(
        &mut self,
    ) -> Result<Spanned<StructMember>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let visibility = self.parse_optional_visibility()?;
        let modifiers = self.parse_modifiers();

        let member = match self.peek().kind {
            TokenKind::KwInit => {
                if visibility.is_some() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "struct initializer; visibility is not allowed",
                        found: TokenKind::KwInit,
                        span: self.peek().span,
                    });
                }
                let init = self.parse_init_decl_with_prefix(
                    start, docs, attributes, modifiers,
                )?;
                let span = init.span;
                Spanned::new(StructMember::Init(init), span)
            }
            TokenKind::KwFn => {
                let function = self.parse_function_decl_with_prefix(
                    start, docs, attributes, visibility, modifiers,
                )?;
                let span = function.span;
                Spanned::new(StructMember::Function(function), span)
            }
            TokenKind::Ident => {
                if visibility.is_some() || !modifiers.is_empty() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "struct field; modifiers are not allowed on fields",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                let field = self.parse_struct_field(start, docs, attributes)?;
                let span = field.span;
                if self.eat(TokenKind::Comma).is_none()
                    && (self.at(TokenKind::Ident) || self.at(TokenKind::At))
                {
                    return Err(ParseError::UnexpectedToken {
                        expected: "','",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                Spanned::new(StructMember::Field(field), span)
            }
            _ => {
                if !attributes.is_empty() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "struct field or declaration member expected after attributes",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                return Err(ParseError::UnexpectedToken {
                    expected: "struct member",
                    found: self.peek().kind,
                    span: self.peek().span,
                });
            }
        };

        Ok(member)
    }

    fn parse_struct_field(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
    ) -> Result<Spanned<StructField>, ParseError> {
        let (name, name_span) = self.expect_identifier_text()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let span_start = if attributes.is_empty() && docs.is_empty() {
            name_span.start
        } else {
            start
        };
        let span = Span::new(span_start, ty.span.end);
        Ok(Spanned::new(
            StructField {
                docs,
                attributes,
                name,
                ty,
            },
            span,
        ))
    }

    /// Parses an `enum` declaration and its member container.
    #[cfg(test)]
    pub(crate) fn parse_enum_decl(
        &mut self,
    ) -> Result<Spanned<EnumDecl>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let visibility = self.parse_optional_visibility()?;
        let modifiers = self.parse_modifiers();
        self.parse_enum_decl_with_prefix(
            start, docs, attributes, visibility, modifiers,
        )
    }

    fn parse_enum_decl_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
        visibility: Option<Visibility>,
        modifiers: Vec<Modifier>,
    ) -> Result<Spanned<EnumDecl>, ParseError> {
        self.expect(TokenKind::KwEnum)?;
        let (name, _) = self.expect_identifier_text()?;
        let generic_params = self.parse_optional_generic_params()?;
        self.expect(TokenKind::LBrace)?;
        let mut members = Vec::new();

        while !self.at(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::UnexpectedEof {
                    expected: "'}'",
                    span: self.peek().span,
                });
            }
            members.push(self.parse_enum_member()?);
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            EnumDecl {
                docs,
                attributes,
                visibility,
                modifiers,
                name,
                generic_params,
                members,
            },
            Span::new(start, rbrace.span.end),
        ))
    }

    fn parse_enum_member(&mut self) -> Result<Spanned<EnumMember>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let visibility = self.parse_optional_visibility()?;
        let modifiers = self.parse_modifiers();

        let member = match self.peek().kind {
            TokenKind::KwInit => {
                if visibility.is_some() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "enum initializer; visibility is not allowed",
                        found: TokenKind::KwInit,
                        span: self.peek().span,
                    });
                }
                let init = self.parse_init_decl_with_prefix(
                    start, docs, attributes, modifiers,
                )?;
                let span = init.span;
                Spanned::new(EnumMember::Init(init), span)
            }
            TokenKind::KwFn => {
                let function = self.parse_function_decl_with_prefix(
                    start, docs, attributes, visibility, modifiers,
                )?;
                let span = function.span;
                Spanned::new(EnumMember::Function(function), span)
            }
            TokenKind::Ident => {
                if visibility.is_some() || !modifiers.is_empty() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "enum case; modifiers are not allowed on cases",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                let case = self.parse_enum_case(start, docs, attributes)?;
                let span = case.span;
                if self.eat(TokenKind::Comma).is_none()
                    && (self.at(TokenKind::Ident) || self.at(TokenKind::At))
                {
                    return Err(ParseError::UnexpectedToken {
                        expected: "',' or enum member boundary",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                Spanned::new(EnumMember::Case(case), span)
            }
            _ => {
                if !attributes.is_empty() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "enum case or declaration member expected after attributes",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                return Err(ParseError::UnexpectedToken {
                    expected: "enum member",
                    found: self.peek().kind,
                    span: self.peek().span,
                });
            }
        };

        Ok(member)
    }

    fn parse_enum_case(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
    ) -> Result<Spanned<EnumCase>, ParseError> {
        let (name, name_span) = self.expect_identifier_text()?;
        let mut payload = Vec::new();
        let mut end = name_span.end;

        if self.eat(TokenKind::LParen).is_some() {
            if !self.at(TokenKind::RParen) {
                loop {
                    if self.at(TokenKind::Ident)
                        && self.peek_nth(1).kind == TokenKind::Colon
                    {
                        let (field_name, _) = self.expect_identifier_text()?;
                        self.expect(TokenKind::Colon)?;
                        let ty = self.parse_type()?;
                        let param_end = ty.span.end;
                        payload.push(Spanned::new(
                            EnumCaseParam::Named {
                                name: field_name,
                                ty,
                            },
                            Span::new(name_span.start, param_end),
                        ));
                    } else {
                        let ty = self.parse_type()?;
                        payload.push(Spanned::new(
                            EnumCaseParam::Unnamed(ty.clone()),
                            ty.span,
                        ));
                    }

                    if self.eat(TokenKind::Comma).is_some() {
                        if self.at(TokenKind::RParen) {
                            break;
                        }
                        continue;
                    }
                    break;
                }
            }
            let rparen = self.expect(TokenKind::RParen)?;
            end = rparen.span.end;
        }

        let span_start = if attributes.is_empty() && docs.is_empty() {
            name_span.start
        } else {
            start
        };
        Ok(Spanned::new(
            EnumCase {
                docs,
                attributes,
                name,
                payload,
            },
            Span::new(span_start, end),
        ))
    }

    /// Parses an `impl` declaration container.
    #[cfg(test)]
    pub(crate) fn parse_impl_decl(
        &mut self,
    ) -> Result<Spanned<ImplDecl>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let modifiers = self.parse_modifiers();
        if modifiers
            .iter()
            .any(|modifier| !matches!(modifier, Modifier::Unsafe))
        {
            return Err(ParseError::UnexpectedToken {
                expected: "'unsafe impl' or 'impl'",
                found: self.peek().kind,
                span: self.peek().span,
            });
        }
        self.parse_impl_decl_with_prefix(start, docs, attributes, modifiers)
    }

    fn parse_impl_decl_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
        modifiers: Vec<Modifier>,
    ) -> Result<Spanned<ImplDecl>, ParseError> {
        self.expect(TokenKind::KwImpl)?;
        let first_type = self.parse_type()?;
        let (target, conformance) = if self.eat(TokenKind::KwFor).is_some() {
            let implementing_type = self.parse_type()?;
            (implementing_type, Some(first_type))
        } else {
            (first_type, None)
        };
        self.expect(TokenKind::LBrace)?;
        let mut members = Vec::new();

        while !self.at(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::UnexpectedEof {
                    expected: "'}'",
                    span: self.peek().span,
                });
            }
            members.push(self.parse_impl_member()?);
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            ImplDecl {
                docs,
                attributes,
                modifiers,
                target,
                conformance,
                members,
            },
            Span::new(start, rbrace.span.end),
        ))
    }

    fn parse_impl_member(&mut self) -> Result<Spanned<ImplMember>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let visibility = self.parse_optional_visibility()?;
        let modifiers = self.parse_modifiers();

        match self.peek().kind {
            TokenKind::KwInit => {
                if visibility.is_some() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "impl initializer; visibility is not allowed",
                        found: TokenKind::KwInit,
                        span: self.peek().span,
                    });
                }
                let init = self.parse_init_decl_with_prefix(
                    start, docs, attributes, modifiers,
                )?;
                let span = init.span;
                Ok(Spanned::new(ImplMember::Init(init), span))
            }
            TokenKind::KwFn => {
                let function = self.parse_function_decl_with_prefix(
                    start, docs, attributes, visibility, modifiers,
                )?;
                let span = function.span;
                Ok(Spanned::new(ImplMember::Function(function), span))
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "impl member",
                found: self.peek().kind,
                span: self.peek().span,
            }),
        }
    }

    /// Parses a `protocol` declaration and member container.
    #[cfg(test)]
    pub(crate) fn parse_protocol_decl(
        &mut self,
    ) -> Result<Spanned<ProtocolDecl>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let visibility = self.parse_optional_visibility()?;
        let modifiers = self.parse_modifiers();
        self.parse_protocol_decl_with_prefix(
            start, docs, attributes, visibility, modifiers,
        )
    }

    fn parse_protocol_decl_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
        visibility: Option<Visibility>,
        modifiers: Vec<Modifier>,
    ) -> Result<Spanned<ProtocolDecl>, ParseError> {
        self.expect(TokenKind::KwProtocol)?;
        let (name, _) = self.expect_identifier_text()?;
        let generic_params = self.parse_optional_generic_params()?;
        let inheritance = if self.eat(TokenKind::Colon).is_some() {
            self.parse_type_list()?
        } else {
            Vec::new()
        };
        self.expect(TokenKind::LBrace)?;
        let mut members = Vec::new();

        while !self.at(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::UnexpectedEof {
                    expected: "'}'",
                    span: self.peek().span,
                });
            }
            members.push(self.parse_protocol_member()?);
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            ProtocolDecl {
                docs,
                attributes,
                visibility,
                modifiers,
                name,
                generic_params,
                inheritance,
                members,
            },
            Span::new(start, rbrace.span.end),
        ))
    }

    fn parse_protocol_member(
        &mut self,
    ) -> Result<Spanned<ProtocolMember>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let modifiers = self.parse_modifiers();

        match self.peek().kind {
            TokenKind::KwFn => {
                let function = self
                    .parse_protocol_function_member_with_prefix(
                        start, docs, attributes, modifiers,
                    )?;
                let span = function.span;
                Ok(Spanned::new(ProtocolMember::Function(function), span))
            }
            TokenKind::KwInit => {
                let init = self.parse_protocol_init_member_with_prefix(
                    start, docs, attributes, modifiers,
                )?;
                let span = init.span;
                Ok(Spanned::new(ProtocolMember::Initializer(init), span))
            }
            TokenKind::KwType => {
                if !modifiers.is_empty() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'type'",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                let assoc = self.parse_associated_type_decl_with_prefix(
                    start, docs, attributes,
                )?;
                let span = assoc.span;
                Ok(Spanned::new(ProtocolMember::AssociatedType(assoc), span))
            }
            TokenKind::KwLet | TokenKind::KwVar => {
                let property = self
                    .parse_protocol_property_requirement_with_prefix(
                        start, docs, attributes, modifiers,
                    )?;
                let span = property.span;
                Ok(Spanned::new(ProtocolMember::Property(property), span))
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "protocol member",
                found: self.peek().kind,
                span: self.peek().span,
            }),
        }
    }

    fn parse_protocol_function_member_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
        modifiers: Vec<Modifier>,
    ) -> Result<Spanned<ProtocolFunctionMember>, ParseError> {
        self.expect(TokenKind::KwFn)?;
        let (name, _) = self.expect_identifier_text()?;
        let (receiver, params) = self.parse_receiver_or_param_list(true)?;
        let return_type = if self.eat(TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let where_clause = None;
        let (default_body, end) = if let Some(semi) = self.eat(TokenKind::Semi)
        {
            (None, semi.span.end)
        } else if self.at(TokenKind::LBrace) {
            let (block, block_end) = self.parse_block_with_end()?;
            (Some(block), block_end)
        } else {
            return Err(ParseError::UnexpectedToken {
                expected: "';' or '{'",
                found: self.peek().kind,
                span: self.peek().span,
            });
        };

        Ok(Spanned::new(
            ProtocolFunctionMember {
                docs,
                attributes,
                modifiers,
                name,
                generic_params: Vec::new(),
                receiver,
                params,
                return_type,
                where_clause,
                init_origin: None,
                default_body,
            },
            Span::new(start, end),
        ))
    }

    fn parse_protocol_init_member_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
        modifiers: Vec<Modifier>,
    ) -> Result<Spanned<ProtocolInitMember>, ParseError> {
        self.expect(TokenKind::KwInit)?;

        // Check for deprecated syntax - init? and init! are now syntax errors
        if self.at(TokenKind::Question) {
            return Err(ParseError::UnexpectedToken {
                expected: "init with return type annotation (-> Option<Self>)",
                found: TokenKind::Question,
                span: self.peek().span,
            });
        }
        if self.at(TokenKind::Bang) {
            return Err(ParseError::UnexpectedToken {
                expected: "init with return type annotation (-> Result<Self, E>)",
                found: TokenKind::Bang,
                span: self.peek().span,
            });
        }

        // Default to Plain; will be inferred from return type during desugaring
        let kind = InitKind::Plain;
        let (receiver, params) = self.parse_receiver_or_param_list(true)?;

        // Parse optional return type annotation (e.g., `-> Option<Self>`)
        let return_type = if self.eat(TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };

        let (default_body, end) = if let Some(semi) = self.eat(TokenKind::Semi)
        {
            (None, semi.span.end)
        } else if self.at(TokenKind::LBrace) {
            let (block, block_end) = self.parse_block_with_end()?;
            (Some(block), block_end)
        } else {
            return Err(ParseError::UnexpectedToken {
                expected: "';' or '{'",
                found: self.peek().kind,
                span: self.peek().span,
            });
        };

        Ok(Spanned::new(
            ProtocolInitMember {
                docs,
                attributes,
                modifiers,
                kind,
                receiver,
                params,
                return_type,
                default_body,
            },
            Span::new(start, end),
        ))
    }

    fn parse_associated_type_decl_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
    ) -> Result<Spanned<AssociatedTypeDecl>, ParseError> {
        let type_kw = self.expect(TokenKind::KwType)?;
        let (name, _) = self.expect_identifier_text()?;
        let bounds = if self.eat(TokenKind::Colon).is_some() {
            self.parse_type_list()?
        } else {
            Vec::new()
        };
        let semi = self.expect(TokenKind::Semi)?;
        let span_start = if docs.is_empty() && attributes.is_empty() {
            type_kw.span.start
        } else {
            start
        };
        Ok(Spanned::new(
            AssociatedTypeDecl {
                docs,
                attributes,
                name,
                bounds,
            },
            Span::new(span_start, semi.span.end),
        ))
    }

    fn parse_protocol_property_requirement_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
        modifiers: Vec<Modifier>,
    ) -> Result<Spanned<ProtocolPropertyRequirement>, ParseError> {
        let binding = if self.eat(TokenKind::KwLet).is_some() {
            BindingKind::Let
        } else {
            self.expect(TokenKind::KwVar)?;
            BindingKind::Var
        };
        let (name, _) = self.expect_identifier_text()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::LBrace)?;
        let mut accessors = Vec::new();

        while !self.at(TokenKind::RBrace) {
            let accessor = match self.peek().kind {
                TokenKind::KwGet => {
                    self.bump();
                    AccessorRequirement::Get
                }
                TokenKind::KwSet => {
                    self.bump();
                    AccessorRequirement::Set
                }
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "'get' or 'set'",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
            };
            accessors.push(accessor);
        }

        if accessors.is_empty() {
            return Err(ParseError::UnexpectedToken {
                expected: "'get' or 'set'",
                found: self.peek().kind,
                span: self.peek().span,
            });
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            ProtocolPropertyRequirement {
                docs,
                attributes,
                modifiers,
                binding,
                name,
                ty,
                accessors,
            },
            Span::new(start, rbrace.span.end),
        ))
    }

    fn parse_type_list(&mut self) -> Result<Vec<Spanned<Type>>, ParseError> {
        let mut types = Vec::new();
        loop {
            types.push(self.parse_type()?);
            if self.eat(TokenKind::Comma).is_some() {
                continue;
            }
            break;
        }
        Ok(types)
    }

    /// Parses a function declaration header and body block.
    #[cfg(test)]
    pub(crate) fn parse_function_decl(
        &mut self,
    ) -> Result<Spanned<FunctionDecl>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let visibility = self.parse_optional_visibility()?;
        let modifiers = self.parse_modifiers();
        self.parse_function_decl_with_prefix(
            start, docs, attributes, visibility, modifiers,
        )
    }

    fn parse_function_decl_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
        visibility: Option<Visibility>,
        modifiers: Vec<Modifier>,
    ) -> Result<Spanned<FunctionDecl>, ParseError> {
        let _fn_kw = self.expect(TokenKind::KwFn)?;
        let (name, _) = self.expect_identifier_text()?;
        let (receiver, params) = self.parse_receiver_or_param_list(true)?;
        let return_type = if self.eat(TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let where_clause = None;
        let (body, body_end) = self.parse_block_with_end()?;
        let span = Span::new(start, body_end);

        Ok(Spanned::new(
            FunctionDecl {
                docs,
                attributes,
                visibility,
                modifiers,
                name,
                generic_params: Vec::new(),
                receiver,
                params,
                return_type,
                where_clause,
                init_origin: None,
                body,
            },
            span,
        ))
    }

    /// Parses an initializer declaration signature (`init`, `init?`, `init!`)
    /// and body block.
    #[cfg(test)]
    pub(crate) fn parse_init_decl(
        &mut self,
    ) -> Result<Spanned<InitDecl>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        let modifiers = self.parse_modifiers();
        self.parse_init_decl_with_prefix(start, docs, attributes, modifiers)
    }

    fn parse_init_decl_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
        modifiers: Vec<Modifier>,
    ) -> Result<Spanned<InitDecl>, ParseError> {
        let _init_kw = self.expect(TokenKind::KwInit)?;

        // Check for deprecated syntax - init? and init! are now syntax errors
        if self.at(TokenKind::Question) {
            return Err(ParseError::UnexpectedToken {
                expected: "init with return type annotation (-> Option<Self>)",
                found: TokenKind::Question,
                span: self.peek().span,
            });
        }
        if self.at(TokenKind::Bang) {
            return Err(ParseError::UnexpectedToken {
                expected: "init with return type annotation (-> Result<Self, E>)",
                found: TokenKind::Bang,
                span: self.peek().span,
            });
        }

        // Default to Plain; will be inferred from return type during desugaring
        let kind = InitKind::Plain;
        let (receiver, params) = self.parse_receiver_or_param_list(true)?;

        // Parse optional return type annotation (e.g., `-> Option<Self>`)
        let return_type = if self.eat(TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };

        let (body, body_end) = self.parse_block_with_end()?;
        let span = Span::new(start, body_end);

        Ok(Spanned::new(
            InitDecl {
                docs,
                attributes,
                modifiers,
                kind,
                receiver,
                params,
                return_type,
                body,
            },
            span,
        ))
    }

    fn parse_receiver_or_param_list(
        &mut self,
        allow_receiver: bool,
    ) -> Result<ReceiverAndParams, ParseError> {
        self.expect(TokenKind::LParen)?;

        if self.at(TokenKind::RParen) {
            let _ = self.bump();
            return Ok((None, Vec::new()));
        }

        let mut receiver = None;
        let mut params = Vec::new();

        if allow_receiver {
            receiver = self.parse_receiver_if_present();
            if receiver.is_some() && self.eat(TokenKind::Comma).is_none() {
                self.expect(TokenKind::RParen)?;
                return Ok((receiver, params));
            }
        }

        if !self.at(TokenKind::RParen) {
            loop {
                params.push(self.parse_param_decl()?);
                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RParen) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        self.expect(TokenKind::RParen)?;
        Ok((receiver, params))
    }

    fn parse_receiver_if_present(&mut self) -> Option<Spanned<ReceiverKind>> {
        if self.at(TokenKind::KwSelfValue) {
            let token = self.bump();
            return Some(Spanned::new(ReceiverKind::Owned, token.span));
        }

        if !self.at(TokenKind::Amp) {
            return None;
        }

        if self.peek_nth(1).kind == TokenKind::KwSelfValue {
            let amp = self.bump();
            let self_tok = self.bump();
            return Some(Spanned::new(
                ReceiverKind::Ref,
                Span::new(amp.span.start, self_tok.span.end),
            ));
        }

        if self.peek_nth(1).kind == TokenKind::KwMut
            && self.peek_nth(2).kind == TokenKind::KwSelfValue
        {
            let amp = self.bump();
            let _mut_tok = self.bump();
            let self_tok = self.bump();
            return Some(Spanned::new(
                ReceiverKind::MutRef,
                Span::new(amp.span.start, self_tok.span.end),
            ));
        }

        None
    }

    /// Parses a top-level `macro` declaration.
    #[cfg(test)]
    pub(crate) fn parse_macro_decl(
        &mut self,
    ) -> Result<Spanned<MacroDecl>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        self.parse_macro_decl_with_prefix(start, docs, attributes)
    }

    fn parse_macro_decl_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
    ) -> Result<Spanned<MacroDecl>, ParseError> {
        self.expect(TokenKind::KwMacro)?;
        let (name, _) = self.expect_identifier_text()?;
        self.expect(TokenKind::LBrace)?;
        let mut clauses = Vec::new();

        while !self.at(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::UnexpectedEof {
                    expected: "'}'",
                    span: self.peek().span,
                });
            }
            clauses.push(self.parse_macro_clause()?);
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            MacroDecl {
                docs,
                attributes,
                name,
                clauses,
            },
            Span::new(start, rbrace.span.end),
        ))
    }

    fn parse_macro_clause(
        &mut self,
    ) -> Result<Spanned<MacroClause>, ParseError> {
        let start = self.peek().span.start;
        let kind = match self.peek().kind {
            TokenKind::KwRule => {
                let _ = self.bump();
                MacroClauseKind::Rule
            }
            TokenKind::KwReflect => {
                let _ = self.bump();
                MacroClauseKind::Reflect
            }
            _ => {
                return Err(ParseError::UnexpectedToken {
                    expected: "macro clause ('rule' or 'reflect')",
                    found: self.peek().kind,
                    span: self.peek().span,
                });
            }
        };

        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.at(TokenKind::RParen) {
            loop {
                params.push(self.parse_macro_param()?);
                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RParen) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::FatArrow)?;
        let (body, _) = self.parse_macro_block_tokens()?;
        let semi = self.expect(TokenKind::Semi)?;

        Ok(Spanned::new(
            MacroClause { kind, params, body },
            Span::new(start, semi.span.end),
        ))
    }

    fn parse_macro_param(&mut self) -> Result<Spanned<MacroParam>, ParseError> {
        let (name, name_span) = self.expect_identifier_text()?;
        self.expect(TokenKind::Colon)?;
        let (kind, kind_span) = self.parse_macro_input_kind()?;
        Ok(Spanned::new(
            MacroParam { name, kind },
            Span::new(name_span.start, kind_span.end),
        ))
    }

    fn parse_macro_input_kind(
        &mut self,
    ) -> Result<(MacroInputKind, Span), ParseError> {
        let (text, span) = self.expect_identifier_text()?;
        let kind = match text.as_str() {
            "Item" => MacroInputKind::Item,
            "Expr" => MacroInputKind::Expr,
            "Stmt" => MacroInputKind::Stmt,
            "Block" => MacroInputKind::Block,
            "Type" => MacroInputKind::Type,
            "Pattern" => MacroInputKind::Pattern,
            "Tokens" => MacroInputKind::Tokens,
            "MacroArgs" => MacroInputKind::MacroArgs,
            _ => {
                return Err(ParseError::UnexpectedToken {
                    expected: "macro input kind",
                    found: TokenKind::Ident,
                    span,
                });
            }
        };
        Ok((kind, span))
    }

    /// Parses an `extern` block and its foreign function members.
    #[cfg(test)]
    pub(crate) fn parse_extern_block(
        &mut self,
    ) -> Result<Spanned<ExternBlock>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        self.parse_extern_block_with_prefix(start, docs, attributes)
    }

    fn parse_extern_block_with_prefix(
        &mut self,
        start: usize,
        docs: Vec<Spanned<DocComment>>,
        attributes: Vec<Spanned<Attribute>>,
    ) -> Result<Spanned<ExternBlock>, ParseError> {
        self.expect(TokenKind::KwExtern)?;
        let (library_name, _) = self.expect_identifier_text()?;
        self.expect(TokenKind::LBrace)?;
        let mut members = Vec::new();

        while !self.at(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::UnexpectedEof {
                    expected: "'}'",
                    span: self.peek().span,
                });
            }

            let member = self.parse_extern_function_decl()?;
            let span = member.span;
            members.push(Spanned::new(ExternMember::Function(member), span));
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        let span = Span::new(start, rbrace.span.end);
        Ok(Spanned::new(
            ExternBlock {
                docs,
                attributes,
                library_name,
                members,
            },
            span,
        ))
    }

    /// Parses an extern member foreign function declaration terminated by `;`.
    pub(crate) fn parse_extern_function_decl(
        &mut self,
    ) -> Result<Spanned<ExternFunctionDecl>, ParseError> {
        let start = self.peek().span.start;
        let docs = self.parse_outer_doc_comments();
        let attributes = self.parse_attributes()?;
        self.expect(TokenKind::KwFn)?;
        let (local_name, _) = self.expect_identifier_text()?;
        let native_symbol = if self.eat(TokenKind::Eq).is_some() {
            let (symbol, _) = self.expect_identifier_text()?;
            Some(symbol)
        } else {
            None
        };
        let (_receiver, params) = self.parse_receiver_or_param_list(false)?;
        let return_type = if self.eat(TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let semi = self.expect(TokenKind::Semi)?;
        let span = Span::new(start, semi.span.end);

        Ok(Spanned::new(
            ExternFunctionDecl {
                docs,
                attributes,
                local_name,
                native_symbol,
                params,
                return_type,
            },
            span,
        ))
    }

    /// Parses a real statement block with optional tail expression.
    fn parse_block(&mut self) -> Result<ast::Block, ParseError> {
        self.parse_block_with_end().map(|(block, _)| block)
    }

    fn parse_block_with_end(
        &mut self,
    ) -> Result<(ast::Block, usize), ParseError> {
        if self.recovery_enabled && self.at(TokenKind::LBrace) {
            return Ok(self.parse_block_with_end_recovering());
        }

        self.expect(TokenKind::LBrace)?;
        let (statements, tail_expr, end) =
            self.parse_block_contents_until_rbrace()?;
        Ok((
            ast::Block {
                statements,
                tail_expr,
            },
            end,
        ))
    }

    fn parse_block_contents_until_rbrace(
        &mut self,
    ) -> Result<BlockContentsParse, ParseError> {
        let mut statements = Vec::new();
        let mut tail_expr = None;

        while !self.at(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::UnexpectedEof {
                    expected: "'}'",
                    span: self.peek().span,
                });
            }

            let kind = self.peek().kind;
            if kind == TokenKind::KwIf {
                let checkpoint = self.cursor;
                if let Ok(expr) = self.parse_if_expr() {
                    if let Some(semi) = self.eat(TokenKind::Semi) {
                        let span = Span::new(expr.span.start, semi.span.end);
                        statements.push(Spanned::new(
                            ast::Stmt::Expr {
                                expr: Box::new(expr),
                                has_semi: true,
                            },
                            span,
                        ));
                        continue;
                    }
                    if self.at(TokenKind::RBrace) {
                        tail_expr = Some(Box::new(expr));
                        break;
                    }
                }
                self.cursor = checkpoint;
                statements.push(self.parse_stmt()?);
                continue;
            }

            if Self::can_start_stmt(kind) {
                statements.push(self.parse_stmt()?);
                continue;
            }

            if self.attribute_prefix_before_statement() {
                return Err(ParseError::UnexpectedToken {
                    expected: "statement (attributes are only allowed on declarations)",
                    found: self.peek().kind,
                    span: self.peek().span,
                });
            }

            if Self::can_start_expr_statement(kind) {
                let expr = self.parse_expr()?;
                if let Some(semi) = self.eat(TokenKind::Semi) {
                    let span = Span::new(expr.span.start, semi.span.end);
                    statements.push(Spanned::new(
                        ast::Stmt::Expr {
                            expr: Box::new(expr),
                            has_semi: true,
                        },
                        span,
                    ));
                    continue;
                }

                if self.at(TokenKind::RBrace) {
                    tail_expr = Some(Box::new(expr));
                    break;
                }

                return Err(ParseError::UnexpectedToken {
                    expected: "';' or '}'",
                    found: self.peek().kind,
                    span: self.peek().span,
                });
            }

            return Err(ParseError::UnexpectedToken {
                expected: "statement or expression",
                found: kind,
                span: self.peek().span,
            });
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok((statements, tail_expr, rbrace.span.end))
    }

    fn parse_block_with_end_recovering(&mut self) -> (ast::Block, usize) {
        let _ = self.bump();
        let (statements, tail_expr, end) =
            self.parse_block_contents_until_rbrace_recovering();
        (
            ast::Block {
                statements,
                tail_expr,
            },
            end,
        )
    }

    fn parse_block_contents_until_rbrace_recovering(
        &mut self,
    ) -> BlockContentsParse {
        let mut statements = Vec::new();
        let mut tail_expr = None;

        loop {
            if self.at(TokenKind::RBrace) {
                let rbrace = self.bump();
                return (statements, tail_expr, rbrace.span.end);
            }

            if self.is_eof() {
                let error = ParseError::UnexpectedEof {
                    expected: "'}'",
                    span: self.peek().span,
                };
                self.record_parse_error(&error);
                return (statements, tail_expr, self.peek().span.end);
            }

            let kind = self.peek().kind;
            if kind == TokenKind::KwIf {
                self.parse_if_or_stmt_recovering(
                    &mut statements,
                    &mut tail_expr,
                );
                continue;
            }

            if Self::can_start_stmt(kind) {
                if let Some(stmt) = self.parse_stmt_recovering() {
                    statements.push(stmt);
                }
                continue;
            }

            if self.attribute_prefix_before_statement() {
                let error = ParseError::UnexpectedToken {
                    expected: "statement (attributes are only allowed on declarations)",
                    found: self.peek().kind,
                    span: self.peek().span,
                };
                self.record_parse_error(&error);
                let checkpoint = self.cursor;
                self.recover_to_statement_boundary_from(checkpoint);
                continue;
            }

            if Self::can_start_expr_statement(kind) {
                self.parse_expr_stmt_or_tail_recovering(
                    &mut statements,
                    &mut tail_expr,
                );
                continue;
            }

            let error = ParseError::UnexpectedToken {
                expected: "statement or expression",
                found: kind,
                span: self.peek().span,
            };
            self.record_parse_error(&error);
            let checkpoint = self.cursor;
            self.recover_to_statement_boundary_from(checkpoint);
        }
    }

    fn parse_if_or_stmt_recovering(
        &mut self,
        statements: &mut Vec<Spanned<ast::Stmt>>,
        tail_expr: &mut Option<Box<Spanned<Expr>>>,
    ) {
        let checkpoint = self.cursor;
        if let Ok(expr) = self.parse_if_expr() {
            if let Some(semi) = self.eat(TokenKind::Semi) {
                let span = Span::new(expr.span.start, semi.span.end);
                statements.push(Spanned::new(
                    ast::Stmt::Expr {
                        expr: Box::new(expr),
                        has_semi: true,
                    },
                    span,
                ));
                return;
            }
            if self.at(TokenKind::RBrace) {
                *tail_expr = Some(Box::new(expr));
                return;
            }
        }

        self.cursor = checkpoint;
        if let Some(stmt) = self.parse_stmt_recovering() {
            statements.push(stmt);
        }
    }

    fn parse_expr_stmt_or_tail_recovering(
        &mut self,
        statements: &mut Vec<Spanned<ast::Stmt>>,
        tail_expr: &mut Option<Box<Spanned<Expr>>>,
    ) {
        let checkpoint = self.cursor;
        match self.parse_expr() {
            Ok(expr) => {
                if let Some(semi) = self.eat(TokenKind::Semi) {
                    let span = Span::new(expr.span.start, semi.span.end);
                    statements.push(Spanned::new(
                        ast::Stmt::Expr {
                            expr: Box::new(expr),
                            has_semi: true,
                        },
                        span,
                    ));
                    return;
                }

                if self.at(TokenKind::RBrace) {
                    *tail_expr = Some(Box::new(expr));
                    return;
                }

                let error = ParseError::UnexpectedToken {
                    expected: "';' or '}'",
                    found: self.peek().kind,
                    span: self.peek().span,
                };
                self.record_parse_error(&error);
                self.recover_to_statement_boundary_from(self.cursor);
            }
            Err(error) => {
                self.record_parse_error(&error);
                self.recover_to_statement_boundary_from(checkpoint);
            }
        }
    }

    fn recover_to_statement_boundary_from(&mut self, checkpoint: usize) {
        self.synchronize_to_statement_boundary();
        if self.cursor == checkpoint && !self.is_eof() {
            let _ = self.bump();
        }
    }

    /// Parses one statement.
    fn parse_stmt(&mut self) -> Result<Spanned<ast::Stmt>, ParseError> {
        if self.attribute_prefix_before_statement() {
            return Err(ParseError::UnexpectedToken {
                expected: "statement (attributes are only allowed on declarations)",
                found: self.peek().kind,
                span: self.peek().span,
            });
        }

        match self.peek().kind {
            TokenKind::KwLet => {
                let stmt = self.parse_let_stmt()?;
                let span = stmt.span;
                Ok(Spanned::new(ast::Stmt::Let(stmt), span))
            }
            TokenKind::KwIf => {
                let stmt = self.parse_if_stmt()?;
                let span = stmt.span;
                Ok(Spanned::new(ast::Stmt::If(stmt), span))
            }
            TokenKind::KwVar => {
                let stmt = self.parse_var_stmt()?;
                let span = stmt.span;
                Ok(Spanned::new(ast::Stmt::Var(stmt), span))
            }
            TokenKind::KwGuard => {
                let stmt = self.parse_guard_stmt()?;
                let span = stmt.span;
                Ok(Spanned::new(ast::Stmt::Guard(stmt), span))
            }
            TokenKind::KwWhile => {
                let stmt = self.parse_while_stmt()?;
                let span = stmt.span;
                Ok(Spanned::new(ast::Stmt::While(stmt), span))
            }
            TokenKind::KwFor => {
                let stmt = self.parse_for_stmt()?;
                let span = stmt.span;
                Ok(Spanned::new(ast::Stmt::For(stmt), span))
            }
            TokenKind::KwReturn => self.parse_return_stmt(),
            TokenKind::KwBreak => self.parse_break_stmt(),
            TokenKind::KwContinue => self.parse_continue_stmt(),
            kind if Self::can_start_expr_statement(kind) => {
                let expr = self.parse_expr()?;
                let semi = self.expect(TokenKind::Semi)?;
                let span = Span::new(expr.span.start, semi.span.end);
                Ok(Spanned::new(
                    ast::Stmt::Expr {
                        expr: Box::new(expr),
                        has_semi: true,
                    },
                    span,
                ))
            }
            TokenKind::Eof => Err(ParseError::UnexpectedEof {
                expected: "statement",
                span: self.peek().span,
            }),
            _ => Err(ParseError::UnexpectedToken {
                expected: "statement",
                found: self.peek().kind,
                span: self.peek().span,
            }),
        }
    }

    fn parse_if_stmt(&mut self) -> Result<Spanned<ast::IfStmt>, ParseError> {
        let if_kw = self.expect(TokenKind::KwIf)?;
        let clauses = self.parse_clause_list()?;
        let (then_branch, then_end) = self.parse_block_with_end()?;
        let mut end = then_end;

        let else_branch = if self.eat(TokenKind::KwElse).is_some() {
            if self.at(TokenKind::KwIf) {
                let nested = self.parse_if_stmt()?;
                end = nested.span.end;
                Some(ast::IfStmtElse::If(Box::new(nested)))
            } else {
                let (block, block_end) = self.parse_block_with_end()?;
                end = block_end;
                Some(ast::IfStmtElse::Block(block))
            }
        } else {
            None
        };

        Ok(Spanned::new(
            ast::IfStmt {
                clauses,
                then_branch,
                else_branch,
            },
            Span::new(if_kw.span.start, end),
        ))
    }

    fn parse_let_stmt(&mut self) -> Result<Spanned<ast::LetStmt>, ParseError> {
        let let_kw = self.expect(TokenKind::KwLet)?;
        let pattern = self.parse_pattern()?;
        let ty = if self.eat(TokenKind::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let value = if self.eat(TokenKind::Eq).is_some() {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        let semi = self.expect(TokenKind::Semi)?;
        Ok(Spanned::new(
            LetStmt { pattern, ty, value },
            Span::new(let_kw.span.start, semi.span.end),
        ))
    }

    fn parse_var_stmt(&mut self) -> Result<Spanned<ast::VarStmt>, ParseError> {
        let var_kw = self.expect(TokenKind::KwVar)?;
        let pattern = self.parse_pattern()?;
        let ty = if self.eat(TokenKind::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let value = if self.eat(TokenKind::Eq).is_some() {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        let semi = self.expect(TokenKind::Semi)?;
        Ok(Spanned::new(
            VarStmt { pattern, ty, value },
            Span::new(var_kw.span.start, semi.span.end),
        ))
    }

    /// Parses a `guard` statement with shared clause-list syntax.
    fn parse_guard_stmt(
        &mut self,
    ) -> Result<Spanned<ast::GuardStmt>, ParseError> {
        let guard_kw = self.expect(TokenKind::KwGuard)?;
        let clauses = self.parse_clause_list()?;
        self.expect(TokenKind::KwElse)?;
        let (else_block, end) = self.parse_block_with_end()?;
        Ok(Spanned::new(
            ast::GuardStmt {
                clauses,
                else_block,
            },
            Span::new(guard_kw.span.start, end),
        ))
    }

    /// Parses a `while` statement with shared clause-list syntax.
    fn parse_while_stmt(
        &mut self,
    ) -> Result<Spanned<ast::WhileStmt>, ParseError> {
        let while_kw = self.expect(TokenKind::KwWhile)?;
        let clauses = self.parse_clause_list()?;
        let (body, end) = self.parse_block_with_end()?;
        Ok(Spanned::new(
            WhileStmt { clauses, body },
            Span::new(while_kw.span.start, end),
        ))
    }

    /// Parses a `for <pattern> in <expr> <block>` statement.
    fn parse_for_stmt(&mut self) -> Result<Spanned<ast::ForStmt>, ParseError> {
        let for_kw = self.expect(TokenKind::KwFor)?;
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::KwIn)?;
        let iterator = self.parse_expr()?;
        let (body, end) = self.parse_block_with_end()?;
        Ok(Spanned::new(
            ast::ForStmt {
                pattern,
                iterator: Box::new(iterator),
                body,
            },
            Span::new(for_kw.span.start, end),
        ))
    }

    /// Parses a semicolon-delimited clause list used by `guard` and `while`.
    fn parse_clause_list(&mut self) -> Result<ast::ClauseList, ParseError> {
        let mut clauses = Vec::new();
        clauses.push(self.parse_clause()?);
        while self.eat(TokenKind::Semi).is_some() {
            clauses.push(self.parse_clause()?);
        }
        Ok(ast::ClauseList { clauses })
    }

    fn parse_clause(&mut self) -> Result<Spanned<ast::Clause>, ParseError> {
        match self.peek().kind {
            TokenKind::KwLet => self.parse_binding_clause(BindingKind::Let),
            TokenKind::KwVar => self.parse_binding_clause(BindingKind::Var),
            _ => {
                let expr = self.parse_expr()?;
                let span = expr.span;
                Ok(Spanned::new(ast::Clause::Expr(Box::new(expr)), span))
            }
        }
    }

    fn parse_binding_clause(
        &mut self,
        kind: BindingKind,
    ) -> Result<Spanned<ast::Clause>, ParseError> {
        let start = self.peek().span.start;
        match kind {
            BindingKind::Let => {
                self.expect(TokenKind::KwLet)?;
            }
            BindingKind::Var => {
                self.expect(TokenKind::KwVar)?;
            }
        }

        let pattern = self.parse_pattern()?;
        let ty = if self.eat(TokenKind::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let end = value.span.end;
        let binding = ast::BindingClause {
            pattern,
            ty,
            value: Box::new(value),
        };
        let clause = match kind {
            BindingKind::Let => ast::Clause::LetBinding(binding),
            BindingKind::Var => ast::Clause::VarBinding(binding),
        };

        Ok(Spanned::new(clause, Span::new(start, end)))
    }

    fn parse_return_stmt(&mut self) -> Result<Spanned<ast::Stmt>, ParseError> {
        let return_kw = self.expect(TokenKind::KwReturn)?;
        let value = if self.at(TokenKind::Semi) {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };
        let semi = self.expect(TokenKind::Semi)?;
        Ok(Spanned::new(
            ast::Stmt::Return(value),
            Span::new(return_kw.span.start, semi.span.end),
        ))
    }

    fn parse_break_stmt(&mut self) -> Result<Spanned<ast::Stmt>, ParseError> {
        let break_kw = self.expect(TokenKind::KwBreak)?;
        let semi = self.expect(TokenKind::Semi)?;
        Ok(Spanned::new(
            ast::Stmt::Break,
            Span::new(break_kw.span.start, semi.span.end),
        ))
    }

    fn parse_continue_stmt(
        &mut self,
    ) -> Result<Spanned<ast::Stmt>, ParseError> {
        let continue_kw = self.expect(TokenKind::KwContinue)?;
        let semi = self.expect(TokenKind::Semi)?;
        Ok(Spanned::new(
            ast::Stmt::Continue,
            Span::new(continue_kw.span.start, semi.span.end),
        ))
    }

    fn can_start_top_level_item(kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::KwUse
                | TokenKind::KwScope
                | TokenKind::KwFn
                | TokenKind::KwStruct
                | TokenKind::KwEnum
                | TokenKind::KwImpl
                | TokenKind::KwProtocol
                | TokenKind::KwExtern
                | TokenKind::KwMacro
                | TokenKind::KwPub
                | TokenKind::KwAsync
                | TokenKind::KwUnsafe
                | TokenKind::At
        )
    }

    fn can_start_stmt(kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::KwIf
                | TokenKind::KwLet
                | TokenKind::KwVar
                | TokenKind::KwGuard
                | TokenKind::KwWhile
                | TokenKind::KwFor
                | TokenKind::KwReturn
                | TokenKind::KwBreak
                | TokenKind::KwContinue
        )
    }

    fn can_start_expr_statement(kind: TokenKind) -> bool {
        Self::can_start_expr(kind)
    }

    fn attribute_prefix_before_statement(&self) -> bool {
        if self.peek().kind != TokenKind::At
            || self.peek_nth(1).kind != TokenKind::Ident
        {
            return false;
        }

        let mut idx = 2usize;
        match self.peek_nth(idx).kind {
            TokenKind::LParen => {
                idx = self.scan_past_delimited(
                    idx,
                    TokenKind::LParen,
                    TokenKind::RParen,
                );
            }
            TokenKind::LBrace => {
                idx = self.scan_past_delimited(
                    idx,
                    TokenKind::LBrace,
                    TokenKind::RBrace,
                );
            }
            _ => {}
        }

        matches!(
            self.peek_nth(idx).kind,
            TokenKind::KwLet
                | TokenKind::KwVar
                | TokenKind::KwIf
                | TokenKind::KwGuard
                | TokenKind::KwWhile
                | TokenKind::KwFor
                | TokenKind::KwReturn
                | TokenKind::KwBreak
                | TokenKind::KwContinue
        )
    }

    fn scan_past_delimited(
        &self,
        mut idx: usize,
        open: TokenKind,
        close: TokenKind,
    ) -> usize {
        if self.peek_nth(idx).kind != open {
            return idx;
        }

        let mut depth = 0usize;
        loop {
            let kind = self.peek_nth(idx).kind;
            if kind == TokenKind::Eof {
                return idx;
            }
            if kind == open {
                depth += 1;
            } else if kind == close {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return idx + 1;
                }
            }
            idx += 1;
        }
    }

    /// Parses a type in type-context grammar.
    ///
    /// Supported forms include named/path types, `Self`, references, raw
    /// pointers, arrays, grouped types, generic application, and postfix `?`
    /// / `!E` forms.
    ///
    /// Prefix forms (`&`, `&mut`, `*`, `*mut`) parse their inner type via
    /// `parse_type`, so postfix forms on that inner type bind first (for
    /// example, `&Foo?` parses as `&(Foo?)`).
    pub(crate) fn parse_type(&mut self) -> Result<Spanned<Type>, ParseError> {
        let mut ty = match self.peek().kind {
            TokenKind::Amp => {
                let start = self.bump().span.start;
                let mutable = self.eat(TokenKind::KwMut).is_some();
                let inner = self.parse_type()?;
                let span = Span::new(start, inner.span.end);
                if mutable {
                    Spanned::new(Type::MutableReference(Box::new(inner)), span)
                } else {
                    Spanned::new(Type::Reference(Box::new(inner)), span)
                }
            }
            TokenKind::Star => {
                let start = self.bump().span.start;
                let mutable = self.eat(TokenKind::KwMut).is_some();
                let inner = self.parse_type()?;
                let span = Span::new(start, inner.span.end);
                if mutable {
                    Spanned::new(Type::MutablePointer(Box::new(inner)), span)
                } else {
                    // `*T` source syntax currently maps to this legacy AST
                    // variant name.
                    Spanned::new(Type::ConstPointer(Box::new(inner)), span)
                }
            }
            TokenKind::LBracket => {
                let start = self.bump().span.start;
                let inner = self.parse_type()?;
                let end = self.expect(TokenKind::RBracket)?.span.end;
                let span = Span::new(start, end);
                Spanned::new(Type::Array(Box::new(inner)), span)
            }
            TokenKind::LParen => {
                let start = self.bump().span.start;
                let inner = self.parse_type()?;
                let end = self.expect(TokenKind::RParen)?.span.end;
                let span = Span::new(start, end);
                Spanned::new(Type::Grouped(Box::new(inner)), span)
            }
            TokenKind::KwSelfType => {
                let tok = self.bump();
                Spanned::new(Type::SelfType, tok.span)
            }
            TokenKind::Ident => self.parse_named_or_generic_type()?,
            TokenKind::Eof => {
                return Err(ParseError::UnexpectedEof {
                    expected: "type",
                    span: self.peek().span,
                });
            }
            _ => {
                return Err(ParseError::UnexpectedToken {
                    expected: "type",
                    found: self.peek().kind,
                    span: self.peek().span,
                });
            }
        };

        if self.eat(TokenKind::Question).is_some() {
            let span = Span::new(ty.span.start, self.peek_nth(0).span.start);
            ty = Spanned::new(Type::Optional(Box::new(ty)), span);
        } else if self.eat(TokenKind::Bang).is_some() {
            let err_ty = self.parse_type()?;
            let span = Span::new(ty.span.start, err_ty.span.end);
            ty = Spanned::new(
                Type::Result {
                    ok: Box::new(ty),
                    err: Box::new(err_ty),
                },
                span,
            );
        }

        Ok(ty)
    }

    fn parse_named_or_generic_type(
        &mut self,
    ) -> Result<Spanned<Type>, ParseError> {
        let (first, first_span) = self.expect_identifier_text()?;
        let mut segments = vec![first];
        let mut end = first_span.end;

        while self.eat(TokenKind::ColonColon).is_some() {
            let (segment, seg_span) = self.expect_identifier_text()?;
            segments.push(segment);
            end = seg_span.end;
        }

        let named_span = Span::new(first_span.start, end);
        let named = Spanned::new(Type::Named { segments }, named_span);

        if !self.at(TokenKind::Lt) {
            return Ok(named);
        }

        let (args, generic_end) = self.parse_generic_arg_list()?;
        let full_span = Span::new(named_span.start, generic_end);
        Ok(Spanned::new(
            Type::GenericApplication {
                base: Box::new(named),
                args,
            },
            full_span,
        ))
    }

    fn parse_generic_arg_list(
        &mut self,
    ) -> Result<(Vec<Spanned<Type>>, usize), ParseError> {
        let _lt = self.expect(TokenKind::Lt)?;
        let mut args = Vec::new();

        if !self.at(TokenKind::Gt) {
            loop {
                args.push(self.parse_type()?);
                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::Gt) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let gt = self.expect(TokenKind::Gt)?;
        Ok((args, gt.span.end))
    }

    /// Parses one parameter declaration.
    ///
    /// Supported forms:
    /// - `x: T`
    /// - `_ x: T`
    /// - `label x: T`
    pub(crate) fn parse_param_decl(
        &mut self,
    ) -> Result<Spanned<ParamDecl>, ParseError> {
        let start = self.peek().span.start;
        let (first, _) = self.expect_identifier_text()?;

        let (label, name) = if first == "_" {
            // `_ x: T` -> external label = None, name = x
            let (name, _) = self.expect_identifier_text()?;
            (ParamLabel::None, name)
        } else if self.eat(TokenKind::Colon).is_some() {
            // `x: T` -> external label = FromName, name = x
            let ty = self.parse_type()?;
            let span = Span::new(start, ty.span.end);
            let node = ParamDecl {
                label: ParamLabel::FromName,
                name: first,
                ty,
            };
            return Ok(Spanned::new(node, span));
        } else {
            // `foo x: T` -> external label = Explicit("foo"), name = x
            let (name, _) = self.expect_identifier_text()?;
            (ParamLabel::Explicit(first), name)
        };

        let _colon = self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let span = Span::new(start, ty.span.end);
        let node = ParamDecl { label, name, ty };
        Ok(Spanned::new(node, span))
    }

    /// Parses a parenthesized parameter list.
    ///
    /// Supports empty lists and comma-separated declarations with optional
    /// trailing comma.
    #[cfg(test)]
    pub(crate) fn parse_param_list(
        &mut self,
    ) -> Result<Vec<Spanned<ParamDecl>>, ParseError> {
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();

        if !self.at(TokenKind::RParen) {
            loop {
                params.push(self.parse_param_decl()?);
                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RParen) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    /// Parses expressions using the current precedence ladder.
    ///
    /// This entry point handles assignment, range, logical/binary operators,
    /// prefix operators, and postfix/primary forms.
    fn parse_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        self.parse_assignment_expr()
    }

    /// Parses assignment expressions as right-associative forms.
    fn parse_assignment_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let lhs = self.parse_ternary_expr()?;
        let op = match self.peek().kind {
            TokenKind::Eq => Some(ast::AssignOp::Assign),
            TokenKind::PlusEq => Some(ast::AssignOp::AddAssign),
            TokenKind::MinusEq => Some(ast::AssignOp::SubAssign),
            TokenKind::StarEq => Some(ast::AssignOp::MulAssign),
            TokenKind::SlashEq => Some(ast::AssignOp::DivAssign),
            TokenKind::PercentEq => Some(ast::AssignOp::RemAssign),
            TokenKind::CaretEq => Some(ast::AssignOp::BitXorAssign),
            TokenKind::PipeEq => Some(ast::AssignOp::BitOrAssign),
            TokenKind::AmpEq => Some(ast::AssignOp::BitAndAssign),
            TokenKind::ShlEq => Some(ast::AssignOp::ShlAssign),
            TokenKind::ShrEq => Some(ast::AssignOp::ShrAssign),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let rhs = self.parse_assignment_expr()?;
            let span = Span::new(lhs.span.start, rhs.span.end);
            return Ok(Spanned::new(
                Expr::Assignment {
                    op,
                    target: Box::new(lhs),
                    value: Box::new(rhs),
                },
                span,
            ));
        }

        Ok(lhs)
    }

    fn parse_ternary_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let condition = self.parse_range_expr()?;
        if self.eat(TokenKind::Question).is_none() {
            return Ok(condition);
        }

        let then_expr = self.parse_expr()?;
        self.expect(TokenKind::Colon)?;
        let else_expr = self.parse_ternary_expr()?;
        let span = Span::new(condition.span.start, else_expr.span.end);
        Ok(Spanned::new(
            Expr::Ternary {
                condition: Box::new(condition),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            },
            span,
        ))
    }

    /// Parses range expressions (`..`, `..=`) around null-coalescing expressions.
    fn parse_range_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        if let Some(op) = self.eat(TokenKind::DotDotEq) {
            if !Self::can_start_expr(self.peek().kind) {
                return Err(ParseError::UnexpectedToken {
                    expected: "expression",
                    found: self.peek().kind,
                    span: self.peek().span,
                });
            }
            let end = self.parse_null_coalescing_expr()?;
            return Ok(Spanned::new(
                Expr::Range {
                    start: None,
                    end: Some(Box::new(end.clone())),
                    inclusive: true,
                },
                Span::new(op.span.start, end.span.end),
            ));
        }

        if let Some(op) = self.eat(TokenKind::DotDot) {
            if !Self::can_start_expr(self.peek().kind) {
                return Err(ParseError::UnexpectedToken {
                    expected: "expression",
                    found: self.peek().kind,
                    span: self.peek().span,
                });
            }
            let end = self.parse_null_coalescing_expr()?;
            return Ok(Spanned::new(
                Expr::Range {
                    start: None,
                    end: Some(Box::new(end.clone())),
                    inclusive: false,
                },
                Span::new(op.span.start, end.span.end),
            ));
        }

        let start = self.parse_null_coalescing_expr()?;
        if self.eat(TokenKind::DotDotEq).is_some() {
            let end = self.parse_null_coalescing_expr()?;
            return Ok(Spanned::new(
                Expr::Range {
                    start: Some(Box::new(start.clone())),
                    end: Some(Box::new(end.clone())),
                    inclusive: true,
                },
                Span::new(start.span.start, end.span.end),
            ));
        }

        if let Some(op) = self.eat(TokenKind::DotDot) {
            if Self::can_start_expr(self.peek().kind) {
                let end = self.parse_null_coalescing_expr()?;
                return Ok(Spanned::new(
                    Expr::Range {
                        start: Some(Box::new(start.clone())),
                        end: Some(Box::new(end.clone())),
                        inclusive: false,
                    },
                    Span::new(start.span.start, end.span.end),
                ));
            }

            return Ok(Spanned::new(
                Expr::Range {
                    start: Some(Box::new(start.clone())),
                    end: None,
                    inclusive: false,
                },
                Span::new(start.span.start, op.span.end),
            ));
        }

        Ok(start)
    }

    fn parse_null_coalescing_expr(
        &mut self,
    ) -> Result<Spanned<Expr>, ParseError> {
        let lhs = self.parse_logical_or_expr()?;
        if self.eat(TokenKind::QuestionQuestion).is_none() {
            return Ok(lhs);
        }
        let rhs = self.parse_null_coalescing_expr()?;
        let span = Span::new(lhs.span.start, rhs.span.end);
        Ok(Spanned::new(
            Expr::Binary {
                op: ast::BinaryOp::NullCoalescing,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        ))
    }

    fn parse_logical_or_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_logical_and_expr()?;
        while self.eat(TokenKind::PipePipe).is_some() {
            let rhs = self.parse_logical_and_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op: ast::BinaryOp::LogicalOr,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_logical_and_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_bitwise_or_expr()?;
        while self.eat(TokenKind::AmpAmp).is_some() {
            let rhs = self.parse_bitwise_or_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op: ast::BinaryOp::LogicalAnd,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_bitwise_or_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_bitwise_xor_expr()?;
        while self.eat(TokenKind::Pipe).is_some() {
            let rhs = self.parse_bitwise_xor_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op: ast::BinaryOp::BitOr,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_bitwise_xor_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_bitwise_and_expr()?;
        while self.eat(TokenKind::Caret).is_some() {
            let rhs = self.parse_bitwise_and_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op: ast::BinaryOp::BitXor,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_bitwise_and_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_equality_expr()?;
        while self.eat(TokenKind::Amp).is_some() {
            let rhs = self.parse_equality_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op: ast::BinaryOp::BitAnd,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_equality_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_comparison_expr()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::EqEq => Some(ast::BinaryOp::Equal),
                TokenKind::BangEq => Some(ast::BinaryOp::NotEqual),
                _ => None,
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_comparison_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_comparison_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_shift_expr()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Lt => Some(ast::BinaryOp::Less),
                TokenKind::Le => Some(ast::BinaryOp::LessEqual),
                TokenKind::Gt => Some(ast::BinaryOp::Greater),
                TokenKind::Ge => Some(ast::BinaryOp::GreaterEqual),
                _ => None,
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_shift_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_shift_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_additive_expr()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Shl => Some(ast::BinaryOp::ShiftLeft),
                TokenKind::Shr => Some(ast::BinaryOp::ShiftRight),
                _ => None,
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_additive_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_additive_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_multiplicative_expr()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => Some(ast::BinaryOp::Add),
                TokenKind::Minus => Some(ast::BinaryOp::Subtract),
                _ => None,
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_multiplicative_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_multiplicative_expr(
        &mut self,
    ) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_cast_expr()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => Some(ast::BinaryOp::Multiply),
                TokenKind::Slash => Some(ast::BinaryOp::Divide),
                TokenKind::Percent => Some(ast::BinaryOp::Remainder),
                _ => None,
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_cast_expr()?;
            let span = Span::new(expr.span.start, rhs.span.end);
            expr = Spanned::new(
                Expr::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_cast_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_prefix_expr()?;
        while self.eat(TokenKind::KwAs).is_some() {
            let is_optional = self.eat(TokenKind::Question).is_some();
            let ty = self.parse_type()?;
            let span = Span::new(expr.span.start, ty.span.end);
            expr = Spanned::new(
                Expr::Cast {
                    expr: Box::new(expr),
                    ty,
                    is_optional,
                },
                span,
            );
        }
        Ok(expr)
    }

    /// Parses prefix unary operators before postfix/primary forms.
    fn parse_prefix_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        if let Some(op) = self.eat(TokenKind::Minus) {
            let expr = self.parse_prefix_expr()?;
            return Ok(Spanned::new(
                Expr::Unary {
                    op: ast::UnaryOp::Negate,
                    expr: Box::new(expr.clone()),
                },
                Span::new(op.span.start, expr.span.end),
            ));
        }

        if let Some(op) = self.eat(TokenKind::Bang) {
            let expr = self.parse_prefix_expr()?;
            return Ok(Spanned::new(
                Expr::Unary {
                    op: ast::UnaryOp::Not,
                    expr: Box::new(expr.clone()),
                },
                Span::new(op.span.start, expr.span.end),
            ));
        }

        if let Some(op) = self.eat(TokenKind::KwTry) {
            let expr = self.parse_prefix_expr()?;
            return Ok(Spanned::new(
                Expr::Try {
                    expr: Box::new(expr.clone()),
                },
                Span::new(op.span.start, expr.span.end),
            ));
        }

        self.parse_postfix_expr()
    }

    fn can_start_expr(kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::KwIf
                | TokenKind::KwMatch
                | TokenKind::KwUnsafe
                | TokenKind::LBrace
                | TokenKind::ClosureShorthandParam
                | TokenKind::Ident
                | TokenKind::Integer
                | TokenKind::Float
                | TokenKind::KwTrue
                | TokenKind::KwFalse
                | TokenKind::Char
                | TokenKind::StringStart
                | TokenKind::KwSelfValue
                | TokenKind::KwSelfType
                | TokenKind::LParen
                | TokenKind::LBracket
                | TokenKind::Dot
                | TokenKind::At
                | TokenKind::Minus
                | TokenKind::Bang
                | TokenKind::KwTry
        )
    }

    /// Parses a primary expression atom.
    ///
    /// Supported forms include identifiers, literals, grouped expressions,
    /// arrays, shorthand members, macros, basic struct literals, control
    /// expressions (`if`, `match`, closures), and segmented strings with
    /// interpolation assembly.
    fn parse_primary_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let token = *self.peek();
        match token.kind {
            TokenKind::KwIf => self.parse_if_expr(),
            TokenKind::KwMatch => self.parse_match_expr(),
            TokenKind::KwUnsafe => self.parse_unsafe_expr(),
            TokenKind::LBrace => self.parse_closure_expr(),
            TokenKind::Ident => self.parse_struct_literal_or_identifier_expr(),
            TokenKind::ClosureShorthandParam => {
                let raw = self.slice(token.span).to_owned();
                let _ = self.bump();
                Ok(Spanned::new(Expr::Identifier(raw), token.span))
            }
            TokenKind::Integer => {
                let raw = self.slice(token.span).to_owned();
                let _ = self.bump();
                Ok(Spanned::new(Expr::IntegerLiteral(raw), token.span))
            }
            TokenKind::Float => {
                let raw = self.slice(token.span).to_owned();
                let _ = self.bump();
                Ok(Spanned::new(Expr::FloatLiteral(raw), token.span))
            }
            TokenKind::KwTrue => {
                let _ = self.bump();
                Ok(Spanned::new(Expr::BooleanLiteral(true), token.span))
            }
            TokenKind::KwFalse => {
                let _ = self.bump();
                Ok(Spanned::new(Expr::BooleanLiteral(false), token.span))
            }
            TokenKind::Char => {
                let raw = self.slice(token.span).to_owned();
                let _ = self.bump();
                Ok(Spanned::new(Expr::CharLiteral(raw), token.span))
            }
            TokenKind::StringStart => self.parse_string_literal_expr(),
            TokenKind::KwSelfValue => {
                let _ = self.bump();
                Ok(Spanned::new(Expr::SelfValue, token.span))
            }
            TokenKind::KwSelfType => {
                if self.peek_nth(1).kind == TokenKind::LBrace
                    && self.can_start_struct_literal_fields(2)
                {
                    // Self { ... } struct literal
                    let self_tok = self.bump();
                    self.parse_struct_literal_expr(
                        self_tok.span.start,
                        TypeExpr::SelfType,
                    )
                } else if self.peek_nth(1).kind == TokenKind::LParen {
                    // Self(...) constructor call
                    let self_tok = self.bump();
                    self.parse_constructor_call_expr(
                        self_tok.span.start,
                        "Self".to_string(),
                    )
                } else {
                    let _ = self.bump();
                    Ok(Spanned::new(Expr::SelfType, token.span))
                }
            }
            TokenKind::LParen => self.parse_grouped_expr(),
            TokenKind::LBracket => self.parse_array_literal_expr(),
            TokenKind::Dot => {
                let dot = self.bump();
                let (name, end) = self.expect_identifier_text()?;
                Ok(Spanned::new(
                    Expr::ShorthandMember { name },
                    Span::new(dot.span.start, end.end),
                ))
            }
            TokenKind::At => self.parse_macro_expr(),
            TokenKind::Eof => Err(ParseError::UnexpectedEof {
                expected: "expression",
                span: token.span,
            }),
            _ => Err(ParseError::UnexpectedToken {
                expected: "expression",
                found: token.kind,
                span: token.span,
            }),
        }
    }

    /// Parses expression-form `if` with required `else` branch.
    fn parse_if_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let if_kw = self.expect(TokenKind::KwIf)?;
        let clauses = self.parse_clause_list()?;
        let then_branch = self.parse_block()?;
        if self.eat(TokenKind::KwElse).is_none() {
            if self.is_eof() {
                return Err(ParseError::UnexpectedEof {
                    expected: "'else' branch for expression-form if",
                    span: self.peek().span,
                });
            }
            return Err(ParseError::UnexpectedToken {
                expected: "'else' branch for expression-form if",
                found: self.peek().kind,
                span: self.peek().span,
            });
        }

        let else_expr = if self.at(TokenKind::KwIf) {
            self.parse_if_expr()?
        } else if self.at(TokenKind::LBrace) {
            let else_start = self.peek().span.start;
            let (block, end) = self.parse_block_with_end()?;
            Spanned::new(Expr::Block(block), Span::new(else_start, end))
        } else {
            self.parse_expr()?
        };
        let span = Span::new(if_kw.span.start, else_expr.span.end);
        Ok(Spanned::new(
            Expr::If {
                clauses,
                then_branch,
                else_branch: Some(Box::new(else_expr)),
            },
            span,
        ))
    }

    /// Parses `match` expressions with comma-separated arms.
    fn parse_match_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let match_kw = self.expect(TokenKind::KwMatch)?;
        let subject = self.parse_expr()?;
        self.expect(TokenKind::LBrace)?;
        let mut arms = Vec::new();

        if !self.at(TokenKind::RBrace) {
            loop {
                arms.push(self.parse_match_arm()?);
                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RBrace) {
                        break;
                    }
                    continue;
                }

                if self.at(TokenKind::RBrace) {
                    break;
                }

                return Err(ParseError::UnexpectedToken {
                    expected: "',' or '}'",
                    found: self.peek().kind,
                    span: self.peek().span,
                });
            }
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            Expr::Match {
                subject: Box::new(subject),
                arms,
            },
            Span::new(match_kw.span.start, rbrace.span.end),
        ))
    }

    fn parse_match_arm(&mut self) -> Result<Spanned<MatchArm>, ParseError> {
        let start = self.peek().span.start;
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::FatArrow)?;
        let (body, end) = if self.at(TokenKind::LBrace) {
            let (block, end) = self.parse_block_with_end()?;
            (MatchArmBody::Block(block), end)
        } else {
            let expr = self.parse_expr()?;
            let end = expr.span.end;
            (MatchArmBody::Expr(Box::new(expr)), end)
        };
        Ok(Spanned::new(
            MatchArm { pattern, body },
            Span::new(start, end),
        ))
    }

    /// Parses closure expressions beginning with `{`.
    fn parse_closure_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let lbrace = self.expect(TokenKind::LBrace)?;
        let checkpoint = self.cursor;
        let mut params = Vec::new();
        let mut uses_shorthand_params = true;

        if let Some(parsed_params) = self.try_parse_closure_params_and_in()? {
            params = parsed_params;
            uses_shorthand_params = false;
        } else {
            self.cursor = checkpoint;
        }

        let (statements, tail_expr, end) =
            self.parse_block_contents_until_rbrace()?;
        let body = ast::Block {
            statements,
            tail_expr,
        };
        Ok(Spanned::new(
            Expr::Closure {
                params,
                body,
                uses_shorthand_params,
                is_unsafe: false,
            },
            Span::new(lbrace.span.start, end),
        ))
    }

    /// Parses `unsafe { ... }` as either an unsafe closure (when explicit
    /// closure signature is present) or an unsafe block expression.
    fn parse_unsafe_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let unsafe_kw = self.expect(TokenKind::KwUnsafe)?;
        if !self.at(TokenKind::LBrace) {
            return Err(ParseError::UnexpectedToken {
                expected: "'{' after 'unsafe'",
                found: self.peek().kind,
                span: self.peek().span,
            });
        }

        let _ = self.expect(TokenKind::LBrace)?;
        let checkpoint = self.cursor;

        if let Some(params) = self.try_parse_closure_params_and_in()? {
            let (statements, tail_expr, end) =
                self.parse_block_contents_until_rbrace()?;
            let body = ast::Block {
                statements,
                tail_expr,
            };
            return Ok(Spanned::new(
                Expr::Closure {
                    params,
                    body,
                    uses_shorthand_params: false,
                    is_unsafe: true,
                },
                Span::new(unsafe_kw.span.start, end),
            ));
        }

        self.cursor = checkpoint;
        let (statements, tail_expr, end) =
            self.parse_block_contents_until_rbrace()?;
        let block = ast::Block {
            statements,
            tail_expr,
        };
        Ok(Spanned::new(
            Expr::UnsafeBlock(block),
            Span::new(unsafe_kw.span.start, end),
        ))
    }

    fn try_parse_closure_params_and_in(
        &mut self,
    ) -> Result<Option<Vec<ast::ClosureParam>>, ParseError> {
        if !self.at(TokenKind::Ident) {
            return Ok(None);
        }

        let mut params = Vec::new();
        loop {
            if !self.at(TokenKind::Ident) {
                return Ok(None);
            }

            let (name, _) = self.expect_identifier_text()?;
            let ty = if self.eat(TokenKind::Colon).is_some() {
                Some(self.parse_type()?)
            } else {
                None
            };
            params.push(ast::ClosureParam { name, ty });

            if self.eat(TokenKind::Comma).is_some() {
                continue;
            }

            break;
        }

        if self.eat(TokenKind::KwIn).is_none() {
            return Ok(None);
        }

        Ok(Some(params))
    }

    /// Parses postfix expression chains over a primary base expression.
    ///
    /// Supported suffixes are member access, namespace/static access, calls,
    /// and indexing.
    fn parse_postfix_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            if !self.try_parse_postfix_suffix(&mut expr)? {
                break;
            }
        }

        Ok(expr)
    }

    fn try_parse_postfix_suffix(
        &mut self,
        expr: &mut Spanned<Expr>,
    ) -> Result<bool, ParseError> {
        let next = match self.peek().kind {
            TokenKind::QuestionDot => {
                self.parse_optional_member_access(expr)?
            }
            TokenKind::Dot => self.parse_member_access(expr)?,
            TokenKind::ColonColon => self.parse_namespace_access(expr)?,
            TokenKind::LParen => self.parse_call_expr(expr)?,
            TokenKind::LBracket => self.parse_index_expr(expr)?,
            TokenKind::Question
                if self.peek_nth(1).kind == TokenKind::LBracket =>
            {
                self.parse_optional_index_expr(expr)?
            }
            TokenKind::Bang => self.parse_force_unwrap_expr(expr),
            _ => return Ok(false),
        };
        *expr = next;
        Ok(true)
    }

    fn parse_optional_member_access(
        &mut self,
        expr: &Spanned<Expr>,
    ) -> Result<Spanned<Expr>, ParseError> {
        self.bump();
        let (member, member_span) = self.expect_identifier_text()?;
        let span = Span::new(expr.span.start, member_span.end);
        Ok(Spanned::new(
            Expr::OptionalMemberAccess {
                base: Box::new(expr.clone()),
                member,
            },
            span,
        ))
    }

    fn parse_member_access(
        &mut self,
        expr: &Spanned<Expr>,
    ) -> Result<Spanned<Expr>, ParseError> {
        let _dot = self.bump();
        let (member, member_span) = self.expect_identifier_text()?;
        let span = Span::new(expr.span.start, member_span.end);
        Ok(Spanned::new(
            Expr::MemberAccess {
                base: Box::new(expr.clone()),
                member,
            },
            span,
        ))
    }

    fn parse_namespace_access(
        &mut self,
        expr: &Spanned<Expr>,
    ) -> Result<Spanned<Expr>, ParseError> {
        self.bump();
        let (member, member_span) = self.expect_namespace_member_text()?;
        let mut turbofish = Vec::new();
        let mut end = member_span.end;
        if self.at(TokenKind::ColonColon)
            && self.peek_nth(1).kind == TokenKind::Lt
        {
            self.bump();
            let (args, generic_end) = self.parse_generic_arg_list()?;
            turbofish = args;
            end = generic_end;
        }
        let span = Span::new(expr.span.start, end);
        Ok(Spanned::new(
            Expr::NamespaceAccess {
                base: Box::new(expr.clone()),
                member,
                turbofish,
            },
            span,
        ))
    }

    fn parse_call_expr(
        &mut self,
        expr: &Spanned<Expr>,
    ) -> Result<Spanned<Expr>, ParseError> {
        let (args, end) = self.parse_call_arg_list()?;
        let span = Span::new(expr.span.start, end);
        Ok(Spanned::new(
            Expr::Call {
                callee: Box::new(expr.clone()),
                args,
                trailing_closure: None,
            },
            span,
        ))
    }

    fn parse_index_expr(
        &mut self,
        expr: &Spanned<Expr>,
    ) -> Result<Spanned<Expr>, ParseError> {
        let _ = self.bump();
        let index = self.parse_expr()?;
        let rbracket = self.expect(TokenKind::RBracket)?;
        let span = Span::new(expr.span.start, rbracket.span.end);
        Ok(Spanned::new(
            Expr::Index {
                base: Box::new(expr.clone()),
                index: Box::new(index),
            },
            span,
        ))
    }

    fn parse_optional_index_expr(
        &mut self,
        expr: &Spanned<Expr>,
    ) -> Result<Spanned<Expr>, ParseError> {
        self.bump();
        self.expect(TokenKind::LBracket)?;
        let index = self.parse_expr()?;
        let rbracket = self.expect(TokenKind::RBracket)?;
        let span = Span::new(expr.span.start, rbracket.span.end);
        Ok(Spanned::new(
            Expr::OptionalIndex {
                base: Box::new(expr.clone()),
                index: Box::new(index),
            },
            span,
        ))
    }

    fn parse_force_unwrap_expr(
        &mut self,
        expr: &Spanned<Expr>,
    ) -> Spanned<Expr> {
        let bang = self.bump();
        let start = expr.span.start;
        Spanned::new(
            Expr::ForceUnwrap {
                expr: Box::new(expr.clone()),
            },
            Span::new(start, bang.span.end),
        )
    }

    fn parse_grouped_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let lparen = self.expect(TokenKind::LParen)?;
        let inner = self.parse_expr()?;
        let rparen = self.expect(TokenKind::RParen)?;
        Ok(Spanned::new(
            Expr::Grouped(Box::new(inner)),
            Span::new(lparen.span.start, rparen.span.end),
        ))
    }

    fn parse_array_literal_expr(
        &mut self,
    ) -> Result<Spanned<Expr>, ParseError> {
        let lbracket = self.expect(TokenKind::LBracket)?;
        let mut elements = Vec::new();

        if !self.at(TokenKind::RBracket) {
            loop {
                let value = self.parse_expr()?;
                elements.push(ArrayElement::Expr(Box::new(value)));
                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RBracket) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let rbracket = self.expect(TokenKind::RBracket)?;
        Ok(Spanned::new(
            Expr::ArrayLiteral(elements),
            Span::new(lbracket.span.start, rbracket.span.end),
        ))
    }

    fn parse_struct_literal_or_identifier_expr(
        &mut self,
    ) -> Result<Spanned<Expr>, ParseError> {
        // Check for struct literal: TypeName { ... }
        if self.peek_nth(1).kind == TokenKind::LBrace
            && self.ident_token_looks_type_head(*self.peek())
            && self.can_start_struct_literal_fields(2)
        {
            let (name, name_span) = self.expect_identifier_text()?;
            return self.parse_struct_literal_expr(
                name_span.start,
                TypeExpr::Path(vec![name]),
            );
        }

        // Check for constructor call: TypeName(...)
        // Creates a ConstructorCall AST node - desugaring and validation happen later
        if self.peek_nth(1).kind == TokenKind::LParen
            && self.ident_token_looks_type_head(*self.peek())
        {
            let (name, name_span) = self.expect_identifier_text()?;
            return self.parse_constructor_call_expr(name_span.start, name);
        }

        let (name, span) = self.expect_identifier_text()?;
        Ok(Spanned::new(Expr::Identifier(name), span))
    }

    fn ident_token_looks_type_head(&self, token: Token) -> bool {
        let text = self.slice(token.span);
        text.chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
    }

    fn can_start_struct_literal_fields(&self, offset: usize) -> bool {
        match self.peek_nth(offset).kind {
            TokenKind::RBrace => true,
            TokenKind::Ident => matches!(
                self.peek_nth(offset + 1).kind,
                TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace
            ),
            _ => false,
        }
    }

    fn parse_struct_literal_expr(
        &mut self,
        start: usize,
        ty: TypeExpr,
    ) -> Result<Spanned<Expr>, ParseError> {
        let lbrace = self.expect(TokenKind::LBrace)?;
        let _ = lbrace;
        let mut fields = Vec::new();

        if !self.at(TokenKind::RBrace) {
            loop {
                let (name, _) = self.expect_identifier_text()?;
                if self.eat(TokenKind::Colon).is_some() {
                    let value = self.parse_expr()?;
                    fields.push(StructLiteralField::Named {
                        name,
                        value: Box::new(value),
                    });
                } else {
                    fields.push(StructLiteralField::Shorthand { name });
                }

                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RBrace) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        let literal = Expr::StructLiteral { ty, fields };
        Ok(Spanned::new(literal, Span::new(start, rbrace.span.end)))
    }

    fn parse_constructor_call_expr(
        &mut self,
        start: usize,
        type_name: String,
    ) -> Result<Spanned<Expr>, ParseError> {
        // Parse the argument list: TypeName(...)
        let (args, end) = self.parse_call_arg_list()?;

        // Create a ConstructorCall AST node
        // Desugaring and validation happen in later phases
        Ok(Spanned::new(
            Expr::ConstructorCall { type_name, args },
            Span::new(start, end),
        ))
    }

    fn parse_string_literal_expr(
        &mut self,
    ) -> Result<Spanned<Expr>, ParseError> {
        let start = self.expect(TokenKind::StringStart)?.span.start;
        let mut parts = Vec::new();

        loop {
            match self.peek().kind {
                TokenKind::StringText => {
                    let token = self.bump();
                    let text = self.slice(token.span).to_owned();
                    parts.push(StringPart::Text(text));
                }
                TokenKind::InterpolationStart => {
                    self.bump();
                    let expr = self.parse_expr()?;
                    self.expect(TokenKind::InterpolationEnd)?;
                    parts.push(StringPart::Interpolation(Box::new(expr)));
                }
                TokenKind::StringEnd => {
                    let end = self.bump().span.end;
                    return Ok(Spanned::new(
                        Expr::StringLiteral(StringLiteral { parts }),
                        Span::new(start, end),
                    ));
                }
                TokenKind::Eof => {
                    return Err(ParseError::UnexpectedEof {
                        expected: "string segment",
                        span: self.peek().span,
                    });
                }
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "string segment",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
            }
        }
    }

    fn parse_macro_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let at = self.expect(TokenKind::At)?;
        let (name, _) = self.expect_identifier_text()?;
        let (args, end) = if self.at(TokenKind::LParen) {
            let (args, end) = self.parse_call_arg_list()?;
            (MacroExprArgs::Paren(args), end)
        } else if self.at(TokenKind::LBrace) {
            let (block, end) = self.parse_macro_block_tokens()?;
            (MacroExprArgs::Braced(block), end)
        } else {
            (MacroExprArgs::Paren(Vec::new()), self.peek().span.start)
        };

        Ok(Spanned::new(
            Expr::Macro { name, args },
            Span::new(at.span.start, end),
        ))
    }

    fn parse_call_arg_list(
        &mut self,
    ) -> Result<(Vec<CallArg>, usize), ParseError> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();

        if !self.at(TokenKind::RParen) {
            loop {
                args.push(self.parse_call_arg()?);
                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RParen) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let rparen = self.expect(TokenKind::RParen)?;
        Ok((args, rparen.span.end))
    }

    fn parse_call_arg(&mut self) -> Result<CallArg, ParseError> {
        if self.at(TokenKind::Ident)
            && self.peek_nth(1).kind == TokenKind::Colon
        {
            let (label, _) = self.expect_identifier_text()?;
            self.expect(TokenKind::Colon)?;
            let value = self.parse_expr()?;
            return Ok(CallArg {
                label: Some(label),
                value: Box::new(value),
            });
        }

        let value = self.parse_expr()?;
        Ok(CallArg {
            label: None,
            value: Box::new(value),
        })
    }

    /// Parses source patterns for bindings and match arms.
    fn parse_pattern(&mut self) -> Result<Spanned<Pattern>, ParseError> {
        let token = *self.peek();
        match token.kind {
            TokenKind::Ident => {
                let text = self.slice(token.span).to_owned();
                if text == "_" {
                    let _ = self.bump();
                    return Ok(Spanned::new(Pattern::Wildcard, token.span));
                }
                if self.peek_nth(1).kind == TokenKind::LParen {
                    return self.parse_variant_pattern(false);
                }
                if self.peek_nth(1).kind == TokenKind::LBrace
                    && self.ident_token_looks_type_head(token)
                {
                    return self.parse_struct_pattern();
                }
                let _ = self.bump();
                Ok(Spanned::new(Pattern::Identifier(text), token.span))
            }
            TokenKind::Integer => {
                let raw = self.slice(token.span).to_owned();
                let _ = self.bump();
                Ok(Spanned::new(Pattern::IntegerLiteral(raw), token.span))
            }
            TokenKind::KwTrue => {
                let _ = self.bump();
                Ok(Spanned::new(Pattern::BooleanLiteral(true), token.span))
            }
            TokenKind::KwFalse => {
                let _ = self.bump();
                Ok(Spanned::new(Pattern::BooleanLiteral(false), token.span))
            }
            TokenKind::Char => {
                let raw = self.slice(token.span).to_owned();
                let _ = self.bump();
                Ok(Spanned::new(Pattern::CharLiteral(raw), token.span))
            }
            TokenKind::StringStart => self.parse_string_literal_pattern(),
            TokenKind::LParen => self.parse_tuple_pattern(),
            TokenKind::LBracket => self.parse_array_pattern(),
            TokenKind::Dot => {
                if self.peek_nth(1).kind == TokenKind::DotDot {
                    return Err(ParseError::UnexpectedToken {
                        expected: "pattern",
                        found: token.kind,
                        span: token.span,
                    });
                }
                self.parse_variant_pattern(true)
            }
            TokenKind::Eof => Err(ParseError::UnexpectedEof {
                expected: "pattern",
                span: token.span,
            }),
            _ => Err(ParseError::UnexpectedToken {
                expected: "pattern",
                found: token.kind,
                span: token.span,
            }),
        }
    }

    fn parse_tuple_pattern(&mut self) -> Result<Spanned<Pattern>, ParseError> {
        let lparen = self.expect(TokenKind::LParen)?;
        let mut elements = Vec::new();
        elements.push(self.parse_pattern()?);

        if self.eat(TokenKind::Comma).is_none() {
            return Err(ParseError::UnexpectedToken {
                expected: "',' in tuple pattern",
                found: self.peek().kind,
                span: self.peek().span,
            });
        }

        if !self.at(TokenKind::RParen) {
            loop {
                elements.push(self.parse_pattern()?);
                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RParen) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let rparen = self.expect(TokenKind::RParen)?;
        Ok(Spanned::new(
            Pattern::Tuple(elements),
            Span::new(lparen.span.start, rparen.span.end),
        ))
    }

    fn parse_variant_pattern(
        &mut self,
        shorthand: bool,
    ) -> Result<Spanned<Pattern>, ParseError> {
        let start = self.peek().span.start;
        if shorthand {
            self.expect(TokenKind::Dot)?;
        }
        let (name, name_span) = self.expect_identifier_text()?;
        let mut args = Vec::new();
        let mut has_rest = false;
        let mut end = name_span.end;

        if self.eat(TokenKind::LParen).is_some() {
            if !self.at(TokenKind::RParen) {
                loop {
                    if self.eat(TokenKind::DotDot).is_some() {
                        if has_rest {
                            return Err(ParseError::UnexpectedToken {
                                expected: "at most one `..` rest marker in variant pattern payload",
                                found: self.peek().kind,
                                span: self.peek().span,
                            });
                        }
                        has_rest = true;
                        if self.eat(TokenKind::Comma).is_some() {
                            if !self.at(TokenKind::RParen) {
                                return Err(ParseError::UnexpectedToken {
                                    expected: "`..` rest marker must be final in variant pattern payload",
                                    found: self.peek().kind,
                                    span: self.peek().span,
                                });
                            }
                        } else if !self.at(TokenKind::RParen) {
                            return Err(ParseError::UnexpectedToken {
                                expected: "`..` rest marker must be final in variant pattern payload",
                                found: self.peek().kind,
                                span: self.peek().span,
                            });
                        }
                        break;
                    }
                    if has_rest {
                        return Err(ParseError::UnexpectedToken {
                            expected: "`..` rest marker must be final in variant pattern payload",
                            found: self.peek().kind,
                            span: self.peek().span,
                        });
                    }
                    args.push(self.parse_pattern()?);

                    if self.eat(TokenKind::Comma).is_some() {
                        if self.at(TokenKind::RParen) {
                            break;
                        }
                        continue;
                    }
                    break;
                }
            }

            let rparen = self.expect(TokenKind::RParen)?;
            end = rparen.span.end;
        }

        Ok(Spanned::new(
            Pattern::Variant {
                path: vec![name],
                shorthand,
                args,
                has_rest,
            },
            Span::new(start, end),
        ))
    }

    fn parse_struct_pattern(&mut self) -> Result<Spanned<Pattern>, ParseError> {
        let (name, name_span) = self.expect_identifier_text()?;
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        let mut has_rest = false;

        if !self.at(TokenKind::RBrace) {
            loop {
                if self.eat(TokenKind::DotDot).is_some() {
                    if has_rest {
                        return Err(ParseError::UnexpectedToken {
                            expected: "at most one `..` rest marker in struct pattern",
                            found: self.peek().kind,
                            span: self.peek().span,
                        });
                    }
                    has_rest = true;
                    if self.eat(TokenKind::Comma).is_some() {
                        if !self.at(TokenKind::RBrace) {
                            return Err(ParseError::UnexpectedToken {
                                expected: "`..` rest marker must be final in struct pattern",
                                found: self.peek().kind,
                                span: self.peek().span,
                            });
                        }
                    } else if !self.at(TokenKind::RBrace) {
                        return Err(ParseError::UnexpectedToken {
                            expected: "`..` rest marker must be final in struct pattern",
                            found: self.peek().kind,
                            span: self.peek().span,
                        });
                    }
                    break;
                }
                if has_rest {
                    return Err(ParseError::UnexpectedToken {
                        expected: "`..` rest marker must be final in struct pattern",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                let (field, _) = self.expect_identifier_text()?;
                let pattern = if self.eat(TokenKind::Colon).is_some() {
                    Some(self.parse_pattern()?)
                } else {
                    None
                };
                fields.push(StructPatternField {
                    name: field,
                    pattern,
                });

                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RBrace) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            Pattern::Struct {
                path: vec![name],
                fields,
                has_rest,
            },
            Span::new(name_span.start, rbrace.span.end),
        ))
    }

    fn parse_array_pattern(&mut self) -> Result<Spanned<Pattern>, ParseError> {
        let lbracket = self.expect(TokenKind::LBracket)?;
        let mut elements = Vec::new();
        let mut rest = None;

        if !self.at(TokenKind::RBracket) {
            loop {
                if self.eat(TokenKind::DotDot).is_some() {
                    if rest.is_some() {
                        return Err(ParseError::UnexpectedToken {
                            expected: "at most one `..` rest marker in array pattern",
                            found: self.peek().kind,
                            span: self.peek().span,
                        });
                    }
                    rest = if self.at(TokenKind::Ident) {
                        let (name, _) = self.expect_identifier_text()?;
                        Some(ArrayPatternRest::Bind(name))
                    } else {
                        Some(ArrayPatternRest::Ignore)
                    };
                    if self.eat(TokenKind::Comma).is_some() {
                        if !self.at(TokenKind::RBracket) {
                            return Err(ParseError::UnexpectedToken {
                                expected: "`..` rest marker must be final in array pattern",
                                found: self.peek().kind,
                                span: self.peek().span,
                            });
                        }
                    } else if !self.at(TokenKind::RBracket) {
                        return Err(ParseError::UnexpectedToken {
                            expected: "`..` rest marker must be final in array pattern",
                            found: self.peek().kind,
                            span: self.peek().span,
                        });
                    }
                    break;
                }
                if rest.is_some() {
                    return Err(ParseError::UnexpectedToken {
                        expected: "`..` rest marker must be final in array pattern",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                elements.push(self.parse_pattern()?);

                if self.eat(TokenKind::Comma).is_some() {
                    if self.at(TokenKind::RBracket) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let rbracket = self.expect(TokenKind::RBracket)?;
        Ok(Spanned::new(
            Pattern::Array { elements, rest },
            Span::new(lbracket.span.start, rbracket.span.end),
        ))
    }

    fn parse_string_literal_pattern(
        &mut self,
    ) -> Result<Spanned<Pattern>, ParseError> {
        let start = self.expect(TokenKind::StringStart)?.span.start;
        loop {
            match self.peek().kind {
                TokenKind::StringText => {
                    self.bump();
                }
                TokenKind::InterpolationStart => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "non-interpolated string literal pattern",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
                TokenKind::StringEnd => {
                    let end = self.bump().span.end;
                    return Ok(Spanned::new(
                        Pattern::StringLiteral(
                            self.slice(Span::new(start, end)).to_owned(),
                        ),
                        Span::new(start, end),
                    ));
                }
                TokenKind::Eof => {
                    return Err(ParseError::UnexpectedEof {
                        expected: "string end",
                        span: self.peek().span,
                    });
                }
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "string segment",
                        found: self.peek().kind,
                        span: self.peek().span,
                    });
                }
            }
        }
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

    fn expect_namespace_member_text(
        &mut self,
    ) -> Result<(String, Span), ParseError> {
        let token = *self.peek();
        match token.kind {
            TokenKind::Ident | TokenKind::KwInit => {
                let member = self.slice(token.span).to_owned();
                let _ = self.bump();
                Ok((member, token.span))
            }
            TokenKind::Eof => Err(ParseError::UnexpectedEof {
                expected: "namespace member",
                span: token.span,
            }),
            _ => Err(ParseError::UnexpectedToken {
                expected: "namespace member",
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
        self.last_token_end = token.span.end;
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
///
/// # Errors
///
/// Returns `ParseError` if lexing fails or parsing cannot continue.
pub fn parse_source_file(
    source: &str,
) -> Result<crate::frontend::ParsedFile, ParseError> {
    let mut parser = Parser::new(source)?;
    let ast = parser.parse_file()?;
    Ok(crate::frontend::ParsedFile {
        file_id: crate::frontend::source::FileId::new(0),
        ast,
        diagnostics: crate::frontend::DiagnosticsBag::new(),
    })
}

/// Parses a whole source file with conservative recovery and diagnostics.
///
/// # Errors
///
/// Returns `ParseError` if lexing fails before parsing with recovery starts.
pub fn parse_source_file_with_recovery(
    source: &str,
) -> Result<crate::frontend::ParsedFile, ParseError> {
    let mut parser = Parser::new(source)?;
    let ast = parser.parse_file_with_recovery();
    Ok(crate::frontend::ParsedFile {
        file_id: crate::frontend::source::FileId::new(0),
        ast,
        diagnostics: parser.diagnostics,
    })
}

/// Parses a whole source file using an explicit file id.
pub(crate) fn parse_source_file_with_file_id(
    source: &str,
    file_id: crate::frontend::source::FileId,
) -> Result<crate::frontend::ParsedFile, ParseError> {
    let mut parser = Parser::new(source)?;
    let ast = parser.parse_file()?;
    Ok(crate::frontend::ParsedFile {
        file_id,
        ast,
        diagnostics: crate::frontend::DiagnosticsBag::new(),
    })
}

/// Parses a whole source file with recovery using an explicit file id for
/// diagnostics.
pub(crate) fn parse_source_file_with_recovery_and_file_id(
    source: &str,
    file_id: crate::frontend::source::FileId,
) -> Result<crate::frontend::ParsedFile, ParseError> {
    let mut parser = Parser::new(source)?;
    parser.enable_recovery_with_file_id(file_id);
    let ast = parser.parse_file_with_recovery();
    Ok(crate::frontend::ParsedFile {
        file_id,
        ast,
        diagnostics: parser.diagnostics,
    })
}

fn expected_for_token(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::KwUse => "'use'",
        TokenKind::KwScope => "'scope'",
        TokenKind::KwFn => "'fn'",
        TokenKind::KwStruct => "'struct'",
        TokenKind::KwEnum => "'enum'",
        TokenKind::KwImpl => "'impl'",
        TokenKind::KwProtocol => "'protocol'",
        TokenKind::KwExtern => "'extern'",
        TokenKind::KwMacro => "'macro'",
        TokenKind::Semi => "';'",
        TokenKind::LBrace => "'{'",
        TokenKind::RBrace => "'}'",
        TokenKind::LParen => "'('",
        TokenKind::RParen => "')'",
        TokenKind::Lt => "'<'",
        TokenKind::Gt => "'>'",
        TokenKind::Colon => "':'",
        TokenKind::Comma => "','",
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
    fn parse_minimal_function_decl() {
        let mut parser = Parser::new("fn f() {}").expect("parser creation");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 1);
        match &file.items[0].node {
            Item::Function(function) => {
                assert_eq!(function.node.name, "f");
                assert!(function.node.params.is_empty());
            }
            _ => panic!("expected function item"),
        }
    }

    #[test]
    fn parse_file_dispatches_top_level_struct_start() {
        let mut parser =
            Parser::new("struct Demo {}").expect("parser creation");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 1);
        assert!(matches!(file.items[0].node, Item::Struct(_)));
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
                UseTree::Path { path } => {
                    assert_eq!(path.segments, vec!["core", "fmt"]);
                }
                _ => panic!("expected path use tree"),
            },
            _ => panic!("expected use item"),
        }
    }

    fn parse_function_decl_from_source(source: &str) -> Spanned<FunctionDecl> {
        let mut parser = Parser::new(source).expect("parser");
        let function = parser.parse_function_decl().expect("function parse");
        assert!(parser.is_eof(), "expected eof after function parse");
        function
    }

    fn parse_init_decl_from_source(source: &str) -> Spanned<InitDecl> {
        let mut parser = Parser::new(source).expect("parser");
        let init = parser.parse_init_decl().expect("init parse");
        assert!(parser.is_eof(), "expected eof after init parse");
        init
    }

    fn parse_init_decl_from_source_result(
        source: &str,
    ) -> Result<Spanned<InitDecl>, ParseError> {
        let mut parser = Parser::new(source).expect("parser");
        let init = parser.parse_init_decl()?;
        if !parser.is_eof() {
            return Err(ParseError::UnexpectedToken {
                expected: "end of input",
                found: parser.peek().kind,
                span: parser.peek().span,
            });
        }
        Ok(init)
    }

    fn parse_extern_block_from_source(source: &str) -> Spanned<ExternBlock> {
        let mut parser = Parser::new(source).expect("parser");
        let block = parser.parse_extern_block().expect("extern parse");
        assert!(parser.is_eof(), "expected eof after extern parse");
        block
    }

    fn parse_struct_decl_from_source(source: &str) -> Spanned<StructDecl> {
        let mut parser = Parser::new(source).expect("parser");
        let decl = parser.parse_struct_decl().expect("struct parse");
        assert!(parser.is_eof(), "expected eof after struct parse");
        decl
    }

    fn parse_enum_decl_from_source(source: &str) -> Spanned<EnumDecl> {
        let mut parser = Parser::new(source).expect("parser");
        let decl = parser.parse_enum_decl().expect("enum parse");
        assert!(parser.is_eof(), "expected eof after enum parse");
        decl
    }

    fn parse_impl_decl_from_source(source: &str) -> Spanned<ImplDecl> {
        let mut parser = Parser::new(source).expect("parser");
        let decl = parser.parse_impl_decl().expect("impl parse");
        assert!(parser.is_eof(), "expected eof after impl parse");
        decl
    }

    fn parse_protocol_decl_from_source(source: &str) -> Spanned<ProtocolDecl> {
        let mut parser = Parser::new(source).expect("parser");
        let decl = parser.parse_protocol_decl().expect("protocol parse");
        assert!(parser.is_eof(), "expected eof after protocol parse");
        decl
    }

    fn parse_macro_decl_from_source(source: &str) -> Spanned<MacroDecl> {
        let mut parser = Parser::new(source).expect("parser");
        let decl = parser.parse_macro_decl().expect("macro parse");
        assert!(parser.is_eof(), "expected eof after macro parse");
        decl
    }

    #[test]
    fn parse_function_with_modifiers_and_return_type() {
        let function =
            parse_function_decl_from_source("pub async fn f(x: i32) -> i32 {}");
        assert!(matches!(function.node.visibility, Some(Visibility::Public)));
        assert_eq!(function.node.modifiers.len(), 1);
        assert_eq!(function.node.params.len(), 1);
        assert!(function.node.return_type.is_some());
    }

    #[test]
    fn parse_function_with_unsafe_modifier() {
        let function = parse_function_decl_from_source("unsafe fn f() {}");
        assert_eq!(function.node.modifiers.len(), 1);
        assert!(matches!(function.node.modifiers[0], Modifier::Unsafe));
    }

    #[test]
    fn parse_function_with_receiver() {
        let function =
            parse_function_decl_from_source("fn f(&self, x: i32) {}");
        assert!(matches!(
            function.node.receiver.map(|receiver| receiver.node),
            Some(ReceiverKind::Ref)
        ));
        assert_eq!(function.node.params.len(), 1);
    }

    #[test]
    fn parse_function_with_mut_receiver() {
        let function = parse_function_decl_from_source("fn f(&mut self) {}");
        assert!(matches!(
            function.node.receiver.map(|receiver| receiver.node),
            Some(ReceiverKind::MutRef)
        ));
        assert!(function.node.params.is_empty());
    }

    #[test]
    fn parse_function_body_consumes_then_parses_next_item() {
        let source = "fn f() { while x { break; } } use core::fmt;";
        let mut parser = Parser::new(source).expect("parser");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 2);
        assert!(matches!(file.items[0].node, Item::Function(_)));
        assert!(matches!(file.items[1].node, Item::Use(_)));
    }

    #[test]
    fn parse_plain_init_decl() {
        let init = parse_init_decl_from_source("init() {}");
        assert!(matches!(init.node.kind, InitKind::Plain));
    }

    #[test]
    fn parse_init_with_optional_syntax_errors() {
        // init? is now a syntax error
        let result = parse_init_decl_from_source_result("init?() {}");
        assert!(result.is_err());
    }

    #[test]
    fn parse_init_with_fallible_syntax_errors() {
        // init! is now a syntax error
        let result = parse_init_decl_from_source_result("init!() {}");
        assert!(result.is_err());
    }

    #[test]
    fn parse_init_with_unsafe_modifier() {
        let init = parse_init_decl_from_source("unsafe init() {}");
        assert_eq!(init.node.modifiers.len(), 1);
        assert!(matches!(init.node.modifiers[0], Modifier::Unsafe));
    }

    #[test]
    fn parse_minimal_extern_block() {
        let block = parse_extern_block_from_source(
            "extern libc { fn strlen(s: *void) -> usize; }",
        );
        assert_eq!(block.node.library_name, "libc");
        assert_eq!(block.node.members.len(), 1);
    }

    #[test]
    fn parse_extern_function_alias() {
        let block = parse_extern_block_from_source(
            "extern libc { fn pid = getpid() -> i32; }",
        );
        match &block.node.members[0].node {
            ExternMember::Function(function) => {
                assert_eq!(function.node.local_name, "pid");
                assert_eq!(
                    function.node.native_symbol.as_deref(),
                    Some("getpid")
                );
            }
        }
    }

    #[test]
    fn parse_extern_block_with_multiple_members() {
        let block = parse_extern_block_from_source(
            "extern libc { fn a() -> i32; fn b(x: i32) -> i32; }",
        );
        assert_eq!(block.node.members.len(), 2);
    }

    #[test]
    fn parse_function_with_attribute() {
        let function = parse_function_decl_from_source("@call(.C) fn f() {}");
        assert_eq!(function.node.attributes.len(), 1);
        assert_eq!(function.node.attributes[0].node.name, "call");
        match &function.node.attributes[0].node.args {
            AttributeArgs::Paren { raw } => assert_eq!(raw, ".C"),
            _ => panic!("expected paren attribute args"),
        }
    }

    #[test]
    fn parse_doc_comment_on_top_level_function_attached() {
        let mut parser = Parser::new("/// docs\nfn f() {}").expect("parser");
        let file = parser.parse_file().expect("parse file");
        match &file.items[0].node {
            Item::Function(function) => {
                assert_eq!(function.node.docs.len(), 1);
                assert_eq!(function.node.docs[0].node.text, "/// docs");
            }
            _ => panic!("expected function item"),
        }
    }

    #[test]
    fn parse_multiple_doc_comments_on_top_level_function_attached() {
        let mut parser =
            Parser::new("/// a\n/// b\nfn f() {}").expect("parser");
        let file = parser.parse_file().expect("parse file");
        match &file.items[0].node {
            Item::Function(function) => {
                assert_eq!(function.node.docs.len(), 2);
                assert_eq!(function.node.docs[0].node.text, "/// a");
                assert_eq!(function.node.docs[1].node.text, "/// b");
            }
            _ => panic!("expected function item"),
        }
    }

    #[test]
    fn parse_doc_comment_and_attributes_on_top_level_function() {
        let mut parser =
            Parser::new("/// docs\n@trace fn f() {}").expect("parser");
        let file = parser.parse_file().expect("parse file");
        match &file.items[0].node {
            Item::Function(function) => {
                assert_eq!(function.node.docs.len(), 1);
                assert_eq!(function.node.docs[0].node.text, "/// docs");
                assert_eq!(function.node.attributes.len(), 1);
                assert_eq!(function.node.attributes[0].node.name, "trace");
            }
            _ => panic!("expected function item"),
        }
    }

    #[test]
    fn parse_extern_function_with_attribute() {
        let block = parse_extern_block_from_source(
            "extern libc { @call(.C) fn strlen(s: *void) -> usize; }",
        );
        match &block.node.members[0].node {
            ExternMember::Function(function) => {
                assert_eq!(function.node.attributes.len(), 1);
                assert_eq!(function.node.attributes[0].node.name, "call");
            }
        }
    }

    #[test]
    fn parse_top_level_function_attribute_allowed() {
        let mut parser = Parser::new("@trace fn f() {}").expect("parser");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 1);
        match &file.items[0].node {
            Item::Function(function) => {
                assert_eq!(function.node.attributes.len(), 1);
                assert_eq!(function.node.attributes[0].node.name, "trace");
            }
            _ => panic!("expected function item"),
        }
    }

    #[test]
    fn parse_top_level_function_stacked_attributes_preserve_order() {
        let mut parser = Parser::new("@a @b fn f() {}").expect("parser");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 1);
        match &file.items[0].node {
            Item::Function(function) => {
                assert_eq!(function.node.attributes.len(), 2);
                assert_eq!(function.node.attributes[0].node.name, "a");
                assert_eq!(function.node.attributes[1].node.name, "b");
            }
            _ => panic!("expected function item"),
        }
    }

    #[test]
    fn parse_function_reports_error_on_missing_name() {
        let mut parser = Parser::new("fn () {}").expect("parser");
        let err = parser
            .parse_function_decl()
            .expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_extern_function_reports_error_on_missing_semicolon() {
        let mut parser =
            Parser::new("extern libc { fn strlen(s: *void) -> usize }")
                .expect("parser");
        let err = parser
            .parse_extern_block()
            .expect_err("expected parse error");
        assert!(matches!(
            err,
            ParseError::UnexpectedToken { .. }
                | ParseError::UnexpectedEof { .. }
        ));
    }

    #[test]
    fn parse_extern_block_reports_error_on_missing_rbrace() {
        let mut parser =
            Parser::new("extern libc { fn strlen(s: *void) -> usize;")
                .expect("parser");
        let err = parser
            .parse_extern_block()
            .expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }

    #[test]
    fn parse_file_with_use_then_function() {
        let mut parser =
            Parser::new("use core::fmt; fn f() {}").expect("parser");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 2);
        assert!(matches!(file.items[0].node, Item::Use(_)));
        assert!(matches!(file.items[1].node, Item::Function(_)));
    }

    #[test]
    fn parse_file_with_extern_then_use() {
        let mut parser = Parser::new(
            "extern libc { fn strlen(s: *void) -> usize; } use core::fmt;",
        )
        .expect("parser");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 2);
        assert!(matches!(file.items[0].node, Item::ExternBlock(_)));
        assert!(matches!(file.items[1].node, Item::Use(_)));
    }

    #[test]
    fn parse_file_with_macro_then_function() {
        let mut parser = Parser::new(
            "macro m { rule(input: Tokens) => { input }; } fn f() {}",
        )
        .expect("parser");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 2);
        assert!(matches!(file.items[0].node, Item::Macro(_)));
        assert!(matches!(file.items[1].node, Item::Function(_)));
    }

    #[test]
    fn parse_minimal_struct_decl() {
        let decl = parse_struct_decl_from_source("struct Foo {}");
        assert_eq!(decl.node.name, "Foo");
        assert!(decl.node.members.is_empty());
    }

    #[test]
    fn parse_struct_with_fields() {
        let decl =
            parse_struct_decl_from_source("struct Foo { x: i32, y: string }");
        assert_eq!(decl.node.members.len(), 2);
        assert!(matches!(decl.node.members[0].node, StructMember::Field(_)));
        assert!(matches!(decl.node.members[1].node, StructMember::Field(_)));
    }

    #[test]
    fn parse_attribute_on_struct_field_allowed() {
        let decl =
            parse_struct_decl_from_source("struct Foo { @trace x: i32, }");
        assert_eq!(decl.node.members.len(), 1);
        match &decl.node.members[0].node {
            StructMember::Field(field) => {
                assert_eq!(field.node.attributes.len(), 1);
                assert_eq!(field.node.attributes[0].node.name, "trace");
            }
            _ => panic!("expected struct field member"),
        }
    }

    #[test]
    fn parse_doc_comment_on_struct_field_attached() {
        let decl =
            parse_struct_decl_from_source("struct Foo { /// docs\n x: i32, }");
        assert_eq!(decl.node.members.len(), 1);
        match &decl.node.members[0].node {
            StructMember::Field(field) => {
                assert_eq!(field.node.docs.len(), 1);
                assert_eq!(field.node.docs[0].node.text, "/// docs");
            }
            _ => panic!("expected struct field member"),
        }
    }

    #[test]
    fn parse_multiple_attributes_on_struct_field_allowed() {
        let decl =
            parse_struct_decl_from_source("struct Foo { @a @b x: i32, }");
        assert_eq!(decl.node.members.len(), 1);
        match &decl.node.members[0].node {
            StructMember::Field(field) => {
                assert_eq!(field.node.attributes.len(), 2);
                assert_eq!(field.node.attributes[0].node.name, "a");
                assert_eq!(field.node.attributes[1].node.name, "b");
            }
            _ => panic!("expected struct field member"),
        }
    }

    #[test]
    fn parse_struct_with_init_and_method() {
        let decl = parse_struct_decl_from_source(
            "struct Foo { init() {} fn bar() {} }",
        );
        assert_eq!(decl.node.members.len(), 2);
        assert!(matches!(decl.node.members[0].node, StructMember::Init(_)));
        assert!(matches!(
            decl.node.members[1].node,
            StructMember::Function(_)
        ));
    }

    #[test]
    fn parse_minimal_enum_decl() {
        let decl = parse_enum_decl_from_source("enum Maybe { None }");
        assert_eq!(decl.node.members.len(), 1);
        assert!(matches!(decl.node.members[0].node, EnumMember::Case(_)));
    }

    #[test]
    fn parse_enum_with_tuple_cases() {
        let decl =
            parse_enum_decl_from_source("enum Maybe { None, Some(Foo) }");
        assert_eq!(decl.node.members.len(), 2);
    }

    #[test]
    fn parse_enum_with_methods() {
        let decl = parse_enum_decl_from_source(
            "enum Maybe { None, fn is_some(&self) {} }",
        );
        assert_eq!(decl.node.members.len(), 2);
        assert!(matches!(decl.node.members[0].node, EnumMember::Case(_)));
        assert!(matches!(decl.node.members[1].node, EnumMember::Function(_)));
    }

    #[test]
    fn parse_attribute_on_enum_case_allowed() {
        let decl = parse_enum_decl_from_source("enum E { @trace A, }");
        assert_eq!(decl.node.members.len(), 1);
        match &decl.node.members[0].node {
            EnumMember::Case(case_decl) => {
                assert_eq!(case_decl.node.attributes.len(), 1);
                assert_eq!(case_decl.node.attributes[0].node.name, "trace");
            }
            _ => panic!("expected enum case member"),
        }
    }

    #[test]
    fn parse_doc_comment_on_enum_case_attached() {
        let decl = parse_enum_decl_from_source("enum E { /// docs\n A, }");
        assert_eq!(decl.node.members.len(), 1);
        match &decl.node.members[0].node {
            EnumMember::Case(case_decl) => {
                assert_eq!(case_decl.node.docs.len(), 1);
                assert_eq!(case_decl.node.docs[0].node.text, "/// docs");
            }
            _ => panic!("expected enum case member"),
        }
    }

    #[test]
    fn parse_attribute_on_enum_case_with_payload_allowed() {
        let decl = parse_enum_decl_from_source("enum E { @tag Some(i32), }");
        assert_eq!(decl.node.members.len(), 1);
        match &decl.node.members[0].node {
            EnumMember::Case(case_decl) => {
                assert_eq!(case_decl.node.attributes.len(), 1);
                assert_eq!(case_decl.node.attributes[0].node.name, "tag");
                assert_eq!(case_decl.node.payload.len(), 1);
            }
            _ => panic!("expected enum case member"),
        }
    }

    #[test]
    fn parse_multiple_attributes_on_enum_case_allowed() {
        let decl = parse_enum_decl_from_source("enum E { @a @b A, }");
        assert_eq!(decl.node.members.len(), 1);
        match &decl.node.members[0].node {
            EnumMember::Case(case_decl) => {
                assert_eq!(case_decl.node.attributes.len(), 2);
                assert_eq!(case_decl.node.attributes[0].node.name, "a");
                assert_eq!(case_decl.node.attributes[1].node.name, "b");
            }
            _ => panic!("expected enum case member"),
        }
    }

    #[test]
    fn parse_minimal_impl_decl() {
        let decl = parse_impl_decl_from_source("impl Foo {}");
        assert!(decl.node.members.is_empty());
    }

    #[test]
    fn parse_impl_with_conformance() {
        let decl = parse_impl_decl_from_source("impl Display for Foo {}");
        assert!(decl.node.conformance.is_some());
    }

    #[test]
    fn parse_impl_with_unsafe_modifier() {
        let decl = parse_impl_decl_from_source("unsafe impl Foo {}");
        assert_eq!(decl.node.modifiers.len(), 1);
        assert!(matches!(decl.node.modifiers[0], Modifier::Unsafe));
    }

    #[test]
    fn parse_impl_with_init_and_method() {
        let decl =
            parse_impl_decl_from_source("impl Foo { init() {} fn bar() {} }");
        assert_eq!(decl.node.members.len(), 2);
        assert!(matches!(decl.node.members[0].node, ImplMember::Init(_)));
        assert!(matches!(decl.node.members[1].node, ImplMember::Function(_)));
    }

    #[test]
    fn parse_minimal_protocol_decl() {
        let decl = parse_protocol_decl_from_source("protocol Display {}");
        assert!(decl.node.members.is_empty());
    }

    #[test]
    fn parse_protocol_with_function_requirement() {
        let decl = parse_protocol_decl_from_source(
            "protocol Display { fn fmt(&self) -> string; }",
        );
        assert_eq!(decl.node.members.len(), 1);
        match &decl.node.members[0].node {
            ProtocolMember::Function(member) => {
                assert!(member.node.default_body.is_none());
            }
            _ => panic!("expected protocol function member"),
        }
    }

    #[test]
    fn parse_protocol_function_requirement_with_unsafe_modifier() {
        let decl =
            parse_protocol_decl_from_source("protocol P { unsafe fn f(); }");
        match &decl.node.members[0].node {
            ProtocolMember::Function(member) => {
                assert_eq!(member.node.modifiers.len(), 1);
                assert!(matches!(member.node.modifiers[0], Modifier::Unsafe));
            }
            _ => panic!("expected protocol function member"),
        }
    }

    #[test]
    fn parse_protocol_with_function_default_impl() {
        let decl = parse_protocol_decl_from_source(
            "protocol Display { fn fmt(&self) -> string {} }",
        );
        match &decl.node.members[0].node {
            ProtocolMember::Function(member) => {
                assert!(member.node.default_body.is_some());
            }
            _ => panic!("expected protocol function member"),
        }
    }

    #[test]
    fn parse_protocol_member_with_attribute() {
        let decl = parse_protocol_decl_from_source(
            "protocol P { @trace fn f() -> i32; }",
        );
        match &decl.node.members[0].node {
            ProtocolMember::Function(function) => {
                assert_eq!(function.node.attributes.len(), 1);
                assert_eq!(function.node.attributes[0].node.name, "trace");
            }
            _ => panic!("expected protocol function member"),
        }
    }

    #[test]
    fn parse_doc_comment_on_protocol_member_attached() {
        let decl = parse_protocol_decl_from_source(
            "protocol P { /// docs\n fn f() -> i32; }",
        );
        match &decl.node.members[0].node {
            ProtocolMember::Function(function) => {
                assert_eq!(function.node.docs.len(), 1);
                assert_eq!(function.node.docs[0].node.text, "/// docs");
            }
            _ => panic!("expected protocol function member"),
        }
    }

    #[test]
    fn parse_protocol_member_stacked_attributes_preserve_order() {
        let decl = parse_protocol_decl_from_source(
            "protocol P { @a @b fn f() -> i32; }",
        );
        match &decl.node.members[0].node {
            ProtocolMember::Function(function) => {
                assert_eq!(function.node.attributes.len(), 2);
                assert_eq!(function.node.attributes[0].node.name, "a");
                assert_eq!(function.node.attributes[1].node.name, "b");
            }
            _ => panic!("expected protocol function member"),
        }
    }

    #[test]
    fn parse_protocol_with_init_requirement() {
        let decl =
            parse_protocol_decl_from_source("protocol P { init(_ x: i32); }");
        match &decl.node.members[0].node {
            ProtocolMember::Initializer(member) => {
                assert!(member.node.default_body.is_none());
            }
            _ => panic!("expected protocol init member"),
        }
    }

    #[test]
    fn parse_protocol_with_associated_type() {
        let decl =
            parse_protocol_decl_from_source("protocol P { type Output; }");
        assert!(matches!(
            decl.node.members[0].node,
            ProtocolMember::AssociatedType(_)
        ));
    }

    #[test]
    fn parse_protocol_with_property_requirement() {
        let decl = parse_protocol_decl_from_source(
            "protocol P { var name: string { get set } }",
        );
        match &decl.node.members[0].node {
            ProtocolMember::Property(property) => {
                assert_eq!(
                    property.node.accessors,
                    vec![AccessorRequirement::Get, AccessorRequirement::Set]
                );
            }
            _ => panic!("expected protocol property member"),
        }
    }

    #[test]
    fn parse_macro_decl_with_rule_clause() {
        let decl = parse_macro_decl_from_source(
            "macro unless { rule(cond: Expr, body: Block) => { if !cond body }; }",
        );
        assert_eq!(decl.node.name, "unless");
        assert_eq!(decl.node.clauses.len(), 1);
        let clause = &decl.node.clauses[0].node;
        assert!(matches!(clause.kind, MacroClauseKind::Rule));
        assert_eq!(clause.params.len(), 2);
        assert!(matches!(clause.params[0].node.kind, MacroInputKind::Expr));
        assert!(matches!(clause.params[1].node.kind, MacroInputKind::Block));
        assert_eq!(
            clause
                .body
                .tokens
                .iter()
                .map(|token| token.kind)
                .collect::<Vec<_>>(),
            vec![
                TokenKind::KwIf,
                TokenKind::Bang,
                TokenKind::Ident,
                TokenKind::Ident,
            ]
        );
    }

    #[test]
    fn parse_macro_decl_with_reflect_item_and_args_clause() {
        let decl = parse_macro_decl_from_source(
            "macro derive { reflect(item: Item, args: MacroArgs) => { item }; }",
        );
        assert_eq!(decl.node.name, "derive");
        assert_eq!(decl.node.clauses.len(), 1);
        let clause = &decl.node.clauses[0].node;
        assert!(matches!(clause.kind, MacroClauseKind::Reflect));
        assert_eq!(clause.params.len(), 2);
        assert!(matches!(clause.params[0].node.kind, MacroInputKind::Item));
        assert!(matches!(
            clause.params[1].node.kind,
            MacroInputKind::MacroArgs
        ));
    }

    #[test]
    fn parse_macro_decl_reports_error_on_invalid_input_kind() {
        let mut parser =
            Parser::new("macro m { rule(x: UnknownKind) => { x }; }")
                .expect("parser");
        let err = parser.parse_macro_decl().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_file_with_struct_enum_impl_protocol_sequence() {
        let source = "struct S {} enum E { A } impl S {} protocol P {}";
        let mut parser = Parser::new(source).expect("parser");
        let file = parser.parse_file().expect("parse file");
        assert_eq!(file.items.len(), 4);
        assert!(matches!(file.items[0].node, Item::Struct(_)));
        assert!(matches!(file.items[1].node, Item::Enum(_)));
        assert!(matches!(file.items[2].node, Item::Impl(_)));
        assert!(matches!(file.items[3].node, Item::Protocol(_)));
    }

    #[test]
    fn parse_struct_reports_error_on_missing_rbrace() {
        let mut parser = Parser::new("struct Foo { x: i32").expect("parser");
        let err = parser
            .parse_struct_decl()
            .expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }

    #[test]
    fn parse_struct_dangling_attributes_report_clear_error() {
        let mut parser = Parser::new("struct Foo { @a @b }").expect("parser");
        let err = parser
            .parse_struct_decl()
            .expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { expected, .. } => {
                assert!(expected.contains("expected after attributes"));
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_enum_reports_error_on_missing_case_separator_or_rbrace() {
        let mut parser = Parser::new("enum E { A B }").expect("parser");
        let err = parser.parse_enum_decl().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_enum_dangling_attributes_report_clear_error() {
        let mut parser = Parser::new("enum E { @a }").expect("parser");
        let err = parser.parse_enum_decl().expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { expected, .. } => {
                assert!(expected.contains("expected after attributes"));
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_protocol_reports_error_on_bad_property_accessor_block() {
        let mut parser = Parser::new("protocol P { var name: string { foo } }")
            .expect("parser");
        let err = parser
            .parse_protocol_decl()
            .expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_impl_reports_error_on_bad_member_start() {
        let mut parser = Parser::new("impl Foo { x: i32 }").expect("parser");
        let err = parser.parse_impl_decl().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    fn parse_pattern_from_source(source: &str) -> Spanned<Pattern> {
        let mut parser = Parser::new(source).expect("parser");
        let pattern = parser.parse_pattern().expect("pattern parse");
        assert!(parser.is_eof(), "expected eof after pattern parse");
        pattern
    }

    fn parse_pattern_with_parser(source: &str) -> Parser<'_> {
        Parser::new(source).expect("parser")
    }

    #[test]
    fn parse_identifier_pattern() {
        let pattern = parse_pattern_from_source("foo");
        assert!(matches!(pattern.node, Pattern::Identifier(_)));
    }

    #[test]
    fn parse_wildcard_pattern() {
        let pattern = parse_pattern_from_source("_");
        assert!(matches!(pattern.node, Pattern::Wildcard));
    }

    #[test]
    fn parse_pattern_preserves_identifier_text() {
        let pattern = parse_pattern_from_source("foo_bar123");
        match pattern.node {
            Pattern::Identifier(name) => assert_eq!(name, "foo_bar123"),
            _ => panic!("expected identifier pattern"),
        }
    }

    #[test]
    fn parse_pattern_consumes_exactly_one_pattern() {
        let mut parser = parse_pattern_with_parser("foo bar");
        let first = parser.parse_pattern().expect("first pattern");
        assert!(matches!(first.node, Pattern::Identifier(_)));
        assert_eq!(parser.peek().kind, TokenKind::Ident);
        assert_eq!(parser.slice(parser.peek().span), "bar");
    }

    #[test]
    fn parse_pattern_reports_unexpected_token_for_non_pattern_start() {
        let mut parser = parse_pattern_with_parser("fn");
        let err = parser.parse_pattern().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));

        let mut parser = parse_pattern_with_parser("@");
        let err = parser.parse_pattern().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_pattern_literal_forms() {
        assert!(matches!(
            parse_pattern_from_source("42").node,
            Pattern::IntegerLiteral(_)
        ));
        assert!(matches!(
            parse_pattern_from_source("true").node,
            Pattern::BooleanLiteral(true)
        ));
        assert!(matches!(
            parse_pattern_from_source("'a'").node,
            Pattern::CharLiteral(_)
        ));
        assert!(matches!(
            parse_pattern_from_source("\"x\"").node,
            Pattern::StringLiteral(_)
        ));
    }

    #[test]
    fn parse_tuple_pattern() {
        let pattern = parse_pattern_from_source("(x, y)");
        assert!(matches!(pattern.node, Pattern::Tuple(_)));
    }

    #[test]
    fn parse_tuple_pattern_with_trailing_comma() {
        let pattern = parse_pattern_from_source("(x, y,)");
        assert!(matches!(pattern.node, Pattern::Tuple(_)));
    }

    #[test]
    fn parse_single_parenthesized_pattern_without_comma_fails() {
        let mut parser = parse_pattern_with_parser("(x)");
        let err = parser.parse_pattern().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_variant_patterns() {
        let named = parse_pattern_from_source("Some(x)");
        assert!(matches!(
            named.node,
            Pattern::Variant {
                shorthand: false,
                ..
            }
        ));

        let shorthand = parse_pattern_from_source(".some_variant(..)");
        assert!(matches!(
            shorthand.node,
            Pattern::Variant {
                shorthand: true,
                has_rest: true,
                ..
            }
        ));

        let plain = parse_pattern_from_source(".none");
        assert!(matches!(
            plain.node,
            Pattern::Variant {
                shorthand: true,
                ..
            }
        ));
    }

    #[test]
    fn parse_struct_patterns() {
        let pattern = parse_pattern_from_source("Point { x, y }");
        assert!(matches!(pattern.node, Pattern::Struct { .. }));

        let pattern = parse_pattern_from_source("Point { x: _, y }");
        match pattern.node {
            Pattern::Struct { fields, .. } => assert_eq!(fields.len(), 2),
            _ => panic!("expected struct pattern"),
        }
    }

    #[test]
    fn parse_array_patterns_with_rest() {
        let bind_rest = parse_pattern_from_source("[a, b, ..rest]");
        match bind_rest.node {
            Pattern::Array { rest, .. } => {
                assert!(matches!(rest, Some(ArrayPatternRest::Bind(_))));
            }
            _ => panic!("expected array pattern"),
        }

        let ignore_rest = parse_pattern_from_source("[1, 2, ..]");
        match ignore_rest.node {
            Pattern::Array { rest, .. } => {
                assert!(matches!(rest, Some(ArrayPatternRest::Ignore)));
            }
            _ => panic!("expected array pattern"),
        }
    }

    #[test]
    fn parse_array_pattern_rest_must_be_final() {
        let mut parser = parse_pattern_with_parser("[a, ..rest, b]");
        let err = parser.parse_pattern().expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { expected, .. } => {
                assert!(expected.contains("must be final"));
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_array_pattern_multiple_rest_rejected() {
        let mut parser = parse_pattern_with_parser("[..a, ..b]");
        let err = parser.parse_pattern().expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { expected, .. } => {
                assert!(
                    expected.contains("at most one")
                        || expected.contains("must be final"),
                    "unexpected expected message: {expected}"
                );
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_variant_pattern_rest_must_be_final() {
        let mut parser = parse_pattern_with_parser(".some(.., x)");
        let err = parser.parse_pattern().expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { expected, .. } => {
                assert!(expected.contains("must be final"));
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_variant_pattern_multiple_rest_rejected() {
        let mut parser = parse_pattern_with_parser(".some(.., ..)");
        let err = parser.parse_pattern().expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { expected, .. } => {
                assert!(expected.contains("must be final"));
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_struct_pattern_rest_must_be_final() {
        let mut parser = parse_pattern_with_parser("Point { .., x }");
        let err = parser.parse_pattern().expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { expected, .. } => {
                assert!(expected.contains("must be final"));
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_struct_pattern_multiple_rest_rejected() {
        let mut parser = parse_pattern_with_parser("Point { .., .. }");
        let err = parser.parse_pattern().expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { expected, .. } => {
                assert!(
                    expected.contains("at most one")
                        || expected.contains("must be final"),
                    "unexpected expected message: {expected}"
                );
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_pattern_reports_unexpected_eof() {
        let mut parser = parse_pattern_with_parser("");
        let err = parser.parse_pattern().expect_err("expected eof");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }

    #[test]
    fn parse_normal_comment_not_attached_as_doc() {
        let mut parser = Parser::new("// comment\nfn f() {}").expect("parser");
        let file = parser.parse_file().expect("parse file");
        match &file.items[0].node {
            Item::Function(function) => {
                assert!(function.node.docs.is_empty());
            }
            _ => panic!("expected function item"),
        }
    }

    fn parse_expr_from_source(source: &str) -> Spanned<Expr> {
        let mut parser = Parser::new(source).expect("parser");
        let expr = parser.parse_expr().expect("expr parse");
        assert!(parser.is_eof(), "expected eof after expr parse");
        expr
    }

    fn parse_expr_with_parser(source: &str) -> Parser<'_> {
        Parser::new(source).expect("parser")
    }

    fn parse_block_from_source(source: &str) -> ast::Block {
        let mut parser = Parser::new(source).expect("parser");
        let block = parser.parse_block().expect("block parse");
        assert!(parser.is_eof(), "expected eof after block parse");
        block
    }

    fn parse_stmt_from_source(source: &str) -> Spanned<ast::Stmt> {
        let mut parser = Parser::new(source).expect("parser");
        let stmt = parser.parse_stmt().expect("stmt parse");
        assert!(parser.is_eof(), "expected eof after stmt parse");
        stmt
    }

    fn parse_clause_list_from_source(source: &str) -> ast::ClauseList {
        let mut parser = Parser::new(source).expect("parser");
        let clauses = parser.parse_clause_list().expect("clause-list parse");
        assert!(parser.is_eof(), "expected eof after clause-list parse");
        clauses
    }

    #[test]
    fn parse_empty_block() {
        let block = parse_block_from_source("{}");
        assert!(block.statements.is_empty());
        assert!(block.tail_expr.is_none());
    }

    #[test]
    fn parse_block_with_only_statements() {
        let block = parse_block_from_source("{ let x = y; foo(); }");
        assert_eq!(block.statements.len(), 2);
        assert!(block.tail_expr.is_none());
    }

    #[test]
    fn parse_block_with_tail_expr() {
        let block = parse_block_from_source("{ let x = y; x }");
        assert_eq!(block.statements.len(), 1);
        assert!(block.tail_expr.is_some());
    }

    #[test]
    fn parse_block_with_expr_statement_not_tail() {
        let block = parse_block_from_source("{ x; }");
        assert_eq!(block.statements.len(), 1);
        assert!(block.tail_expr.is_none());
        assert!(matches!(
            block.statements[0].node,
            ast::Stmt::Expr { has_semi: true, .. }
        ));
    }

    #[test]
    fn parse_block_reports_error_on_missing_rbrace() {
        let mut parser = Parser::new("{ let x = y;").expect("parser");
        let err = parser.parse_block().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }

    #[test]
    fn parse_let_stmt_with_type_and_value() {
        let stmt = parse_stmt_from_source("let x: i32 = y;");
        match stmt.node {
            ast::Stmt::Let(let_stmt) => {
                assert!(let_stmt.node.ty.is_some());
                assert!(let_stmt.node.value.is_some());
            }
            _ => panic!("expected let stmt"),
        }
    }

    #[test]
    fn parse_let_stmt_with_value_only() {
        let stmt = parse_stmt_from_source("let x = y;");
        match stmt.node {
            ast::Stmt::Let(let_stmt) => {
                assert!(let_stmt.node.ty.is_none());
                assert!(let_stmt.node.value.is_some());
            }
            _ => panic!("expected let stmt"),
        }
    }

    #[test]
    fn parse_let_stmt_without_initializer() {
        let stmt = parse_stmt_from_source("let x;");
        match stmt.node {
            ast::Stmt::Let(let_stmt) => {
                assert!(let_stmt.node.ty.is_none());
                assert!(let_stmt.node.value.is_none());
            }
            _ => panic!("expected let stmt"),
        }
    }

    #[test]
    fn parse_let_stmt_typed_without_initializer() {
        let stmt = parse_stmt_from_source("let x: i32;");
        match stmt.node {
            ast::Stmt::Let(let_stmt) => {
                assert!(let_stmt.node.ty.is_some());
                assert!(let_stmt.node.value.is_none());
            }
            _ => panic!("expected let stmt"),
        }
    }

    #[test]
    fn parse_var_stmt_without_initializer() {
        let stmt = parse_stmt_from_source("var x;");
        match stmt.node {
            ast::Stmt::Var(var_stmt) => {
                assert!(var_stmt.node.ty.is_none());
                assert!(var_stmt.node.value.is_none());
            }
            _ => panic!("expected var stmt"),
        }
    }

    #[test]
    fn parse_var_stmt_typed_without_initializer() {
        let stmt = parse_stmt_from_source("var x: i32;");
        match stmt.node {
            ast::Stmt::Var(var_stmt) => {
                assert!(var_stmt.node.ty.is_some());
                assert!(var_stmt.node.value.is_none());
            }
            _ => panic!("expected var stmt"),
        }
    }

    #[test]
    fn parse_var_stmt_with_value_only() {
        let stmt = parse_stmt_from_source("var x = y;");
        match stmt.node {
            ast::Stmt::Var(var_stmt) => {
                assert!(var_stmt.node.ty.is_none());
                assert!(var_stmt.node.value.is_some());
            }
            _ => panic!("expected var stmt"),
        }
    }

    #[test]
    fn parse_return_stmt_empty() {
        let stmt = parse_stmt_from_source("return;");
        assert!(matches!(stmt.node, ast::Stmt::Return(None)));
    }

    #[test]
    fn parse_return_stmt_with_value() {
        let stmt = parse_stmt_from_source("return x;");
        match stmt.node {
            ast::Stmt::Return(Some(value)) => {
                assert!(matches!(value.node, Expr::Identifier(_)));
            }
            _ => panic!("expected return with value"),
        }
    }

    #[test]
    fn parse_break_stmt() {
        let stmt = parse_stmt_from_source("break;");
        assert!(matches!(stmt.node, ast::Stmt::Break));
    }

    #[test]
    fn parse_continue_stmt() {
        let stmt = parse_stmt_from_source("continue;");
        assert!(matches!(stmt.node, ast::Stmt::Continue));
    }

    #[test]
    fn parse_clause_expr() {
        let clauses = parse_clause_list_from_source("x == y");
        assert_eq!(clauses.clauses.len(), 1);
        assert!(matches!(clauses.clauses[0].node, ast::Clause::Expr(_)));
    }

    #[test]
    fn parse_clause_let_binding() {
        let clauses = parse_clause_list_from_source("let x = y");
        assert_eq!(clauses.clauses.len(), 1);
        assert!(matches!(
            clauses.clauses[0].node,
            ast::Clause::LetBinding(_)
        ));
    }

    #[test]
    fn parse_clause_var_binding_with_type() {
        let clauses = parse_clause_list_from_source("var x: i32 = y");
        assert_eq!(clauses.clauses.len(), 1);
        match &clauses.clauses[0].node {
            ast::Clause::VarBinding(binding) => {
                assert!(binding.ty.is_some());
            }
            _ => panic!("expected var binding clause"),
        }
    }

    #[test]
    fn parse_guard_stmt_with_multiple_clauses() {
        let stmt = parse_stmt_from_source(
            "guard let x = a; let y = b; x == y else { return; }",
        );
        match stmt.node {
            ast::Stmt::Guard(guard_stmt) => {
                assert_eq!(guard_stmt.node.clauses.clauses.len(), 3);
                assert_eq!(guard_stmt.node.else_block.statements.len(), 1);
            }
            _ => panic!("expected guard stmt"),
        }
    }

    #[test]
    fn parse_while_stmt_with_clause_list() {
        let stmt =
            parse_stmt_from_source("while let x = next(); x < y { break; }");
        match stmt.node {
            ast::Stmt::While(while_stmt) => {
                assert_eq!(while_stmt.node.clauses.clauses.len(), 2);
                assert_eq!(while_stmt.node.body.statements.len(), 1);
            }
            _ => panic!("expected while stmt"),
        }
    }

    #[test]
    fn parse_for_stmt() {
        let stmt = parse_stmt_from_source("for x in xs { continue; }");
        match stmt.node {
            ast::Stmt::For(for_stmt) => {
                assert_eq!(for_stmt.node.body.statements.len(), 1);
            }
            _ => panic!("expected for stmt"),
        }
    }

    #[test]
    fn parse_function_body_now_uses_real_block() {
        let function =
            parse_function_decl_from_source("fn f() { let x = y; x }");
        assert_eq!(function.node.body.statements.len(), 1);
        assert!(function.node.body.tail_expr.is_some());
    }

    #[test]
    fn parse_protocol_default_impl_now_uses_real_block() {
        let decl = parse_protocol_decl_from_source(
            "protocol P { fn f() { let x = y; x } }",
        );
        match &decl.node.members[0].node {
            ProtocolMember::Function(member) => {
                let body = member
                    .node
                    .default_body
                    .as_ref()
                    .expect("expected default body");
                assert_eq!(body.statements.len(), 1);
                assert!(body.tail_expr.is_some());
            }
            _ => panic!("expected protocol function member"),
        }
    }

    #[test]
    fn parse_macro_braced_expr_captures_raw_block() {
        let expr = parse_expr_from_source("@build { let x = y; x }");
        match expr.node {
            Expr::Macro { args, .. } => match args {
                MacroExprArgs::Braced(block) => {
                    assert_eq!(
                        block
                            .tokens
                            .iter()
                            .map(|token| token.kind)
                            .collect::<Vec<_>>(),
                        vec![
                            TokenKind::KwLet,
                            TokenKind::Ident,
                            TokenKind::Eq,
                            TokenKind::Ident,
                            TokenKind::Semi,
                            TokenKind::Ident,
                        ]
                    );
                }
                MacroExprArgs::Paren(_) => panic!("expected braced macro args"),
            },
            _ => panic!("expected macro expr"),
        }
    }

    #[test]
    fn parse_stmt_reports_error_on_missing_semicolon_for_let() {
        let mut parser = Parser::new("let x = y").expect("parser");
        let err = parser.parse_stmt().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }

    #[test]
    fn parse_clause_binding_still_requires_initializer() {
        let mut parser =
            Parser::new("guard let x else { return; }").expect("parser");
        let err = parser.parse_stmt().expect_err("expected parse error");
        assert!(matches!(
            err,
            ParseError::UnexpectedToken { .. }
                | ParseError::UnexpectedEof { .. }
        ));
    }

    #[test]
    fn parse_attribute_on_statement_rejected() {
        let mut parser =
            Parser::new("fn f() { @trace let x = y; }").expect("parser");
        let err = parser.parse_file().expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { expected, .. } => {
                assert!(
                    expected.contains(
                        "attributes are only allowed on declarations"
                    )
                );
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_stmt_reports_error_on_missing_else_in_guard() {
        let mut parser =
            Parser::new("guard x == y { return; }").expect("parser");
        let err = parser.parse_stmt().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_stmt_reports_error_on_bad_for_syntax() {
        let mut parser =
            Parser::new("for in xs { continue; }").expect("parser");
        let err = parser.parse_stmt().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_if_stmt_without_else() {
        let stmt = parse_stmt_from_source("if x { foo(); }");
        match stmt.node {
            ast::Stmt::If(if_stmt) => {
                assert!(if_stmt.node.else_branch.is_none());
            }
            _ => panic!("expected if stmt"),
        }
    }

    #[test]
    fn parse_if_stmt_with_else_if() {
        let stmt = parse_stmt_from_source(
            "if a { foo(); } else if b { bar(); } else { baz(); }",
        );
        match stmt.node {
            ast::Stmt::If(if_stmt) => {
                assert!(if_stmt.node.else_branch.is_some());
            }
            _ => panic!("expected if stmt"),
        }
    }

    #[test]
    fn parse_identifier_expr() {
        let expr = parse_expr_from_source("foo");
        assert!(matches!(expr.node, Expr::Identifier(_)));
    }

    #[test]
    fn parse_integer_literal_expr() {
        let expr = parse_expr_from_source("123");
        assert!(matches!(expr.node, Expr::IntegerLiteral(_)));
    }

    #[test]
    fn parse_float_literal_expr() {
        let expr = parse_expr_from_source("1.25");
        assert!(matches!(expr.node, Expr::FloatLiteral(_)));
    }

    #[test]
    fn parse_boolean_literal_expr() {
        let expr = parse_expr_from_source("true");
        assert!(matches!(expr.node, Expr::BooleanLiteral(true)));
    }

    #[test]
    fn parse_char_literal_expr() {
        let expr = parse_expr_from_source("'a'");
        assert!(matches!(expr.node, Expr::CharLiteral(_)));
    }

    #[test]
    fn parse_string_literal_expr() {
        let expr = parse_expr_from_source("\"abc\"");
        match expr.node {
            Expr::StringLiteral(literal) => {
                assert_eq!(literal.parts.len(), 1);
            }
            _ => panic!("expected string literal"),
        }
    }

    #[test]
    fn parse_string_literal_with_interpolation() {
        let expr = parse_expr_from_source("\"a\\(x)b\"");
        match expr.node {
            Expr::StringLiteral(literal) => {
                assert_eq!(literal.parts.len(), 3);
                assert!(matches!(literal.parts[0], StringPart::Text(_)));
                assert!(matches!(
                    literal.parts[1],
                    StringPart::Interpolation(_)
                ));
                assert!(matches!(literal.parts[2], StringPart::Text(_)));
            }
            _ => panic!("expected string literal"),
        }
    }

    #[test]
    fn parse_self_expr() {
        let expr = parse_expr_from_source("self");
        assert!(matches!(expr.node, Expr::SelfValue));
    }

    #[test]
    fn parse_self_type_expr() {
        let expr = parse_expr_from_source("Self");
        assert!(matches!(expr.node, Expr::SelfType));
    }

    #[test]
    fn parse_grouped_expr() {
        let expr = parse_expr_from_source("(foo)");
        assert!(matches!(expr.node, Expr::Grouped(_)));
    }

    #[test]
    fn parse_array_literal_empty() {
        let expr = parse_expr_from_source("[]");
        match expr.node {
            Expr::ArrayLiteral(elements) => assert!(elements.is_empty()),
            _ => panic!("expected array literal"),
        }
    }

    #[test]
    fn parse_array_literal_multiple() {
        let expr = parse_expr_from_source("[a, b, c]");
        match expr.node {
            Expr::ArrayLiteral(elements) => assert_eq!(elements.len(), 3),
            _ => panic!("expected array literal"),
        }
    }

    #[test]
    fn parse_shorthand_member_expr() {
        let expr = parse_expr_from_source(".some");
        assert!(matches!(expr.node, Expr::ShorthandMember { .. }));
    }

    #[test]
    fn parse_macro_expr_paren() {
        let expr = parse_expr_from_source("@call(x)");
        match expr.node {
            Expr::Macro { args, .. } => match args {
                MacroExprArgs::Paren(values) => assert_eq!(values.len(), 1),
                MacroExprArgs::Braced(_) => panic!("expected paren macro args"),
            },
            _ => panic!("expected macro expr"),
        }
    }

    #[test]
    fn parse_macro_expr_braced() {
        let expr = parse_expr_from_source("@build {}");
        match expr.node {
            Expr::Macro { args, .. } => match args {
                MacroExprArgs::Braced(_) => {}
                MacroExprArgs::Paren(_) => panic!("expected braced macro args"),
            },
            _ => panic!("expected macro expr"),
        }
    }

    #[test]
    fn parse_struct_literal_expr() {
        let expr = parse_expr_from_source("Foo { x: a, y }");
        match expr.node {
            Expr::StructLiteral { fields, .. } => assert_eq!(fields.len(), 2),
            _ => panic!("expected struct literal"),
        }
    }

    #[test]
    fn lowercase_identifier_with_braces_is_not_misparsed_as_struct_literal() {
        let mut parser = parse_expr_with_parser("foo { x: y }");
        let expr = parser.parse_expr().expect("expr parse");
        assert!(matches!(expr.node, Expr::Identifier(_)));
        assert!(parser.at(TokenKind::LBrace));
    }

    #[test]
    fn parse_member_access_expr() {
        let expr = parse_expr_from_source("foo.bar");
        assert!(matches!(expr.node, Expr::MemberAccess { .. }));
    }

    #[test]
    fn parse_namespace_access_expr() {
        let expr = parse_expr_from_source("Foo::bar");
        assert!(matches!(expr.node, Expr::NamespaceAccess { .. }));
    }

    #[test]
    fn parse_namespace_access_init_keyword_expr() {
        let expr = parse_expr_from_source("Point::init");
        match expr.node {
            Expr::NamespaceAccess { member, .. } => assert_eq!(member, "init"),
            _ => panic!("expected namespace access expr"),
        }
    }

    #[test]
    fn parse_call_expr() {
        let expr = parse_expr_from_source("foo(x, y)");
        match expr.node {
            Expr::Call { args, .. } => assert_eq!(args.len(), 2),
            _ => panic!("expected call expr"),
        }
    }

    #[test]
    fn parse_call_expr_with_labeled_arg() {
        let expr = parse_expr_from_source("foo(name: x)");
        match expr.node {
            Expr::Call { args, .. } => {
                assert_eq!(args.len(), 1);
                assert_eq!(args[0].label.as_deref(), Some("name"));
            }
            _ => panic!("expected call expr"),
        }
    }

    #[test]
    fn parse_index_expr() {
        let expr = parse_expr_from_source("xs[i]");
        assert!(matches!(expr.node, Expr::Index { .. }));
    }

    #[test]
    fn parse_chained_postfix_expr() {
        let expr = parse_expr_from_source("foo.bar(x)[i]");
        assert!(matches!(expr.node, Expr::Index { .. }));
    }

    #[test]
    fn parse_struct_literal_then_postfix_if_supported_or_rejected_cleanly() {
        let expr = parse_expr_from_source("Foo { x: a }.y");
        assert!(matches!(expr.node, Expr::MemberAccess { .. }));
    }

    #[test]
    fn parse_expr_reports_unexpected_token_for_bad_start() {
        let mut parser = parse_expr_with_parser("}");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));

        let mut parser = parse_expr_with_parser(",");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_string_literal_reports_error_on_missing_interpolation_end_or_string_end()
     {
        let mut parser = parse_expr_with_parser("\"a\\(x y)b\"");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_malformed_ternary_reports_error() {
        let mut parser = parse_expr_with_parser("a ? b");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(
            err,
            ParseError::UnexpectedToken { .. }
                | ParseError::UnexpectedEof { .. }
        ));
    }

    #[test]
    fn parse_malformed_optional_chaining_reports_error() {
        let mut parser = parse_expr_with_parser("a?.");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(
            err,
            ParseError::UnexpectedToken { .. }
                | ParseError::UnexpectedEof { .. }
        ));
    }

    #[test]
    fn parse_malformed_cast_reports_error() {
        let mut parser = parse_expr_with_parser("x as");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(
            err,
            ParseError::UnexpectedToken { .. }
                | ParseError::UnexpectedEof { .. }
        ));
    }

    #[test]
    fn parse_if_expr_basic() {
        let expr = parse_expr_from_source("if x == y { a } else { b }");
        match expr.node {
            Expr::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                assert!(then_branch.tail_expr.is_some());
                assert!(matches!(else_branch.node, Expr::Block(_)));
            }
            _ => panic!("expected if expr"),
        }
    }

    #[test]
    fn parse_if_expr_with_clause_list() {
        let expr =
            parse_expr_from_source("if let x = a; x == y { b } else { c }");
        match expr.node {
            Expr::If { clauses, .. } => assert_eq!(clauses.clauses.len(), 2),
            _ => panic!("expected if expr"),
        }
    }

    #[test]
    fn parse_if_expr_with_else_if() {
        let expr =
            parse_expr_from_source("if x { a } else if y { b } else { c }");
        match expr.node {
            Expr::If {
                else_branch: Some(else_branch),
                ..
            } => {
                assert!(matches!(else_branch.node, Expr::If { .. }));
            }
            _ => panic!("expected if expr with else-if"),
        }
    }

    #[test]
    fn parse_if_else_block_branch_is_expr_block_not_closure() {
        let expr = parse_expr_from_source("if x { a } else { b }");
        match expr.node {
            Expr::If {
                else_branch: Some(else_branch),
                ..
            } => {
                assert!(matches!(else_branch.node, Expr::Block(_)));
            }
            _ => panic!("expected if expr"),
        }
    }

    #[test]
    fn parse_if_expr_reports_error_without_else() {
        let mut parser = parse_expr_with_parser("if x == y { a }");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(
            err,
            ParseError::UnexpectedEof { .. }
                | ParseError::UnexpectedToken { .. }
        ));
    }

    #[test]
    fn parse_match_expr_basic() {
        let expr = parse_expr_from_source("match x { foo => a, _ => b }");
        match expr.node {
            Expr::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert!(matches!(arms[0].node.body, MatchArmBody::Expr(_)));
            }
            _ => panic!("expected match expr"),
        }
    }

    #[test]
    fn parse_match_expr_with_block_arm() {
        let expr =
            parse_expr_from_source("match x { foo => { bar(); baz }, _ => b }");
        match expr.node {
            Expr::Match { arms, .. } => {
                assert!(matches!(arms[0].node.body, MatchArmBody::Block(_)));
            }
            _ => panic!("expected match expr"),
        }
    }

    #[test]
    fn parse_match_expr_allows_trailing_comma() {
        let expr = parse_expr_from_source("match x { foo => a, _ => b, }");
        match expr.node {
            Expr::Match { arms, .. } => assert_eq!(arms.len(), 2),
            _ => panic!("expected match expr"),
        }
    }

    #[test]
    fn parse_match_expr_reports_error_on_missing_fat_arrow() {
        let mut parser = parse_expr_with_parser("match x { foo a }");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_closure_expr_explicit_single_param() {
        let expr = parse_expr_from_source("{ x in x }");
        match expr.node {
            Expr::Closure {
                params,
                uses_shorthand_params,
                ..
            } => {
                assert_eq!(params.len(), 1);
                assert!(!uses_shorthand_params);
            }
            _ => panic!("expected closure expr"),
        }
    }

    #[test]
    fn parse_closure_expr_explicit_typed_param() {
        let expr = parse_expr_from_source("{ x: string in x }");
        match expr.node {
            Expr::Closure { params, .. } => {
                assert_eq!(params.len(), 1);
                assert!(params[0].ty.is_some());
            }
            _ => panic!("expected closure expr"),
        }
    }

    #[test]
    fn parse_closure_expr_multiple_params() {
        let expr = parse_expr_from_source("{ x, y in x }");
        match expr.node {
            Expr::Closure { params, .. } => assert_eq!(params.len(), 2),
            _ => panic!("expected closure expr"),
        }
    }

    #[test]
    fn parse_closure_expr_shorthand_params() {
        let expr = parse_expr_from_source("{ print($0) }");
        match expr.node {
            Expr::Closure {
                params,
                uses_shorthand_params,
                ..
            } => {
                assert!(params.is_empty());
                assert!(uses_shorthand_params);
            }
            _ => panic!("expected closure expr"),
        }
    }

    #[test]
    fn parse_closure_expr_body_with_statements_and_tail() {
        let expr = parse_expr_from_source("{ x in let y = x; y }");
        match expr.node {
            Expr::Closure { body, .. } => {
                assert_eq!(body.statements.len(), 1);
                assert!(body.tail_expr.is_some());
            }
            _ => panic!("expected closure expr"),
        }
    }

    #[test]
    fn parse_unsafe_closure_expr_with_explicit_param() {
        let expr = parse_expr_from_source("unsafe { x in x }");
        match expr.node {
            Expr::Closure {
                is_unsafe,
                uses_shorthand_params,
                params,
                ..
            } => {
                assert!(is_unsafe);
                assert!(!uses_shorthand_params);
                assert_eq!(params.len(), 1);
            }
            _ => panic!("expected unsafe closure expr"),
        }
    }

    #[test]
    fn parse_unsafe_block_expr() {
        let expr = parse_expr_from_source("unsafe { let x = 1; x }");
        match expr.node {
            Expr::UnsafeBlock(block) => {
                assert_eq!(block.statements.len(), 1);
                assert!(block.tail_expr.is_some());
            }
            _ => panic!("expected unsafe block expr"),
        }
    }

    #[test]
    fn parse_closure_expr_reports_error_on_missing_in_for_explicit_params() {
        let mut parser = parse_expr_with_parser("{ x: string x }");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedToken { .. }));
    }

    #[test]
    fn parse_closure_header_fallback_keeps_cursor_consistent() {
        let mut parser = parse_expr_with_parser("{ x, y }");
        let err = parser.parse_expr().expect_err("expected parse error");
        match err {
            ParseError::UnexpectedToken { found, .. } => {
                assert_eq!(found, TokenKind::Comma);
            }
            _ => panic!("expected unexpected-token parse error"),
        }
    }

    #[test]
    fn parse_call_with_closure_expr_arg() {
        let expr = parse_expr_from_source("call({ x in x })");
        match expr.node {
            Expr::Call { args, .. } => {
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0].value.node, Expr::Closure { .. }));
            }
            _ => panic!("expected call expr"),
        }
    }

    #[test]
    fn parse_binary_with_if_expr_operand() {
        let expr = parse_expr_from_source("(if x { a } else { b }) + z");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::Add,
                lhs,
                ..
            } => {
                assert!(matches!(lhs.node, Expr::Grouped(_)));
            }
            _ => panic!("expected additive binary expr"),
        }
    }

    #[test]
    fn parse_binary_with_match_expr_operand() {
        let expr = parse_expr_from_source("match x { _ => y } + z");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::Add,
                lhs,
                ..
            } => {
                assert!(matches!(lhs.node, Expr::Match { .. }));
            }
            _ => panic!("expected additive binary expr"),
        }
    }

    #[test]
    fn parse_ternary_expr_basic() {
        let expr = parse_expr_from_source("cond ? a : b");
        assert!(matches!(expr.node, Expr::Ternary { .. }));
    }

    #[test]
    fn parse_nested_right_associative_ternary_expr() {
        let expr = parse_expr_from_source("a ? b ? c : d : e");
        match expr.node {
            Expr::Ternary { then_expr, .. } => {
                assert!(matches!(then_expr.node, Expr::Ternary { .. }));
            }
            _ => panic!("expected ternary root"),
        }
    }

    #[test]
    fn parse_ternary_right_associative() {
        let expr = parse_expr_from_source("a ? b : c ? d : e");
        match expr.node {
            Expr::Ternary { else_expr, .. } => {
                assert!(matches!(else_expr.node, Expr::Ternary { .. }));
            }
            _ => panic!("expected ternary root"),
        }
    }

    #[test]
    fn parse_null_coalescing_expr() {
        let expr = parse_expr_from_source("a ?? b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::NullCoalescing,
                ..
            }
        ));
    }

    #[test]
    fn parse_null_coalescing_precedence_with_logical_or() {
        let expr = parse_expr_from_source("a || b ?? c");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::NullCoalescing,
                lhs,
                ..
            } => {
                assert!(matches!(
                    lhs.node,
                    Expr::Binary {
                        op: ast::BinaryOp::LogicalOr,
                        ..
                    }
                ));
            }
            _ => panic!("expected null-coalescing root"),
        }
    }

    #[test]
    fn parse_null_coalescing_vs_logical_or() {
        let expr = parse_expr_from_source("a || b ?? c");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::NullCoalescing,
                lhs,
                ..
            } => {
                assert!(matches!(
                    lhs.node,
                    Expr::Binary {
                        op: ast::BinaryOp::LogicalOr,
                        ..
                    }
                ));
            }
            _ => panic!("expected null-coalescing root"),
        }
    }

    #[test]
    fn parse_optional_member_access_expr() {
        let expr = parse_expr_from_source("a?.b");
        assert!(matches!(expr.node, Expr::OptionalMemberAccess { .. }));
    }

    #[test]
    fn parse_optional_member_call_expr() {
        let expr = parse_expr_from_source("a?.call()");
        match expr.node {
            Expr::Call { callee, .. } => {
                assert!(matches!(
                    callee.node,
                    Expr::OptionalMemberAccess { .. }
                ));
            }
            _ => panic!("expected call over optional member"),
        }
    }

    #[test]
    fn parse_optional_chain_then_call() {
        let expr = parse_expr_from_source("a?.b()");
        match expr.node {
            Expr::Call { callee, .. } => {
                assert!(matches!(
                    callee.node,
                    Expr::OptionalMemberAccess { .. }
                ));
            }
            _ => panic!("expected call over optional member"),
        }
    }

    #[test]
    fn parse_optional_index_expr() {
        let expr = parse_expr_from_source("a?[x]");
        assert!(matches!(expr.node, Expr::OptionalIndex { .. }));
    }

    #[test]
    fn parse_force_unwrap_expr() {
        let expr = parse_expr_from_source("a!");
        assert!(matches!(expr.node, Expr::ForceUnwrap { .. }));
    }

    #[test]
    fn parse_force_unwrap_member_chain_expr() {
        let expr = parse_expr_from_source("a!.b");
        match expr.node {
            Expr::MemberAccess { base, .. } => {
                assert!(matches!(base.node, Expr::ForceUnwrap { .. }));
            }
            _ => panic!("expected member access after force unwrap"),
        }
    }

    #[test]
    fn parse_force_unwrap_then_member_chain() {
        let expr = parse_expr_from_source("a!.b");
        match expr.node {
            Expr::MemberAccess { base, .. } => {
                assert!(matches!(base.node, Expr::ForceUnwrap { .. }));
            }
            _ => panic!("expected member access after force unwrap"),
        }
    }

    #[test]
    fn parse_force_unwrap_index_chain_expr() {
        let expr = parse_expr_from_source("a![x]");
        match expr.node {
            Expr::Index { base, .. } => {
                assert!(matches!(base.node, Expr::ForceUnwrap { .. }));
            }
            _ => panic!("expected index after force unwrap"),
        }
    }

    #[test]
    fn parse_optional_index_then_force_unwrap() {
        let expr = parse_expr_from_source("a?[x]!");
        assert!(matches!(expr.node, Expr::ForceUnwrap { .. }));
    }

    #[test]
    fn parse_cast_expr_as() {
        let expr = parse_expr_from_source("x as T");
        assert!(matches!(
            expr.node,
            Expr::Cast {
                is_optional: false,
                ..
            }
        ));
    }

    #[test]
    fn parse_cast_expr_as_optional() {
        let expr = parse_expr_from_source("x as? T");
        assert!(matches!(
            expr.node,
            Expr::Cast {
                is_optional: true,
                ..
            }
        ));
    }

    #[test]
    fn parse_cast_vs_additive_precedence() {
        let expr = parse_expr_from_source("x as T + y");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::Add,
                lhs,
                ..
            } => {
                assert!(matches!(lhs.node, Expr::Cast { .. }));
            }
            _ => panic!("expected additive root"),
        }
    }

    #[test]
    fn parse_optional_member_force_unwrap_member_chain_interaction() {
        let expr = parse_expr_from_source("a?.b!.c");
        match expr.node {
            Expr::MemberAccess { base, .. } => {
                assert!(matches!(base.node, Expr::ForceUnwrap { .. }));
            }
            _ => panic!("expected member-access root"),
        }
    }

    #[test]
    fn parse_optional_index_force_unwrap_member_chain_interaction() {
        let expr = parse_expr_from_source("a?[i]!.x");
        match expr.node {
            Expr::MemberAccess { base, .. } => {
                assert!(matches!(base.node, Expr::ForceUnwrap { .. }));
            }
            _ => panic!("expected member-access root"),
        }
    }

    #[test]
    fn parse_optional_cast_then_null_coalescing_interaction() {
        let expr = parse_expr_from_source("x as? T ?? y");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::NullCoalescing,
                lhs,
                ..
            } => {
                assert!(matches!(
                    lhs.node,
                    Expr::Cast {
                        is_optional: true,
                        ..
                    }
                ));
            }
            _ => panic!("expected null-coalescing root"),
        }
    }

    #[test]
    fn parse_ternary_then_null_coalescing_interaction() {
        let expr = parse_expr_from_source("a ? b : c ?? d");
        match expr.node {
            Expr::Ternary { else_expr, .. } => {
                assert!(matches!(
                    else_expr.node,
                    Expr::Binary {
                        op: ast::BinaryOp::NullCoalescing,
                        ..
                    }
                ));
            }
            _ => panic!("expected ternary root"),
        }
    }

    #[test]
    fn parse_null_coalescing_then_ternary_interaction() {
        let expr = parse_expr_from_source("a ?? b ? c : d");
        match expr.node {
            Expr::Ternary { condition, .. } => {
                assert!(matches!(
                    condition.node,
                    Expr::Binary {
                        op: ast::BinaryOp::NullCoalescing,
                        ..
                    }
                ));
            }
            _ => panic!("expected ternary root"),
        }
    }

    #[test]
    fn parse_shift_additive_interaction() {
        let expr = parse_expr_from_source("x << y + z");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::ShiftLeft,
                rhs,
                ..
            } => {
                assert!(matches!(
                    rhs.node,
                    Expr::Binary {
                        op: ast::BinaryOp::Add,
                        ..
                    }
                ));
            }
            _ => panic!("expected shift root"),
        }
    }

    #[test]
    fn parse_compound_assignment_with_logical_rhs_interaction() {
        let expr = parse_expr_from_source("a |= b && c");
        match expr.node {
            Expr::Assignment {
                op: ast::AssignOp::BitOrAssign,
                value,
                ..
            } => {
                assert!(matches!(
                    value.node,
                    Expr::Binary {
                        op: ast::BinaryOp::LogicalAnd,
                        ..
                    }
                ));
            }
            _ => panic!("expected assignment root"),
        }
    }

    #[test]
    fn parse_compound_assignment_with_optional_chain_rhs_interaction() {
        let expr = parse_expr_from_source("a += b?.c");
        match expr.node {
            Expr::Assignment {
                op: ast::AssignOp::AddAssign,
                value,
                ..
            } => {
                assert!(matches!(
                    value.node,
                    Expr::OptionalMemberAccess { .. }
                ));
            }
            _ => panic!("expected assignment root"),
        }
    }

    #[test]
    fn parse_assignment_ternary_compound_interaction_current_shape() {
        let expr = parse_expr_from_source("a = b ? c : d += e");
        match expr.node {
            Expr::Assignment {
                op: ast::AssignOp::Assign,
                value,
                ..
            } => {
                assert!(matches!(
                    value.node,
                    Expr::Assignment {
                        op: ast::AssignOp::AddAssign,
                        ..
                    }
                ));
            }
            _ => panic!("expected assignment root"),
        }
    }

    #[test]
    fn parse_force_unwrap_then_range_interaction() {
        let expr = parse_expr_from_source("a!..b");
        match expr.node {
            Expr::Range {
                start: Some(start), ..
            } => {
                assert!(matches!(start.node, Expr::ForceUnwrap { .. }));
            }
            _ => panic!("expected range root"),
        }
    }

    #[test]
    fn parse_invalid_postfix_force_unwrap_chain_reports_error() {
        let mut parser = parse_expr_with_parser("a!?.");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(
            err,
            ParseError::UnexpectedToken { .. }
                | ParseError::UnexpectedEof { .. }
        ));
    }

    #[test]
    fn parse_unary_negation_expr() {
        let expr = parse_expr_from_source("-x");
        assert!(matches!(
            expr.node,
            Expr::Unary {
                op: ast::UnaryOp::Negate,
                ..
            }
        ));
    }

    #[test]
    fn parse_unary_not_expr() {
        let expr = parse_expr_from_source("!flag");
        assert!(matches!(
            expr.node,
            Expr::Unary {
                op: ast::UnaryOp::Not,
                ..
            }
        ));
    }

    #[test]
    fn parse_try_expr() {
        let expr = parse_expr_from_source("try value");
        assert!(matches!(expr.node, Expr::Try { .. }));
    }

    #[test]
    fn parse_multiplicative_expr() {
        let expr = parse_expr_from_source("a * b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::Multiply,
                ..
            }
        ));
    }

    #[test]
    fn parse_additive_expr() {
        let expr = parse_expr_from_source("a + b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::Add,
                ..
            }
        ));
    }

    #[test]
    fn parse_precedence_mul_before_add() {
        let expr = parse_expr_from_source("a + b * c");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::Add,
                rhs,
                ..
            } => {
                assert!(matches!(
                    rhs.node,
                    Expr::Binary {
                        op: ast::BinaryOp::Multiply,
                        ..
                    }
                ));
            }
            _ => panic!("expected additive root"),
        }
    }

    #[test]
    fn parse_left_associative_additive_expr() {
        let expr = parse_expr_from_source("a - b - c");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::Subtract,
                lhs,
                ..
            } => {
                assert!(matches!(
                    lhs.node,
                    Expr::Binary {
                        op: ast::BinaryOp::Subtract,
                        ..
                    }
                ));
            }
            _ => panic!("expected subtract root"),
        }
    }

    #[test]
    fn parse_shift_expr_left() {
        let expr = parse_expr_from_source("a << b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::ShiftLeft,
                ..
            }
        ));
    }

    #[test]
    fn parse_shift_expr_precedence_below_additive() {
        let expr = parse_expr_from_source("a + b << c");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::ShiftLeft,
                lhs,
                ..
            } => {
                assert!(matches!(
                    lhs.node,
                    Expr::Binary {
                        op: ast::BinaryOp::Add,
                        ..
                    }
                ));
            }
            _ => panic!("expected shift root"),
        }
    }

    #[test]
    fn parse_shift_expr_left_associative() {
        let expr = parse_expr_from_source("a << b << c");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::ShiftLeft,
                lhs,
                ..
            } => {
                assert!(matches!(
                    lhs.node,
                    Expr::Binary {
                        op: ast::BinaryOp::ShiftLeft,
                        ..
                    }
                ));
            }
            _ => panic!("expected shift root"),
        }
    }

    #[test]
    fn parse_comparison_expr() {
        let expr = parse_expr_from_source("a < b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::Less,
                ..
            }
        ));
    }

    #[test]
    fn parse_equality_expr() {
        let expr = parse_expr_from_source("a == b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::Equal,
                ..
            }
        ));
    }

    #[test]
    fn parse_bitwise_and_expr() {
        let expr = parse_expr_from_source("a & b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::BitAnd,
                ..
            }
        ));
    }

    #[test]
    fn parse_bitwise_xor_expr() {
        let expr = parse_expr_from_source("a ^ b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::BitXor,
                ..
            }
        ));
    }

    #[test]
    fn parse_bitwise_or_expr() {
        let expr = parse_expr_from_source("a | b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::BitOr,
                ..
            }
        ));
    }

    #[test]
    fn parse_bitwise_precedence_and_before_xor_before_or() {
        let expr = parse_expr_from_source("a | b ^ c & d");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::BitOr,
                rhs,
                ..
            } => match rhs.node {
                Expr::Binary {
                    op: ast::BinaryOp::BitXor,
                    rhs,
                    ..
                } => {
                    assert!(matches!(
                        rhs.node,
                        Expr::Binary {
                            op: ast::BinaryOp::BitAnd,
                            ..
                        }
                    ));
                }
                _ => panic!("expected bitwise xor on right side"),
            },
            _ => panic!("expected bitwise or root"),
        }
    }

    #[test]
    fn parse_logical_and_expr() {
        let expr = parse_expr_from_source("a && b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::LogicalAnd,
                ..
            }
        ));
    }

    #[test]
    fn parse_logical_or_expr() {
        let expr = parse_expr_from_source("a || b");
        assert!(matches!(
            expr.node,
            Expr::Binary {
                op: ast::BinaryOp::LogicalOr,
                ..
            }
        ));
    }

    #[test]
    fn parse_logical_precedence_and_before_or() {
        let expr = parse_expr_from_source("a || b && c");
        match expr.node {
            Expr::Binary {
                op: ast::BinaryOp::LogicalOr,
                rhs,
                ..
            } => {
                assert!(matches!(
                    rhs.node,
                    Expr::Binary {
                        op: ast::BinaryOp::LogicalAnd,
                        ..
                    }
                ));
            }
            _ => panic!("expected logical-or root"),
        }
    }

    #[test]
    fn parse_assignment_expr() {
        let expr = parse_expr_from_source("a = b");
        assert!(matches!(
            expr.node,
            Expr::Assignment {
                op: ast::AssignOp::Assign,
                ..
            }
        ));
    }

    #[test]
    fn parse_compound_assignment_add() {
        let expr = parse_expr_from_source("a += b");
        assert!(matches!(
            expr.node,
            Expr::Assignment {
                op: ast::AssignOp::AddAssign,
                ..
            }
        ));
    }

    #[test]
    fn parse_compound_assignment_right_associative() {
        let expr = parse_expr_from_source("a = b += c");
        match expr.node {
            Expr::Assignment { op, value, .. } => {
                assert_eq!(op, ast::AssignOp::Assign);
                assert!(matches!(
                    value.node,
                    Expr::Assignment {
                        op: ast::AssignOp::AddAssign,
                        ..
                    }
                ));
            }
            _ => panic!("expected assignment root"),
        }
    }

    #[test]
    fn parse_compound_assignment_with_rhs_precedence() {
        let expr = parse_expr_from_source("a <<= b + c");
        match expr.node {
            Expr::Assignment { op, value, .. } => {
                assert_eq!(op, ast::AssignOp::ShlAssign);
                assert!(matches!(
                    value.node,
                    Expr::Binary {
                        op: ast::BinaryOp::Add,
                        ..
                    }
                ));
            }
            _ => panic!("expected assignment root"),
        }
    }

    #[test]
    fn parse_right_associative_assignment_expr() {
        let expr = parse_expr_from_source("a = b = c");
        match expr.node {
            Expr::Assignment { value, .. } => {
                assert!(matches!(value.node, Expr::Assignment { .. }));
            }
            _ => panic!("expected assignment root"),
        }
    }

    #[test]
    fn parse_closed_range_expr() {
        let expr = parse_expr_from_source("a..b");
        assert!(matches!(
            expr.node,
            Expr::Range {
                start: Some(_),
                end: Some(_),
                inclusive: false
            }
        ));
    }

    #[test]
    fn parse_inclusive_range_expr() {
        let expr = parse_expr_from_source("a..=b");
        assert!(matches!(
            expr.node,
            Expr::Range {
                start: Some(_),
                end: Some(_),
                inclusive: true
            }
        ));
    }

    #[test]
    fn parse_open_end_range_expr() {
        let expr = parse_expr_from_source("a..");
        assert!(matches!(
            expr.node,
            Expr::Range {
                start: Some(_),
                end: None,
                inclusive: false
            }
        ));
    }

    #[test]
    fn parse_open_start_range_expr() {
        let expr = parse_expr_from_source("..b");
        assert!(matches!(
            expr.node,
            Expr::Range {
                start: None,
                end: Some(_),
                inclusive: false
            }
        ));
    }

    #[test]
    fn parse_open_start_inclusive_range_expr() {
        let expr = parse_expr_from_source("..=b");
        assert!(matches!(
            expr.node,
            Expr::Range {
                start: None,
                end: Some(_),
                inclusive: true
            }
        ));
    }

    #[test]
    fn parse_range_has_lower_precedence_than_additive() {
        let expr = parse_expr_from_source("a + b..c");
        match expr.node {
            Expr::Range {
                start: Some(start), ..
            } => {
                assert!(matches!(
                    start.node,
                    Expr::Binary {
                        op: ast::BinaryOp::Add,
                        ..
                    }
                ));
            }
            _ => panic!("expected range root"),
        }
    }

    #[test]
    fn parse_range_vs_null_coalescing() {
        let expr = parse_expr_from_source("a ?? b..c");
        match expr.node {
            Expr::Range {
                start: Some(start), ..
            } => {
                assert!(matches!(
                    start.node,
                    Expr::Binary {
                        op: ast::BinaryOp::NullCoalescing,
                        ..
                    }
                ));
            }
            _ => panic!("expected range root"),
        }
    }

    #[test]
    fn parse_postfix_then_binary_expr() {
        let expr = parse_expr_from_source("foo.bar + baz");
        match expr.node {
            Expr::Binary { lhs, .. } => {
                assert!(matches!(lhs.node, Expr::MemberAccess { .. }));
            }
            _ => panic!("expected binary root"),
        }
    }

    #[test]
    fn parse_call_then_binary_expr() {
        let expr = parse_expr_from_source("f(x) * y");
        match expr.node {
            Expr::Binary { lhs, .. } => {
                assert!(matches!(lhs.node, Expr::Call { .. }));
            }
            _ => panic!("expected binary root"),
        }
    }

    #[test]
    fn parse_index_then_comparison_expr() {
        let expr = parse_expr_from_source("xs[i] == y");
        match expr.node {
            Expr::Binary { lhs, .. } => {
                assert!(matches!(lhs.node, Expr::Index { .. }));
            }
            _ => panic!("expected binary root"),
        }
    }

    #[test]
    fn parse_expr_reports_error_for_prefix_without_operand() {
        let mut parser = parse_expr_with_parser("-");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));

        let mut parser = parse_expr_with_parser("try");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }

    #[test]
    fn parse_range_reports_error_when_end_is_missing_but_required() {
        let mut parser = parse_expr_with_parser("a..=");
        let err = parser.parse_expr().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }

    fn parse_type_from_source(source: &str) -> Spanned<Type> {
        let mut parser = Parser::new(source).expect("parser");
        let ty = parser.parse_type().expect("type parse");
        assert!(parser.is_eof(), "expected eof after type parse");
        ty
    }

    fn parse_param_list_from_source(source: &str) -> Vec<Spanned<ParamDecl>> {
        let mut parser = Parser::new(source).expect("parser");
        let params = parser.parse_param_list().expect("param list parse");
        assert!(parser.is_eof(), "expected eof after param list parse");
        params
    }

    #[test]
    fn parse_named_type() {
        let ty = parse_type_from_source("Foo");
        match ty.node {
            Type::Named { segments } => assert_eq!(segments, vec!["Foo"]),
            _ => panic!("expected named type"),
        }
    }

    #[test]
    fn parse_path_type() {
        let ty = parse_type_from_source("core::fmt::Formatter");
        match ty.node {
            Type::Named { segments } => {
                assert_eq!(segments, vec!["core", "fmt", "Formatter"]);
            }
            _ => panic!("expected path type"),
        }
    }

    #[test]
    fn parse_self_type() {
        let ty = parse_type_from_source("Self");
        assert!(matches!(ty.node, Type::SelfType));
    }

    #[test]
    fn parse_reference_type() {
        let ty = parse_type_from_source("&Foo");
        match ty.node {
            Type::Reference(inner) => match inner.node {
                Type::Named { segments } => assert_eq!(segments, vec!["Foo"]),
                _ => panic!("expected named inner type"),
            },
            _ => panic!("expected reference type"),
        }
    }

    #[test]
    fn parse_mutable_reference_type() {
        let ty = parse_type_from_source("&mut Foo");
        assert!(matches!(ty.node, Type::MutableReference(_)));
    }

    #[test]
    fn parse_pointer_type() {
        let ty = parse_type_from_source("*void");
        assert!(matches!(ty.node, Type::ConstPointer(_)));
    }

    #[test]
    fn parse_mutable_pointer_type() {
        let ty = parse_type_from_source("*mut void");
        assert!(matches!(ty.node, Type::MutablePointer(_)));
    }

    #[test]
    fn parse_array_type() {
        let ty = parse_type_from_source("[i32]");
        assert!(matches!(ty.node, Type::Array(_)));
    }

    #[test]
    fn parse_optional_type() {
        let ty = parse_type_from_source("Foo?");
        assert!(matches!(ty.node, Type::Optional(_)));
    }

    #[test]
    fn parse_result_type() {
        let ty = parse_type_from_source("Foo!Bar");
        assert!(matches!(ty.node, Type::Result { .. }));
    }

    #[test]
    fn parse_grouped_type() {
        let ty = parse_type_from_source("(Foo)");
        assert!(matches!(ty.node, Type::Grouped(_)));
    }

    #[test]
    fn parse_generic_application_type() {
        let ty = parse_type_from_source("Option<Foo>");
        assert!(matches!(ty.node, Type::GenericApplication { .. }));
    }

    #[test]
    fn parse_path_generic_application_type() {
        let ty = parse_type_from_source("core::option::Option<Foo>");
        match ty.node {
            Type::GenericApplication { base, .. } => match base.node {
                Type::Named { segments } => {
                    assert_eq!(segments, vec!["core", "option", "Option"]);
                }
                _ => panic!("expected path base"),
            },
            _ => panic!("expected generic type"),
        }
    }

    #[test]
    fn parse_unlabeled_param() {
        let params = parse_param_list_from_source("(x: i32)");
        assert_eq!(params.len(), 1);
        assert!(matches!(params[0].node.label, ParamLabel::FromName));
        assert_eq!(params[0].node.name, "x");
    }

    #[test]
    fn parse_underscore_labeled_param() {
        let params = parse_param_list_from_source("(_ x: i32)");
        assert_eq!(params.len(), 1);
        assert!(matches!(params[0].node.label, ParamLabel::None));
        assert_eq!(params[0].node.name, "x");
    }

    #[test]
    fn parse_named_labeled_param() {
        let params = parse_param_list_from_source("(label x: i32)");
        assert_eq!(params.len(), 1);
        match &params[0].node.label {
            ParamLabel::Explicit(label) => assert_eq!(label, "label"),
            _ => panic!("expected explicit label"),
        }
        assert_eq!(params[0].node.name, "x");
    }

    #[test]
    fn parse_empty_param_list() {
        let params = parse_param_list_from_source("()");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_multiple_params() {
        let params =
            parse_param_list_from_source("(_ x: i32, y: string, label z: Foo)");
        assert_eq!(params.len(), 3);
        assert!(matches!(params[0].node.label, ParamLabel::None));
        assert!(matches!(params[1].node.label, ParamLabel::FromName));
        assert!(matches!(params[2].node.label, ParamLabel::Explicit(_)));
    }

    #[test]
    fn parse_type_reports_error_on_missing_inner_type() {
        let mut parser = Parser::new("&").expect("parser");
        let err = parser.parse_type().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));

        let mut parser = Parser::new("*mut").expect("parser");
        let err = parser.parse_type().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }

    #[test]
    fn parse_param_reports_error_on_missing_colon() {
        let mut parser = Parser::new("(x i32)").expect("parser");
        let err = parser.parse_param_list().expect_err("expected parse error");
        assert!(matches!(
            err,
            ParseError::UnexpectedToken { .. }
                | ParseError::UnexpectedEof { .. }
        ));
    }

    #[test]
    fn parse_param_list_reports_error_on_missing_rparen() {
        let mut parser = Parser::new("(x: i32").expect("parser");
        let err = parser.parse_param_list().expect_err("expected parse error");
        assert!(matches!(err, ParseError::UnexpectedEof { .. }));
    }
}
