use core_x::frontend::ast::Span;
use core_x::frontend::source::SourceFile;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[must_use]
pub fn path_to_uri(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(path)
    } else {
        path.to_path_buf()
    };
    let raw = absolute.to_string_lossy().replace('\\', "/");
    format!("file://{}", percent_encode(&raw))
}

#[must_use]
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let value = uri.strip_prefix("file://")?;
    let decoded = percent_decode(value)?;
    Some(PathBuf::from(decoded))
}

#[must_use]
pub fn span_to_lsp_range(file: &SourceFile, span: Span) -> LspRange {
    let start = offset_to_position(file, span.start);
    let end = offset_to_position(file, span.end);
    LspRange { start, end }
}

#[must_use]
pub fn offset_to_position(file: &SourceFile, offset: usize) -> LspPosition {
    let clamped = offset.min(file.len());
    let Some(line_col) = file.line_col(clamped) else {
        return LspPosition {
            line: 0,
            character: 0,
        };
    };
    let line_start = file
        .line_index()
        .line_start(line_col.line)
        .unwrap_or_default();
    let prefix = &file.source()[line_start..clamped];
    LspPosition {
        line: u32::try_from(line_col.line).unwrap_or(u32::MAX),
        character: u32::try_from(prefix.encode_utf16().count())
            .unwrap_or(u32::MAX),
    }
}

#[must_use]
pub fn position_to_offset(
    file: &SourceFile,
    position: LspPosition,
) -> Option<usize> {
    let line = usize::try_from(position.line).ok()?;
    let utf16_column = usize::try_from(position.character).ok()?;
    let line_start = file.line_index().line_start(line)?;
    let line_end = if line + 1 < file.line_index().line_count() {
        file.line_index().line_start(line + 1)?
    } else {
        file.len()
    };
    let line_text = &file.source()[line_start..line_end];

    let mut utf16_seen = 0usize;
    let mut byte_offset = 0usize;
    for ch in line_text.chars() {
        if utf16_seen >= utf16_column {
            break;
        }
        utf16_seen += ch.len_utf16();
        byte_offset += ch.len_utf8();
    }

    Some(line_start + byte_offset)
}

#[must_use]
pub fn word_span_at_position(
    file: &SourceFile,
    position: LspPosition,
) -> Option<(String, Span)> {
    let mut offset = position_to_offset(file, position)?;
    let bytes = file.source().as_bytes();
    if offset >= bytes.len() {
        if bytes.is_empty() {
            return None;
        }
        offset = bytes.len() - 1;
    }

    if !is_ident_byte(*bytes.get(offset)?) {
        return None;
    }

    let mut start = offset;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset + 1;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }

    let text = file.source().get(start..end)?.to_string();
    Some((text, Span::new(start, end)))
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn percent_encode(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'_') {
            out.push(char::from(b));
        } else {
            let _ =
                std::fmt::Write::write_fmt(&mut out, format_args!("%{b:02X}"));
        }
    }
    out
}

fn percent_decode(input: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut idx = 0usize;
    let source = input.as_bytes();
    while idx < source.len() {
        if source[idx] == b'%' {
            let hi = *source.get(idx + 1)?;
            let lo = *source.get(idx + 2)?;
            let hi = hex_value(hi)?;
            let lo = hex_value(lo)?;
            bytes.push((hi << 4) | lo);
            idx += 3;
        } else {
            bytes.push(source[idx]);
            idx += 1;
        }
    }
    String::from_utf8(bytes).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
