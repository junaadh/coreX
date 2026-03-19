use crate::lsp::handlers::{handle_notification, handle_request};
use crate::lsp::state::ServerState;
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Write};

pub fn run_stdio_server() -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    run_server(&mut reader, &mut writer)
}

pub fn run_server<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    let mut state = ServerState::new();

    loop {
        let Some(message) = read_message(reader)? else {
            break;
        };
        let method = message.get("method").and_then(Value::as_str);
        let Some(method) = method else {
            continue;
        };
        let params = message.get("params");

        let output = if let Some(id) = message.get("id").cloned() {
            handle_request(&mut state, id, method, params)
        } else {
            handle_notification(&mut state, method, params)
        };

        for outbound in output.outbound {
            write_message(writer, &outbound)?;
        }

        if output.should_exit {
            break;
        }
    }

    Ok(())
}

fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>, String> {
    let mut content_length = None::<usize>;
    loop {
        let mut line = String::new();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| format!("failed reading LSP header: {error}"))?;
        if bytes_read == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            let parsed = value.trim().parse::<usize>().map_err(|error| {
                format!("invalid content-length header `{value}`: {error}")
            })?;
            content_length = Some(parsed);
        }
    }

    let Some(content_length) = content_length else {
        return Err("missing content-length header".to_string());
    };
    let mut payload = vec![0u8; content_length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| format!("failed reading LSP payload: {error}"))?;
    let payload_text = String::from_utf8(payload)
        .map_err(|error| format!("invalid UTF-8 payload: {error}"))?;
    let message: Value = serde_json::from_str(&payload_text)
        .map_err(|error| format!("invalid JSON payload: {error}"))?;
    Ok(Some(message))
}

fn write_message<W: Write>(
    writer: &mut W,
    message: &Value,
) -> Result<(), String> {
    let payload = serde_json::to_vec(message)
        .map_err(|error| format!("failed encoding JSON payload: {error}"))?;
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    writer
        .write_all(header.as_bytes())
        .and_then(|_| writer.write_all(&payload))
        .and_then(|_| writer.flush())
        .map_err(|error| format!("failed writing LSP payload: {error}"))
}

#[cfg(test)]
mod tests {
    use super::run_server;
    use crate::lsp::convert::path_to_uri;
    use serde_json::{Value, json};
    use std::fs;
    use std::io::{BufReader, Cursor};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn frame(message: &Value) -> Vec<u8> {
        let payload = serde_json::to_vec(message).expect("encode payload");
        let header = format!("Content-Length: {}\r\n\r\n", payload.len());
        [header.as_bytes(), payload.as_slice()].concat()
    }

    fn run_with_messages(messages: &[Value]) -> Vec<Value> {
        let input = messages.iter().flat_map(frame).collect::<Vec<_>>();
        let mut reader = BufReader::new(Cursor::new(input));
        let mut output = Vec::new();
        run_server(&mut reader, &mut output).expect("run server");
        parse_frames(&output)
    }

    fn parse_frames(output: &[u8]) -> Vec<Value> {
        let mut cursor = 0usize;
        let mut messages = Vec::new();
        while cursor < output.len() {
            let mut content_length = None::<usize>;
            loop {
                let Some(line_end) = output[cursor..]
                    .windows(2)
                    .position(|window| window == b"\r\n")
                else {
                    return messages;
                };
                let line_end = cursor + line_end;
                let line = &output[cursor..line_end];
                cursor = line_end + 2;
                if line.is_empty() {
                    break;
                }
                let line_text =
                    String::from_utf8(line.to_vec()).expect("header utf8");
                if let Some((name, value)) = line_text.split_once(':')
                    && name.trim().eq_ignore_ascii_case("content-length")
                {
                    content_length = Some(
                        value.trim().parse().expect("numeric content length"),
                    );
                }
            }
            let Some(length) = content_length else {
                break;
            };
            let end = cursor + length;
            let payload = &output[cursor..end];
            cursor = end;
            messages.push(
                serde_json::from_slice(payload).expect("decode payload json"),
            );
        }
        messages
    }

