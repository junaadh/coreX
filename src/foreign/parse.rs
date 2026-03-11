use super::decl::{
    ForeignFunctionDecl, ForeignLibraryDecl, LoweringError,
    validate_foreign_library_decl,
};
use crate::ffi::{ForeignCallConv, NativeType, Signature};
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// Parsed source-level call-convention marker for foreign declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallConv {
    C,
}

/// Parsed source-level attribute for foreign declarations.
///
/// This parser supports only `@call(.C)` shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute {
    Call(CallConv),
}

/// Parsed source-level type used in foreign declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceForeignType {
    Void,
    I32,
    Usize,
    PtrConstVoid,
    PtrMutVoid,
}

/// Parsed source-level parameter name form.
///
/// This preserves whether parameter syntax used an omitted name, `_`, or an
/// explicit identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamName {
    Unnamed,
    Ignored,
    Named(String),
}

/// Parsed source-level foreign function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedForeignParam {
    name: ParamName,
    ty: SourceForeignType,
}

impl ParsedForeignParam {
    #[must_use]
    pub fn new(name: ParamName, ty: SourceForeignType) -> Self {
        Self { name, ty }
    }

    #[must_use]
    pub fn name(&self) -> &ParamName {
        &self.name
    }

    #[must_use]
    pub fn ty(&self) -> SourceForeignType {
        self.ty
    }
}

/// Parsed source-level foreign function declaration.
///
/// This preserves source-facing attributes and parameter spelling details,
/// distinguishes local and native symbol names, and records the declared source
/// return type. It does not verify native ABI truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedForeignFunctionDecl {
    attributes: Vec<Attribute>,
    local_name: String,
    symbol_name: String,
    params: Vec<ParsedForeignParam>,
    ret_type: SourceForeignType,
}

impl ParsedForeignFunctionDecl {
    #[must_use]
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }

    #[must_use]
    pub fn local_name(&self) -> &str {
        &self.local_name
    }

    #[must_use]
    pub fn symbol_name(&self) -> &str {
        &self.symbol_name
    }

    #[must_use]
    pub fn params(&self) -> &[ParsedForeignParam] {
        &self.params
    }

    #[must_use]
    pub fn ret_type(&self) -> SourceForeignType {
        self.ret_type
    }
}

/// Parsed source-level foreign extern block declaration.
///
/// This type represents source syntax and intentionally does not include a
/// runtime library path. Lowering into runtime-oriented declarations requires a
/// separate explicit path input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedForeignLibraryDecl {
    library_name: String,
    attributes: Vec<Attribute>,
    functions: Vec<ParsedForeignFunctionDecl>,
}

impl ParsedForeignLibraryDecl {
    #[must_use]
    pub fn library_name(&self) -> &str {
        &self.library_name
    }

    #[must_use]
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }

    #[must_use]
    pub fn functions(&self) -> &[ParsedForeignFunctionDecl] {
        &self.functions
    }
}

/// Parsed foreign-only source file containing zero or more extern blocks.
///
/// The `libraries` list preserves source order and does not merge blocks with
/// identical library names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedForeignFile {
    libraries: Vec<ParsedForeignLibraryDecl>,
}

impl ParsedForeignFile {
    #[must_use]
    pub fn libraries(&self) -> &[ParsedForeignLibraryDecl] {
        &self.libraries
    }
}

