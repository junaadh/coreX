use crate::lsp::analysis::{
    analyze_document, completion_for_position, definition_for_position,
    diagnostics_for_document, document_symbols_for_document,
    hover_for_position, inlay_hints_for_range,
};
use crate::lsp::convert::{LspPosition, LspRange};
use crate::lsp::state::ServerState;
use serde_json::{Value, json};

pub struct HandlerOutput {
    pub outbound: Vec<Value>,
    pub should_exit: bool,
}

impl HandlerOutput {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            outbound: Vec::new(),
            should_exit: false,
        }
    }
}

pub fn handle_request(
    state: &mut ServerState,
    id: Value,
    method: &str,
    params: Option<&Value>,
) -> HandlerOutput {
    match method {
        "initialize" => HandlerOutput {
            outbound: vec![json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "capabilities": {
                        "textDocumentSync": {
                            "openClose": true,
                            "change": 1,
                        },
                        "documentSymbolProvider": true,
                        "hoverProvider": true,
                        "definitionProvider": true,
                        "completionProvider": {
                            "resolveProvider": false,
                            "triggerCharacters": [".", ":"]
                        },
                        "inlayHintProvider": true,
                    },
                    "serverInfo": {
                        "name": "corex-lsp",
                        "version": "0.1.0",
                    }
                }
            })],
            should_exit: false,
        },
        "shutdown" => {
            state.mark_shutdown_requested();
            HandlerOutput {
                outbound: vec![json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": Value::Null,
                })],
                should_exit: false,
            }
        }
        "textDocument/documentSymbol" => {
            let Some(uri) = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("uri"))
                .and_then(Value::as_str)
            else {
                return request_error(id, -32602, "missing textDocument.uri");
            };

            match analyze_document(state, uri) {
                Ok(analysis) => HandlerOutput {
                    outbound: vec![json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": document_symbols_for_document(&analysis),
                    })],
                    should_exit: false,
                },
                Err(message) => request_error(id, -32001, &message),
            }
        }
        "textDocument/hover" => {
            let Some(uri) = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("uri"))
                .and_then(Value::as_str)
            else {
                return request_error(id, -32602, "missing textDocument.uri");
            };
            let Some(position) = params
                .and_then(|value| value.get("position"))
                .cloned()
                .and_then(|value| {
                    serde_json::from_value::<LspPosition>(value).ok()
                })
            else {
                return request_error(id, -32602, "missing hover position");
            };

            match analyze_document(state, uri) {
                Ok(analysis) => HandlerOutput {
                    outbound: vec![json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": hover_for_position(&analysis, position).unwrap_or(Value::Null),
                    })],
                    should_exit: false,
                },
                Err(message) => request_error(id, -32001, &message),
            }
        }
        "textDocument/definition" => {
            let Some(uri) = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("uri"))
                .and_then(Value::as_str)
            else {
                return request_error(id, -32602, "missing textDocument.uri");
            };
            let Some(position) = params
                .and_then(|value| value.get("position"))
                .cloned()
                .and_then(|value| {
                    serde_json::from_value::<LspPosition>(value).ok()
                })
            else {
                return request_error(
                    id,
                    -32602,
                    "missing definition position",
                );
            };

            match analyze_document(state, uri) {
                Ok(analysis) => HandlerOutput {
                    outbound: vec![json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": definition_for_position(&analysis, position),
                    })],
                    should_exit: false,
                },
                Err(message) => request_error(id, -32001, &message),
            }
        }
        "textDocument/completion" => {
            let Some(uri) = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("uri"))
                .and_then(Value::as_str)
            else {
                return request_error(id, -32602, "missing textDocument.uri");
            };
            let Some(position) = params
                .and_then(|value| value.get("position"))
                .cloned()
                .and_then(|value| {
                    serde_json::from_value::<LspPosition>(value).ok()
                })
            else {
                return request_error(
                    id,
                    -32602,
                    "missing completion position",
                );
            };

            match analyze_document(state, uri) {
                Ok(analysis) => HandlerOutput {
                    outbound: vec![json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": completion_for_position(&analysis, position),
                    })],
                    should_exit: false,
                },
                Err(message) => request_error(id, -32001, &message),
            }
        }
        "textDocument/inlayHint" => {
            let Some(uri) = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("uri"))
                .and_then(Value::as_str)
            else {
                return request_error(id, -32602, "missing textDocument.uri");
            };
            let Some(range) = params
                .and_then(|value| value.get("range"))
                .cloned()
                .and_then(|value| {
                    serde_json::from_value::<LspRange>(value).ok()
                })
            else {
                return request_error(id, -32602, "missing inlay hint range");
            };

            match analyze_document(state, uri) {
                Ok(analysis) => HandlerOutput {
                    outbound: vec![json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": inlay_hints_for_range(&analysis, range),
                    })],
                    should_exit: false,
                },
                Err(message) => request_error(id, -32001, &message),
            }
        }
        _ => request_error(id, -32601, "method not found"),
    }
}

pub fn handle_notification(
    state: &mut ServerState,
    method: &str,
    params: Option<&Value>,
) -> HandlerOutput {
    match method {
        "initialized" => HandlerOutput::empty(),
        "exit" => HandlerOutput {
            outbound: Vec::new(),
            should_exit: true,
        },
        "textDocument/didOpen" => {
            let Some(uri) = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("uri"))
                .and_then(Value::as_str)
            else {
                return HandlerOutput::empty();
            };
            let Some(text) = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
            else {
                return HandlerOutput::empty();
            };
            let version = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("version"))
                .and_then(Value::as_i64);

            if state
                .upsert_open_document(
                    uri.to_string(),
                    text.to_string(),
                    version,
                )
                .is_err()
            {
                return HandlerOutput::empty();
            }

            publish_document_diagnostics(state, uri)
        }
        "textDocument/didChange" => {
            let Some(uri) = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("uri"))
                .and_then(Value::as_str)
            else {
                return HandlerOutput::empty();
            };
            let version = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("version"))
                .and_then(Value::as_i64);
            let Some(new_text) = params
                .and_then(|value| value.get("contentChanges"))
                .and_then(Value::as_array)
                .and_then(|changes| changes.first())
                .and_then(|change| change.get("text"))
                .and_then(Value::as_str)
            else {
                return HandlerOutput::empty();
            };

            if state
                .update_open_document(uri, new_text.to_string(), version)
                .is_err()
            {
                return HandlerOutput::empty();
            }

            publish_document_diagnostics(state, uri)
        }
        "textDocument/didClose" => {
            let Some(uri) = params
                .and_then(|value| value.get("textDocument"))
                .and_then(|value| value.get("uri"))
                .and_then(Value::as_str)
            else {
                return HandlerOutput::empty();
            };
            state.close_document(uri);
            HandlerOutput {
                outbound: vec![json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": {
                        "uri": uri,
                        "diagnostics": [],
                    }
                })],
                should_exit: false,
            }
        }
        _ => HandlerOutput::empty(),
    }
}

fn request_error(id: Value, code: i64, message: &str) -> HandlerOutput {
    HandlerOutput {
        outbound: vec![json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message,
            }
        })],
        should_exit: false,
    }
}

fn publish_document_diagnostics(
    state: &ServerState,
    uri: &str,
) -> HandlerOutput {
    match analyze_document(state, uri) {
        Ok(analysis) => HandlerOutput {
            outbound: vec![json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": {
                    "uri": analysis.uri,
                    "diagnostics": diagnostics_for_document(&analysis),
                }
            })],
            should_exit: false,
        },
        Err(_) => HandlerOutput::empty(),
    }
}
