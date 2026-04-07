use std::io::{self, BufRead, Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Message {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Serialize)]
struct Response {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ResponseError>,
}

#[derive(Serialize)]
struct ResponseError {
    code: i32,
    message: String,
}

// ---------------------------------------------------------------------------
// LSP capability types (minimal)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResult {
    capabilities: ServerCapabilities,
    server_info: ServerInfo,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerCapabilities {
    text_document_sync: i32,
}

#[derive(Serialize)]
struct ServerInfo {
    name: String,
    version: String,
}

// ---------------------------------------------------------------------------
// Transport: read/write LSP base-protocol messages on stdio
// ---------------------------------------------------------------------------

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut content_length: Option<usize> = None;
    let mut header = String::new();

    loop {
        header.clear();
        let n = reader.read_line(&mut header)?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let line = header.trim();
        if line.is_empty() {
            break; // blank line terminates headers
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            content_length = value.parse().ok();
        }
    }

    let len = content_length.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
    })?;

    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    String::from_utf8(body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn write_message(writer: &mut impl Write, body: &str) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    writer.flush()
}

fn send_response(writer: &mut impl Write, id: Value, result: Value) -> io::Result<()> {
    let resp = Response {
        jsonrpc: "2.0".into(),
        id,
        result: Some(result),
        error: None,
    };
    write_message(writer, &serde_json::to_string(&resp).unwrap())
}

fn send_error(writer: &mut impl Write, id: Value, code: i32, message: &str) -> io::Result<()> {
    let resp = Response {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(ResponseError {
            code,
            message: message.into(),
        }),
    };
    write_message(writer, &serde_json::to_string(&resp).unwrap())
}

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

struct Server {
    initialized: bool,
    shutdown_requested: bool,
}

impl Server {
    fn new() -> Self {
        Self {
            initialized: false,
            shutdown_requested: false,
        }
    }

    fn handle_message(&mut self, msg: Message, out: &mut impl Write) -> io::Result<bool> {
        let method = msg.method.as_deref().unwrap_or("");

        match method {
            "initialize" => {
                let id = msg.id.unwrap_or(Value::Null);
                let result = InitializeResult {
                    capabilities: ServerCapabilities {
                        text_document_sync: 1, // Full sync
                    },
                    server_info: ServerInfo {
                        name: "fuse-lsp".into(),
                        version: "0.1.0".into(),
                    },
                };
                send_response(out, id, serde_json::to_value(result).unwrap())?;
                self.initialized = true;
                eprintln!("[fuse-lsp] initialized");
            }
            "initialized" => {
                // Notification — no response required.
            }
            "shutdown" => {
                let id = msg.id.unwrap_or(Value::Null);
                send_response(out, id, Value::Null)?;
                self.shutdown_requested = true;
                eprintln!("[fuse-lsp] shutdown acknowledged");
            }
            "exit" => {
                eprintln!("[fuse-lsp] exit");
                return Ok(true); // signal to stop the loop
            }
            _ => {
                if let Some(id) = msg.id {
                    // Unknown request — reply with MethodNotFound.
                    send_error(out, id, -32601, &format!("method not found: {method}"))?;
                }
                // Unknown notification — silently ignore.
            }
        }

        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    eprintln!("[fuse-lsp] starting");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut writer = io::BufWriter::new(stdout.lock());
    let mut server = Server::new();

    loop {
        let body = match read_message(&mut reader) {
            Ok(Some(body)) => body,
            Ok(None) => break, // EOF
            Err(e) => {
                eprintln!("[fuse-lsp] read error: {e}");
                break;
            }
        };

        let msg: Message = match serde_json::from_str(&body) {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("[fuse-lsp] parse error: {e}");
                continue;
            }
        };

        let _ = msg.jsonrpc; // acknowledged but not validated

        match server.handle_message(msg, &mut writer) {
            Ok(true) => break,
            Ok(false) => {}
            Err(e) => {
                eprintln!("[fuse-lsp] write error: {e}");
                break;
            }
        }
    }

    let exit_code = if server.shutdown_requested { 0 } else { 1 };
    eprintln!("[fuse-lsp] exiting with code {exit_code}");
    std::process::exit(exit_code);
}