/// Parse failures for source-level foreign declaration syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEof {
        offset: usize,
    },
    UnexpectedToken {
        expected: &'static str,
        found: String,
        offset: usize,
    },
    ExpectedIdentifier {
        offset: usize,
    },
    InvalidType {
        found: String,
        offset: usize,
    },
    InvalidAttribute {
        found: String,
        offset: usize,
    },
    TrailingInput {
        remaining: String,
        offset: usize,
    },
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEof { offset } => {
                write!(f, "unexpected end of input at byte offset {offset}")
            }
            Self::UnexpectedToken {
                expected,
                found,
                offset,
            } => {
                write!(
                    f,
                    "unexpected token at byte offset {offset}: expected {expected}, found {found}"
                )
            }
            Self::ExpectedIdentifier { offset } => {
                write!(f, "expected identifier at byte offset {offset}")
            }
            Self::InvalidType { found, offset } => {
                write!(f, "invalid type {found} at byte offset {offset}")
            }
            Self::InvalidAttribute { found, offset } => {
                write!(f, "invalid attribute {found} at byte offset {offset}")
            }
            Self::TrailingInput { remaining, offset } => {
                write!(
                    f,
                    "unexpected trailing input at byte offset {offset}: {remaining}"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses one foreign extern block from source text.
///
/// This parser recognizes a narrow declaration subset:
/// - optional `@call(.C)` attributes before `extern`
/// - `extern <library_name> { ... }`
/// - optional function attributes and foreign function declarations inside
///   the block
///
/// The parser does not perform runtime loading or runtime lowering.
///
/// # Errors
/// Returns [`ParseError`] when the input does not match the supported syntax.
pub fn parse_foreign_library_decl(
    input: &str,
) -> Result<ParsedForeignLibraryDecl, ParseError> {
    let mut parser = Parser::new(input);
    let parsed = parser.parse_library_decl()?;

    parser.skip_ws();
    if !parser.is_eof() {
        return Err(ParseError::TrailingInput {
            remaining: parser.remaining().to_string(),
            offset: parser.offset,
        });
    }

    Ok(parsed)
}

/// Parses a foreign-only source file into zero or more extern blocks.
///
/// The source file syntax accepted by this parser consists only of whitespace
/// and foreign extern blocks already supported by
/// [`parse_foreign_library_decl`].
///
/// # Errors
/// Returns [`ParseError`] when input contains invalid foreign-block syntax or
/// non-whitespace content that is not a valid extern block.
pub fn parse_foreign_file(
    input: &str,
) -> Result<ParsedForeignFile, ParseError> {
    let mut parser = Parser::new(input);
    let mut libraries = Vec::new();
    parser.skip_ws();

    while !parser.is_eof() {
        libraries.push(parser.parse_library_decl()?);
        parser.skip_ws();
    }

    Ok(ParsedForeignFile { libraries })
}

/// Lowers parsed source-facing foreign declarations into normalized IR.
///
/// Source syntax omits runtime library paths, so lowering requires an explicit
/// `library_path` argument.
///
/// Lowering resolves call-convention attributes with precedence:
/// function-level overrides block-level, block-level overrides default, and the
/// default foreign calling convention is C.
///
/// # Errors
/// Returns [`LoweringError`] if the lowered normalized declaration fails
/// structural validation, or if duplicate call-convention attributes are
/// present on one block/function item.
pub fn lower_parsed_foreign_library_decl(
    parsed: &ParsedForeignLibraryDecl,
    library_path: impl Into<PathBuf>,
) -> Result<ForeignLibraryDecl, LoweringError> {
    let block_call_conv =
        resolve_block_call_conv(parsed.attributes(), parsed.library_name())?;
    let default_call_conv =
        block_call_conv.unwrap_or(ForeignCallConv::default_foreign());

    let functions = parsed
        .functions
        .iter()
        .map(|function| {
            let params = function
                .params
                .iter()
                .map(|param| map_source_type(param.ty))
                .collect::<Vec<_>>();
            let ret = map_source_type(function.ret_type);
            let signature = Signature::new(params, ret);
            let function_call_conv = resolve_function_call_conv(
                function.attributes(),
                function.local_name(),
            )?;
            let resolved_call_conv = function_call_conv
                .or(block_call_conv)
                .unwrap_or(ForeignCallConv::default_foreign());

            Ok(ForeignFunctionDecl::with_call_conv(
                function.local_name.clone(),
                function.symbol_name.clone(),
                signature,
                resolved_call_conv,
            ))
        })
        .collect::<Result<Vec<_>, LoweringError>>()?;

    let lowered = ForeignLibraryDecl::with_default_call_conv(
        parsed.library_name.clone(),
        library_path,
        default_call_conv,
        functions,
    );
    validate_foreign_library_decl(&lowered)?;
    Ok(lowered)
}

fn resolve_block_call_conv(
    attrs: &[Attribute],
    library_name: &str,
) -> Result<Option<ForeignCallConv>, LoweringError> {
    resolve_call_conv(attrs, format!("library `{library_name}`"))
}

fn resolve_function_call_conv(
    attrs: &[Attribute],
    function_name: &str,
) -> Result<Option<ForeignCallConv>, LoweringError> {
    resolve_call_conv(attrs, format!("function `{function_name}`"))
}

fn resolve_call_conv(
    attrs: &[Attribute],
    context: String,
) -> Result<Option<ForeignCallConv>, LoweringError> {
    let mut conv = None;
    for attr in attrs {
        match attr {
            Attribute::Call(source_conv) => {
                if conv.is_some() {
                    return Err(LoweringError::DuplicateCallConvAttribute {
                        context,
                    });
                }
                conv = Some(map_source_call_conv(*source_conv));
            }
        }
    }
    Ok(conv)
}

fn map_source_call_conv(source: CallConv) -> ForeignCallConv {
    match source {
        CallConv::C => ForeignCallConv::C,
    }
}

fn map_source_type(ty: SourceForeignType) -> NativeType {
    match ty {
        SourceForeignType::Void => NativeType::Void,
        SourceForeignType::I32 => NativeType::I32,
        SourceForeignType::Usize => NativeType::USize,
        SourceForeignType::PtrConstVoid | SourceForeignType::PtrMutVoid => {
            NativeType::Ptr
        }
    }
}

struct Parser<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut attributes = Vec::new();
        loop {
            self.skip_ws();
            if !self.try_consume_char('@') {
                break;
            }

            let attr_offset = self.offset.saturating_sub(1);
            let name = self.parse_identifier()?;
            self.expect_char('(', "'('")?;
            self.expect_char('.', "'.'")?;
            let variant = self.parse_identifier()?;
            self.expect_char(')', "')'")?;

            match (name.as_str(), variant.as_str()) {
                ("call", "C") => {
                    attributes.push(Attribute::Call(CallConv::C));
                }
                _ => {
                    return Err(ParseError::InvalidAttribute {
                        found: format!("@{name}(.{variant})"),
                        offset: attr_offset,
                    });
                }
            }
        }
        Ok(attributes)
    }

    fn parse_library_decl(
        &mut self,
    ) -> Result<ParsedForeignLibraryDecl, ParseError> {
        let attributes = self.parse_attributes()?;
        self.expect_keyword("extern")?;
        let library_name = self.parse_identifier()?;
        self.expect_char('{', "'{'")?;

        let mut functions = Vec::new();
        loop {
            let function_attrs = self.parse_attributes()?;
            self.skip_ws();
            if self.try_consume_char('}') {
                break;
            }
            self.expect_keyword("fn")?;
            functions.push(self.parse_function_decl(function_attrs)?);
        }

        Ok(ParsedForeignLibraryDecl {
            library_name,
            attributes,
            functions,
        })
    }

    fn parse_function_decl(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> Result<ParsedForeignFunctionDecl, ParseError> {
        let local_name = self.parse_identifier()?;

        let symbol_name = if self.try_consume_char('=') {
            self.parse_identifier()?
        } else {
            local_name.clone()
        };

        self.expect_char('(', "'('")?;
        let params = self.parse_param_list()?;
        self.expect_arrow()?;
        let ret_type = self.parse_type()?;
        self.expect_char(';', "';'")?;

        Ok(ParsedForeignFunctionDecl {
            attributes,
            local_name,
            symbol_name,
            params,
            ret_type,
        })
    }

    fn parse_param_list(
        &mut self,
    ) -> Result<Vec<ParsedForeignParam>, ParseError> {
        let mut params = Vec::new();
        self.skip_ws();
        if self.try_consume_char(')') {
            return Ok(params);
        }

        loop {
            params.push(self.parse_param()?);
            self.skip_ws();

            if self.try_consume_char(',') {
                continue;
            }

            self.expect_char(')', "')'")?;
            break;
        }

        Ok(params)
    }

    fn parse_param(&mut self) -> Result<ParsedForeignParam, ParseError> {
        self.skip_ws();
        if self.starts_type() {
            let ty = self.parse_type()?;
            return Ok(ParsedForeignParam::new(ParamName::Unnamed, ty));
        }

        let name = self.parse_identifier()?;
        self.expect_char(':', "':'")?;
        let ty = self.parse_type()?;

        let name = if name == "_" {
            ParamName::Ignored
        } else {
            ParamName::Named(name)
        };
        Ok(ParsedForeignParam::new(name, ty))
    }

    fn parse_type(&mut self) -> Result<SourceForeignType, ParseError> {
        self.skip_ws();
        let start = self.offset;

        if self.try_consume_char('*') {
            let qualifier = self.parse_identifier()?;
            let base = self.parse_identifier()?;
            if base != "void" {
                return Err(ParseError::InvalidType {
                    found: format!("*{qualifier} {base}"),
                    offset: start,
                });
            }

            return match qualifier.as_str() {
                "const" => Ok(SourceForeignType::PtrConstVoid),
                "mut" => Ok(SourceForeignType::PtrMutVoid),
                _ => Err(ParseError::InvalidType {
                    found: format!("*{qualifier} void"),
                    offset: start,
                }),
            };
        }

        let ident = self.parse_identifier()?;
        match ident.as_str() {
            "void" => Ok(SourceForeignType::Void),
            "i32" => Ok(SourceForeignType::I32),
            "usize" => Ok(SourceForeignType::Usize),
            _ => Err(ParseError::InvalidType {
                found: ident,
                offset: start,
            }),
        }
    }

    fn starts_type(&mut self) -> bool {
        self.skip_ws();
        self.remaining().starts_with('*')
            || self.starts_with_keyword("void")
            || self.starts_with_keyword("i32")
            || self.starts_with_keyword("usize")
    }

    fn expect_arrow(&mut self) -> Result<(), ParseError> {
        self.skip_ws();
        if self.remaining().starts_with("->") {
            self.offset += 2;
            Ok(())
        } else if self.is_eof() {
            Err(ParseError::UnexpectedEof {
                offset: self.offset,
            })
        } else {
            Err(ParseError::UnexpectedToken {
                expected: "'->'",
                found: self.peek_token(),
                offset: self.offset,
            })
        }
    }

    fn expect_keyword(
        &mut self,
        keyword: &'static str,
    ) -> Result<(), ParseError> {
        self.skip_ws();
        if self.starts_with_keyword(keyword) {
            self.offset += keyword.len();
            Ok(())
        } else if self.is_eof() {
            Err(ParseError::UnexpectedEof {
                offset: self.offset,
            })
        } else {
            Err(ParseError::UnexpectedToken {
                expected: keyword,
                found: self.peek_token(),
                offset: self.offset,
            })
        }
    }

    fn expect_char(
        &mut self,
        expected_char: char,
        expected: &'static str,
    ) -> Result<(), ParseError> {
        self.skip_ws();
        if self.try_consume_char(expected_char) {
            Ok(())
        } else if self.is_eof() {
            Err(ParseError::UnexpectedEof {
                offset: self.offset,
            })
        } else {
            Err(ParseError::UnexpectedToken {
                expected,
                found: self.peek_token(),
                offset: self.offset,
            })
        }
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        let remaining = self.remaining();
        let mut chars = remaining.char_indices();
        let Some((_, first)) = chars.next() else {
            return Err(ParseError::UnexpectedEof {
                offset: self.offset,
            });
        };
        if !is_ident_start(first) {
            return Err(ParseError::ExpectedIdentifier {
                offset: self.offset,
            });
        }

        let mut end = self.offset + first.len_utf8();
        for (idx, ch) in chars {
            if is_ident_continue(ch) {
                end = self.offset + idx + ch.len_utf8();
            } else {
                break;
            }
        }

        let ident = &self.input[self.offset..end];
        self.offset = end;
        Ok(ident.to_owned())
    }

    fn starts_with_keyword(&self, keyword: &str) -> bool {
        let remaining = self.remaining();
        if !remaining.starts_with(keyword) {
            return false;
        }

        let boundary_index = keyword.len();
        match remaining[boundary_index..].chars().next() {
            Some(ch) => !is_ident_continue(ch),
            None => true,
        }
    }

    fn try_consume_char(&mut self, ch: char) -> bool {
        self.skip_ws();
        if self.remaining().starts_with(ch) {
            self.offset += ch.len_utf8();
            true
        } else {
            false
        }
    }

    fn peek_token(&self) -> String {
        let remaining = self.remaining();
        if remaining.is_empty() {
            return "<eof>".to_string();
        }

        let Some(first) = remaining.chars().next() else {
            return "<eof>".to_string();
        };
        if is_ident_start(first) {
            let mut end = first.len_utf8();
            for (idx, ch) in remaining.char_indices().skip(1) {
                if is_ident_continue(ch) {
                    end = idx + ch.len_utf8();
                } else {
                    break;
                }
            }
            remaining[..end].to_string()
        } else if remaining.starts_with("->") {
            "->".to_string()
        } else {
            first.to_string()
        }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.remaining().chars().next() {
            if ch.is_whitespace() {
                self.offset += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    fn is_eof(&self) -> bool {
        self.offset >= self.input.len()
    }

    fn remaining(&self) -> &str {
        &self.input[self.offset..]
    }
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
    use crate::ffi::{ForeignCallConv, NativeType, Value};
    use crate::foreign::lower_foreign_library_decl;
    use std::ffi::CString;
    use std::path::Path;

    #[test]
    fn parse_extern_block_with_identical_name_functions() {
        let src = r"
extern libSystem {
    fn strlen(s: *const void) -> usize;
    fn puts(_: *const void) -> i32;
}
";
        let parsed =
            parse_foreign_library_decl(src).expect("parse extern block");
        assert_eq!(parsed.library_name(), "libSystem");
        assert_eq!(parsed.functions().len(), 2);
        assert_eq!(parsed.functions()[0].local_name(), "strlen");
        assert_eq!(parsed.functions()[0].symbol_name(), "strlen");
    }

    #[test]
    fn parse_extern_block_with_alias_function() {
        let src = r"
extern libSystem {
    fn pid = getpid() -> i32;
}
";
        let parsed = parse_foreign_library_decl(src).expect("parse alias");
        assert_eq!(parsed.functions().len(), 1);
        assert_eq!(parsed.functions()[0].local_name(), "pid");
        assert_eq!(parsed.functions()[0].symbol_name(), "getpid");
    }

    #[test]
    fn parse_block_attribute_call_c() {
        let src = r"
@call(.C)
extern libSystem {
    fn getpid() -> i32;
}
";
        let parsed =
            parse_foreign_library_decl(src).expect("parse block attribute");
        assert_eq!(parsed.attributes(), &[Attribute::Call(CallConv::C)]);
    }

    #[test]
    fn parse_function_attribute_call_c() {
        let src = r"
extern libSystem {
    @call(.C)
    fn getpid() -> i32;
}
";
        let parsed =
            parse_foreign_library_decl(src).expect("parse function attribute");
        assert_eq!(
            parsed.functions()[0].attributes(),
            &[Attribute::Call(CallConv::C)]
        );
    }

    #[test]
    fn parse_param_forms_named_ignored_and_unnamed() {
        let src = r"
extern libSystem {
    fn demo(a: *const void, _: *mut void, usize) -> void;
}
";
        let parsed =
            parse_foreign_library_decl(src).expect("parse parameter forms");
        let params = parsed.functions()[0].params();
        assert_eq!(params.len(), 3);
        assert_eq!(params[0].name(), &ParamName::Named("a".to_string()));
        assert_eq!(params[1].name(), &ParamName::Ignored);
        assert_eq!(params[2].name(), &ParamName::Unnamed);
        assert_eq!(params[0].ty(), SourceForeignType::PtrConstVoid);
        assert_eq!(params[1].ty(), SourceForeignType::PtrMutVoid);
        assert_eq!(params[2].ty(), SourceForeignType::Usize);
    }

    #[test]
    fn parse_rejects_unknown_type_name() {
        let src = r"
extern libSystem {
    fn bad(x: foo) -> i32;
}
";
        let err =
            parse_foreign_library_decl(src).expect_err("unknown type rejected");
        assert!(matches!(err, ParseError::InvalidType { .. }));
    }

    #[test]
    fn parse_rejects_missing_semicolon() {
        let src = r"
extern libSystem {
    fn getpid() -> i32
}
";
        let err = parse_foreign_library_decl(src)
            .expect_err("missing semicolon rejected");
        assert!(matches!(
            err,
            ParseError::UnexpectedToken {
                expected: "';'",
                ..
            }
        ));
    }

    #[test]
    fn parse_rejects_bad_top_level_keyword() {
        let src = r"
module libSystem {
    fn getpid() -> i32;
}
";
        let err = parse_foreign_library_decl(src)
            .expect_err("bad top-level keyword rejected");
        assert!(matches!(
            err,
            ParseError::UnexpectedToken {
                expected: "extern",
                ..
            }
        ));
    }

    #[test]
    fn parse_rejects_bad_alias_form() {
        let src = r"
extern libSystem {
    fn pid = () -> i32;
}
";
        let err =
            parse_foreign_library_decl(src).expect_err("bad alias rejected");
        assert!(matches!(err, ParseError::ExpectedIdentifier { .. }));
    }

    #[test]
    fn parse_rejects_trailing_input() {
        let src = r"
extern libSystem {
    fn getpid() -> i32;
}
garbage
";
        let err = parse_foreign_library_decl(src)
            .expect_err("trailing input rejected");
        assert!(matches!(err, ParseError::TrailingInput { .. }));
    }

    #[test]
    fn parse_file_with_zero_blocks() {
        let parsed = parse_foreign_file("   \n\t  ").expect("parse empty file");
        assert!(parsed.libraries().is_empty());
    }

    #[test]
    fn parse_file_with_one_block() {
        let parsed = parse_foreign_file(
            r"
extern libSystem {
    fn getpid() -> i32;
}
",
        )
        .expect("parse file with one block");

        assert_eq!(parsed.libraries().len(), 1);
        assert_eq!(parsed.libraries()[0].library_name(), "libSystem");
    }

    #[test]
    fn parse_file_with_multiple_blocks() {
        let parsed = parse_foreign_file(
            r"
extern libSystem {
    fn getpid() -> i32;
}

extern sqlite3 {
    fn sqlite3_close(db: *mut void) -> i32;
}
",
        )
        .expect("parse file with multiple blocks");

        assert_eq!(parsed.libraries().len(), 2);
        assert_eq!(parsed.libraries()[0].library_name(), "libSystem");
        assert_eq!(parsed.libraries()[1].library_name(), "sqlite3");
    }

    #[test]
    fn parse_file_preserves_block_attributes() {
        let parsed = parse_foreign_file(
            r"
@call(.C)
extern libSystem {
    fn getpid() -> i32;
}

extern sqlite3 {
    fn sqlite3_close(db: *mut void) -> i32;
}
",
        )
        .expect("parse file with attributes");

        assert_eq!(
            parsed.libraries()[0].attributes(),
            &[Attribute::Call(CallConv::C)]
        );
        assert!(parsed.libraries()[1].attributes().is_empty());
    }

    #[test]
    fn parse_file_rejects_garbage_between_blocks() {
        let err = parse_foreign_file(
            r"
extern libSystem {
    fn getpid() -> i32;
}

garbage

extern sqlite3 {
    fn sqlite3_close(db: *mut void) -> i32;
}
",
        )
        .expect_err("garbage should fail");

        assert!(matches!(
            err,
            ParseError::UnexpectedToken {
                expected: "extern",
                ..
            }
        ));
    }

    #[test]
    fn lower_defaults_to_c_call_conv_when_no_attributes_present() {
        let parsed = parse_foreign_library_decl(
            r"
extern libSystem {
    fn getpid() -> i32;
}
",
        )
        .expect("parse source");

        let lowered = lower_parsed_foreign_library_decl(
            &parsed,
            "/usr/lib/libSystem.B.dylib",
        )
        .expect("lower source");

        assert_eq!(lowered.default_call_conv(), ForeignCallConv::C);
        assert_eq!(lowered.functions()[0].call_conv(), ForeignCallConv::C);
    }

    #[test]
    fn lower_uses_block_call_conv_when_function_has_none() {
        let parsed = parse_foreign_library_decl(
            r"
@call(.C)
extern libSystem {
    fn getpid() -> i32;
}
",
        )
        .expect("parse source");

        let lowered = lower_parsed_foreign_library_decl(
            &parsed,
            "/usr/lib/libSystem.B.dylib",
        )
        .expect("lower source");

        assert_eq!(lowered.default_call_conv(), ForeignCallConv::C);
        assert_eq!(lowered.functions()[0].call_conv(), ForeignCallConv::C);
    }

    #[test]
    fn lower_function_call_conv_overrides_block_call_conv() {
        let parsed = parse_foreign_library_decl(
            r"
@call(.C)
extern libSystem {
    @call(.C)
    fn getpid() -> i32;
}
",
        )
        .expect("parse source");

        let lowered = lower_parsed_foreign_library_decl(
            &parsed,
            "/usr/lib/libSystem.B.dylib",
        )
        .expect("lower source");

        assert_eq!(lowered.functions()[0].call_conv(), ForeignCallConv::C);
    }

    #[test]
    fn duplicate_block_call_conv_attribute_is_rejected() {
        let parsed = parse_foreign_library_decl(
            r"
@call(.C)
@call(.C)
extern libSystem {
    fn getpid() -> i32;
}
",
        )
        .expect("parse source");

        let err = lower_parsed_foreign_library_decl(
            &parsed,
            "/usr/lib/libSystem.B.dylib",
        )
        .expect_err("duplicate block call-conv should fail");

        assert!(matches!(
            err,
            LoweringError::DuplicateCallConvAttribute { .. }
        ));
    }

    #[test]
    fn duplicate_function_call_conv_attribute_is_rejected() {
        let parsed = parse_foreign_library_decl(
            r"
extern libSystem {
    @call(.C)
    @call(.C)
    fn getpid() -> i32;
}
",
        )
        .expect("parse source");

        let err = lower_parsed_foreign_library_decl(
            &parsed,
            "/usr/lib/libSystem.B.dylib",
        )
        .expect_err("duplicate function call-conv should fail");

        assert!(matches!(
            err,
            LoweringError::DuplicateCallConvAttribute { .. }
        ));
    }

    #[test]
    fn parse_then_lower_to_normalized_ir() {
        let src = r"
extern libSystem {
    fn strlen(s: *const void) -> usize;
    fn pid = getpid() -> i32;
}
";
        let parsed = parse_foreign_library_decl(src).expect("parse source");
        let lowered = lower_parsed_foreign_library_decl(
            &parsed,
            "/usr/lib/libSystem.B.dylib",
        )
        .expect("lower parsed declaration");

        assert_eq!(lowered.library_name(), "libSystem");
        assert_eq!(
            lowered.library_path(),
            Path::new("/usr/lib/libSystem.B.dylib")
        );
        assert_eq!(lowered.functions().len(), 2);
        assert_eq!(lowered.functions()[0].local_name(), "strlen");
        assert_eq!(lowered.functions()[0].symbol_name(), "strlen");
        assert_eq!(lowered.functions()[1].local_name(), "pid");
        assert_eq!(lowered.functions()[1].symbol_name(), "getpid");

        assert_eq!(
            lowered.functions()[0].signature().params(),
            &[NativeType::Ptr]
        );
        assert_eq!(lowered.functions()[0].signature().ret(), NativeType::USize);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parse_lower_and_call_integration() {
        let src = r"
extern libSystem {
    fn strlen(s: *const void) -> usize;
    fn pid = getpid() -> i32;
}
";
        let parsed = parse_foreign_library_decl(src).expect("parse source");
        let lowered = lower_parsed_foreign_library_decl(
            &parsed,
            "/usr/lib/libSystem.B.dylib",
        )
        .expect("lower parsed declaration");
        let runtime =
            lower_foreign_library_decl(&lowered).expect("lower to runtime");

        let strlen = runtime.function("strlen").expect("lookup strlen");
        let pid = runtime.function("pid").expect("lookup pid");

        let input = CString::new("hello").expect("literal contains no NUL");
        let strlen_result = strlen
            .call(&[Value::from_c_string(&input)])
            .expect("call strlen");
        let pid_result = pid.call(&[]).expect("call pid");

        match strlen_result {
            Value::USize(len) => assert_eq!(len, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
        match pid_result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }
}