    fn initialize_message(id: i64) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "capabilities": {}
            }
        })
    }

    fn initialized_message() -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        })
    }

    fn shutdown_message(id: i64) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "shutdown",
            "params": Value::Null
        })
    }

    fn exit_message() -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": Value::Null
        })
    }

    fn did_open_message(uri: &str, text: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "corex",
                    "version": 1,
                    "text": text
                }
            }
        })
    }

    fn did_change_message(uri: &str, text: &str, version: i64) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "version": version
                },
                "contentChanges": [
                    { "text": text }
                ]
            }
        })
    }

    fn document_symbol_message(id: i64, uri: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/documentSymbol",
            "params": {
                "textDocument": { "uri": uri }
            }
        })
    }

    fn hover_message(id: i64, uri: &str, line: u32, character: u32) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }
        })
    }

    fn definition_message(
        id: i64,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }
        })
    }

    fn completion_message(
        id: i64,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }
        })
    }

    fn inlay_hint_message(
        id: i64,
        uri: &str,
        start_line: u32,
        start_character: u32,
        end_line: u32,
        end_character: u32,
    ) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/inlayHint",
            "params": {
                "textDocument": { "uri": uri },
                "range": {
                    "start": {
                        "line": start_line,
                        "character": start_character
                    },
                    "end": {
                        "line": end_line,
                        "character": end_character
                    }
                }
            }
        })
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "corex_lsp_{name}_{}_{}",
            std::process::id(),
            nonce
        ));
        fs::create_dir_all(&path).expect("create temp directory");
        path
    }

    fn write_file(path: &Path, source: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, source).expect("write file");
    }

    fn find_response(messages: &[Value], id: i64) -> Option<&Value> {
        messages.iter().find(|message| {
            message.get("id").and_then(Value::as_i64) == Some(id)
        })
    }

    fn find_publish_diagnostics<'a>(
        messages: &'a [Value],
        uri: &str,
    ) -> Option<&'a Value> {
        messages.iter().find(|message| {
            message.get("method").and_then(Value::as_str)
                == Some("textDocument/publishDiagnostics")
                && message
                    .get("params")
                    .and_then(|params| params.get("uri"))
                    .and_then(Value::as_str)
                    == Some(uri)
        })
    }

    #[test]
    fn initialize_shutdown_roundtrip() {
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            shutdown_message(2),
            exit_message(),
        ]);
        let initialize =
            find_response(&messages, 1).expect("initialize response");
        assert!(initialize.get("result").is_some());
        let shutdown = find_response(&messages, 2).expect("shutdown response");
        assert_eq!(shutdown.get("result"), Some(&Value::Null));
    }

    #[test]
    fn did_open_publishes_diagnostics() {
        let file_path = unique_temp_dir("did_open").join("standalone.cx");
        let uri = path_to_uri(&file_path);
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, "fn broken( {"),
            shutdown_message(2),
            exit_message(),
        ]);
        let diagnostics =
            find_publish_diagnostics(&messages, &uri).expect("diagnostics");
        let count = diagnostics["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .len();
        assert!(count > 0);
    }

    #[test]
    fn did_open_reports_unknown_macro_diagnostic_in_standalone_analysis() {
        let file_path = unique_temp_dir("did_open_unknown_macro")
            .join("standalone_macro.cx");
        let uri = path_to_uri(&file_path);
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, "fn main() { @missing(1); }\n"),
            shutdown_message(2),
            exit_message(),
        ]);
        let diagnostics =
            find_publish_diagnostics(&messages, &uri).expect("diagnostics");
        let rendered = diagnostics["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .filter_map(|entry| entry.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            rendered
                .iter()
                .any(|message| message.contains("unknown macro")),
            "standalone LSP analysis should run expansion and report unknown macros"
        );
    }

    #[test]
    fn did_change_updates_diagnostics_from_in_memory_text() {
        let file_path = unique_temp_dir("did_change").join("standalone.cx");
        let uri = path_to_uri(&file_path);
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, "fn broken( {"),
            did_change_message(&uri, "fn ok() {}", 2),
            shutdown_message(2),
            exit_message(),
        ]);

        let diagnostics = messages
            .iter()
            .filter(|message| {
                message.get("method").and_then(Value::as_str)
                    == Some("textDocument/publishDiagnostics")
            })
            .collect::<Vec<_>>();
        assert!(diagnostics.len() >= 2);
        let first_count = diagnostics[0]["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .len();
        let second_count = diagnostics[1]["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .len();
        assert!(first_count > 0);
        assert_eq!(second_count, 0);
    }

    #[test]
    fn document_symbol_returns_top_level_items() {
        let file_path = unique_temp_dir("symbols").join("symbols.cx");
        let uri = path_to_uri(&file_path);
        let source = "fn run() {}\nstruct Client {}\nenum Mode { Fast }\nprotocol Api {}\n";
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, source),
            document_symbol_message(3, &uri),
            shutdown_message(2),
            exit_message(),
        ]);

        let response =
            find_response(&messages, 3).expect("documentSymbol response");
        let symbols = response["result"].as_array().expect("symbol array");
        let names = symbols
            .iter()
            .filter_map(|symbol| symbol.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(names.contains(&"run"));
        assert!(names.contains(&"Client"));
        assert!(names.contains(&"Mode"));
        assert!(names.contains(&"Api"));
    }

    #[test]
    fn hover_returns_useful_info_for_item() {
        let file_path = unique_temp_dir("hover").join("hover.cx");
        let uri = path_to_uri(&file_path);
        let source = "struct Client {}\nfn make(c: Client) -> Client { c }\n";
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, source),
            hover_message(4, &uri, 1, 11),
            shutdown_message(2),
            exit_message(),
        ]);
        let response = find_response(&messages, 4).expect("hover response");
        let value = response["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_default();
        assert!(value.contains("struct"));
        assert!(value.contains("Client"));
    }

    #[test]
    fn definition_resolves_item_reference() {
        let file_path = unique_temp_dir("definition").join("definition.cx");
        let uri = path_to_uri(&file_path);
        let source = "struct Client {}\nfn make(c: Client) -> Client { c }\n";
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, source),
            definition_message(5, &uri, 1, 11),
            shutdown_message(2),
            exit_message(),
        ]);
        let response =
            find_response(&messages, 5).expect("definition response");
        let locations = response["result"].as_array().expect("locations array");
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].get("uri").and_then(Value::as_str),
            Some(uri.as_str())
        );
        let line = locations[0]["range"]["start"]["line"]
            .as_u64()
            .expect("line as u64");
        assert_eq!(line, 0);
    }

    #[test]
    fn definition_resolves_local_reference() {
        let file_path = unique_temp_dir("definition_local").join("local.cx");
        let uri = path_to_uri(&file_path);
        let source = "fn main() {\n  let value = 1;\n  value\n}\n";
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, source),
            definition_message(5, &uri, 2, 3),
            shutdown_message(2),
            exit_message(),
        ]);
        let response =
            find_response(&messages, 5).expect("definition response");
        let locations = response["result"].as_array().expect("locations array");
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].get("uri").and_then(Value::as_str),
            Some(uri.as_str())
        );
        let line = locations[0]["range"]["start"]["line"]
            .as_u64()
            .expect("line as u64");
        assert_eq!(line, 1);
    }

    #[test]
    fn definition_resolves_binary_import_into_library_target() {
        let root = unique_temp_dir("definition_binary_library");
        write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
        write_file(&root.join("src/root.cx"), "pub fn shared_logic() {}\n");
        write_file(
            &root.join("src/main.cx"),
            "use app::shared_logic;\nfn main() { shared_logic(); }\n",
        );
        let main_path = root.join("src/main.cx");
        let uri = path_to_uri(&main_path);
        let source = fs::read_to_string(&main_path)
            .expect("read main source for didOpen");
        let call_line = source
            .lines()
            .position(|line| line.contains("shared_logic();"))
            .expect("shared_logic call line") as u32;
        let call_col = source
            .lines()
            .nth(call_line as usize)
            .and_then(|line| line.find("shared_logic"))
            .expect("shared_logic call column") as u32;

        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, &source),
            definition_message(5, &uri, call_line, call_col),
            shutdown_message(2),
            exit_message(),
        ]);
        let response =
            find_response(&messages, 5).expect("definition response");
        let locations = response["result"].as_array().expect("locations array");
        assert!(!locations.is_empty());
        let uri = locations[0]
            .get("uri")
            .and_then(Value::as_str)
            .expect("uri string");
        assert!(
            uri.ends_with("/src/root.cx"),
            "expected library root definition uri, got {uri}"
        );
    }

    #[test]
    fn definition_resolves_binary_qualified_library_call_into_root() {
        let root = unique_temp_dir("definition_binary_library_qualified");
        write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
        write_file(&root.join("src/root.cx"), "pub fn shared_logic() {}\n");
        write_file(
            &root.join("src/main.cx"),
            "fn main() {\n  app::shared_logic();\n}\n",
        );
        let main_path = root.join("src/main.cx");
        let uri = path_to_uri(&main_path);
        let source = fs::read_to_string(&main_path)
            .expect("read main source for didOpen");

        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, &source),
            definition_message(5, &uri, 1, 10),
            shutdown_message(2),
            exit_message(),
        ]);
        let response =
            find_response(&messages, 5).expect("definition response");
        let locations = response["result"].as_array().expect("locations array");
        assert!(!locations.is_empty());
        let uri = locations[0]
            .get("uri")
            .and_then(Value::as_str)
            .expect("uri string");
        assert!(
            uri.ends_with("/src/root.cx"),
            "expected qualified call definition uri to point at root.cx, got {uri}"
        );
    }

    #[test]
    fn definition_resolves_namespaced_extern_function() {
        let file_path =
            unique_temp_dir("definition_extern").join("extern_def.cx");
        let uri = path_to_uri(&file_path);
        let source = "@call(.C)\nextern libc {\n  fn malloc(size: usize) -> *mut void;\n}\nfn main() { libc::malloc(1); }\n";

        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, source),
            definition_message(5, &uri, 4, 19),
            shutdown_message(2),
            exit_message(),
        ]);
        let response =
            find_response(&messages, 5).expect("definition response");
        let locations = response["result"].as_array().expect("locations array");
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].get("uri").and_then(Value::as_str),
            Some(uri.as_str())
        );
        let line = locations[0]["range"]["start"]["line"]
            .as_u64()
            .expect("line as u64");
        assert_eq!(line, 2);
    }

    #[test]
    fn definition_resolves_dependency_root_item_in_project_context() {
        let workspace = unique_temp_dir("definition_dependency_root");
        let app_dir = workspace.join("app");
        let util_dir = workspace.join("util");

        write_file(
            &app_dir.join("corex.toml"),
            "[project]\nname = \"app\"\n\n[dependencies]\nutil = { path = \"../util\" }\n",
        );
        write_file(
            &app_dir.join("src/main.cx"),
            "fn main() { util::shared_logic(); }\n",
        );
        write_file(
            &util_dir.join("corex.toml"),
            "[project]\nname = \"utility\"\n",
        );
        write_file(&util_dir.join("src/root.cx"), "pub fn shared_logic() {}\n");

        let main_path = app_dir.join("src/main.cx");
        let uri = path_to_uri(&main_path);
        let source = fs::read_to_string(&main_path)
            .expect("read main source for didOpen");
        let call_line = source
            .lines()
            .position(|line| line.contains("util::shared_logic();"))
            .expect("shared_logic call line") as u32;
        let call_col = source
            .lines()
            .nth(call_line as usize)
            .and_then(|line| line.find("shared_logic"))
            .expect("shared_logic call column") as u32;

        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, &source),
            definition_message(5, &uri, call_line, call_col),
            shutdown_message(2),
            exit_message(),
        ]);
        let response =
            find_response(&messages, 5).expect("definition response");
        let locations = response["result"].as_array().expect("locations array");
        assert!(!locations.is_empty());
        let uri = locations[0]
            .get("uri")
            .and_then(Value::as_str)
            .expect("uri string");
        assert!(
            uri.ends_with("/util/src/root.cx"),
            "expected dependency definition uri to point at util root, got {uri}"
        );
    }

    #[test]
    fn completion_returns_locals_and_items() {
        let file_path = unique_temp_dir("completion").join("completion.cx");
        let uri = path_to_uri(&file_path);
        let source = "fn helper() {}\nfn main() {\n  let value = 1;\n  \n}\n";
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, source),
            completion_message(7, &uri, 3, 2),
            shutdown_message(2),
            exit_message(),
        ]);
        let response =
            find_response(&messages, 7).expect("completion response");
        let items = response["result"].as_array().expect("completion items");
        assert!(
            items
                .iter()
                .any(|item| item["label"].as_str() == Some("value")),
            "completion should include local bindings"
        );
        assert!(
            items
                .iter()
                .any(|item| item["label"].as_str() == Some("helper")),
            "completion should include visible top-level items"
        );
    }

    #[test]
    fn inlay_hint_reports_inferred_local_type() {
        let file_path = unique_temp_dir("inlay_hint").join("inlay.cx");
        let uri = path_to_uri(&file_path);
        let source = "fn main() {\n  let value = 1;\n  value\n}\n";
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, source),
            inlay_hint_message(8, &uri, 0, 0, 3, 1),
            shutdown_message(2),
            exit_message(),
        ]);
        let response =
            find_response(&messages, 8).expect("inlay hint response");
        let hints = response["result"].as_array().expect("inlay hints");
        assert!(
            hints.iter().any(|hint| {
                hint["label"]
                    .as_str()
                    .is_some_and(|label| label.contains(": i32"))
            }),
            "inlay hints should include inferred local type annotations"
        );
    }

    #[test]
    fn standalone_file_fallback_works_without_project() {
        let file_path = unique_temp_dir("standalone").join("standalone.cx");
        let uri = path_to_uri(&file_path);
        let source = "fn run() {}\n";
        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, source),
            document_symbol_message(6, &uri),
            shutdown_message(2),
            exit_message(),
        ]);
        let response =
            find_response(&messages, 6).expect("documentSymbol response");
        let symbols = response["result"].as_array().expect("symbol array");
        assert!(
            symbols
                .iter()
                .any(|symbol| symbol["name"].as_str() == Some("run"))
        );
    }

    #[test]
    fn project_context_analysis_works_for_files_inside_project() {
        let root = unique_temp_dir("project_context");
        write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
        write_file(&root.join("src/root.cx"), "scope net;\n");
        write_file(&root.join("src/net.cx"), "struct Client {}\n");
        write_file(
            &root.join("src/main.cx"),
            "use app::net::Client;\nfn main() {}\n",
        );
        let main_path = root.join("src/main.cx");
        let uri = path_to_uri(&main_path);
        let source = fs::read_to_string(&main_path)
            .expect("read main source for didOpen");

        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, &source),
            shutdown_message(2),
            exit_message(),
        ]);
        let diagnostics =
            find_publish_diagnostics(&messages, &uri).expect("diagnostics");
        let count = diagnostics["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .len();
        assert_eq!(count, 0);
    }

    #[test]
    fn project_binary_can_call_library_import_without_invalid_call_target() {
        let root = unique_temp_dir("project_lib_call");
        write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
        write_file(&root.join("src/root.cx"), "pub fn shared_logic() {}\n");
        write_file(
            &root.join("src/main.cx"),
            "use app::shared_logic;\nfn main() { shared_logic(); }\n",
        );
        let main_path = root.join("src/main.cx");
        let uri = path_to_uri(&main_path);
        let source = fs::read_to_string(&main_path)
            .expect("read main source for didOpen");

        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, &source),
            shutdown_message(2),
            exit_message(),
        ]);
        let diagnostics =
            find_publish_diagnostics(&messages, &uri).expect("diagnostics");
        let rendered = diagnostics["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .filter_map(|entry| entry.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            !rendered
                .iter()
                .any(|message| message.contains("invalid call target")),
            "library bridge call should not emit invalid call target"
        );
    }

    #[test]
    fn project_binary_can_call_library_with_qualified_named_root_path() {
        let root = unique_temp_dir("project_lib_qualified_call");
        write_file(
            &root.join("corex.toml"),
            "[project]\nname = \"lib_and_bin\"\n",
        );
        write_file(&root.join("src/root.cx"), "pub fn shared_logic() {}\n");
        write_file(
            &root.join("src/main.cx"),
            "fn main() { lib_and_bin::shared_logic(); }\n",
        );
        let main_path = root.join("src/main.cx");
        let uri = path_to_uri(&main_path);
        let source = fs::read_to_string(&main_path)
            .expect("read main source for didOpen");

        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, &source),
            shutdown_message(2),
            exit_message(),
        ]);
        let diagnostics =
            find_publish_diagnostics(&messages, &uri).expect("diagnostics");
        let rendered = diagnostics["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .filter_map(|entry| entry.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            !rendered
                .iter()
                .any(|message| message.contains("invalid call target")),
            "qualified named-root call into library target should be callable"
        );
    }

    #[test]
    fn project_namespaced_extern_call_does_not_emit_call_target_or_arity_errors()
     {
        let root = unique_temp_dir("project_extern_namespaced_call");
        write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
        write_file(
            &root.join("src/main.cx"),
            "@call(.C)\nextern libc {\n  fn malloc(size: usize) -> *mut void;\n}\nfn main() { libc::malloc(1); }\n",
        );
        let main_path = root.join("src/main.cx");
        let uri = path_to_uri(&main_path);
        let source = fs::read_to_string(&main_path)
            .expect("read main source for didOpen");

        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, &source),
            shutdown_message(2),
            exit_message(),
        ]);
        let diagnostics =
            find_publish_diagnostics(&messages, &uri).expect("diagnostics");
        let rendered = diagnostics["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .filter_map(|entry| entry.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            !rendered
                .iter()
                .any(|message| message.contains("invalid call target")),
            "namespaced extern call should be callable"
        );
        assert!(
            !rendered
                .iter()
                .any(|message| message.contains("invalid call arity")),
            "namespaced extern call should not report arity mismatch"
        );
    }

    #[test]
    fn project_analysis_expands_macros_before_semantic_checks() {
        let root = unique_temp_dir("project_macro_expansion_semantic");
        write_file(&root.join("corex.toml"), "[project]\nname = \"app\"\n");
        write_file(
            &root.join("src/main.cx"),
            "macro call_malloc {\n  rule(size: Expr) => { malloc(size) };\n}\n@call(.C)\nextern libc {\n  fn malloc(size: usize) -> *mut void;\n}\nfn main() { @call_malloc(1); }\n",
        );
        let main_path = root.join("src/main.cx");
        let uri = path_to_uri(&main_path);
        let source = fs::read_to_string(&main_path)
            .expect("read main source for didOpen");

        let messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&uri, &source),
            shutdown_message(2),
            exit_message(),
        ]);
        let diagnostics =
            find_publish_diagnostics(&messages, &uri).expect("diagnostics");
        let rendered = diagnostics["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .filter_map(|entry| entry.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            rendered
                .iter()
                .any(|message| message.contains("invalid extern call target")),
            "semantic analysis should observe expanded macro output in project mode"
        );
        assert!(
            !rendered
                .iter()
                .any(|message| message.contains("unknown macro")),
            "macro invocation should be consumed by expansion before semantic analysis"
        );
    }

    #[test]
    fn namespaced_extern_call_is_valid_but_bare_call_reports_error() {
        let namespaced_path =
            unique_temp_dir("extern_namespaced").join("extern_ns.cx");
        let namespaced_uri = path_to_uri(&namespaced_path);
        let namespaced_source = "@call(.C)\nextern libc {\n  fn malloc(size: usize) -> *mut void;\n}\nfn main() { libc::malloc(1); }\n";
        let namespaced_messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&namespaced_uri, namespaced_source),
            shutdown_message(2),
            exit_message(),
        ]);
        let namespaced_diag =
            find_publish_diagnostics(&namespaced_messages, &namespaced_uri)
                .expect("namespaced diagnostics");
        let namespaced_messages = namespaced_diag["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .filter_map(|entry| entry.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            !namespaced_messages
                .iter()
                .any(|message| message.contains("invalid call target")),
            "namespaced extern call should be callable"
        );

        let bare_path = unique_temp_dir("extern_bare").join("extern_bare.cx");
        let bare_uri = path_to_uri(&bare_path);
        let bare_source = "@call(.C)\nextern libc {\n  fn malloc(size: usize) -> *mut void;\n}\nfn main() { malloc(1); }\n";
        let bare_messages = run_with_messages(&[
            initialize_message(1),
            initialized_message(),
            did_open_message(&bare_uri, bare_source),
            shutdown_message(2),
            exit_message(),
        ]);
        let bare_diag = find_publish_diagnostics(&bare_messages, &bare_uri)
            .expect("bare diagnostics");
        let bare_messages = bare_diag["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .filter_map(|entry| entry.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            bare_messages.iter().any(|message| {
                message.contains("invalid extern call target")
                    || message.contains("must be called through")
            }),
            "bare extern call should emit dedicated namespace diagnostic"
        );
    }
}
