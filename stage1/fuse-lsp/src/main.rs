use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;

use fusec::{Diagnostic, Severity, Span};
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
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

#[derive(Serialize)]
struct Notification {
    jsonrpc: String,
    method: String,
    params: Value,
}

// ---------------------------------------------------------------------------
// LSP capability types
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
// LSP diagnostic types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct LspDiagnostic {
    range: LspRange,
    severity: i32,
    message: String,
    source: String,
}

#[derive(Serialize)]
struct LspRange {
    start: LspPosition,
    end: LspPosition,
}

#[derive(Serialize)]
struct LspPosition {
    line: u32,
    character: u32,
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut content_length: Option<usize> = None;
    let mut header = String::new();

    loop {
        header.clear();
        let n = reader.read_line(&mut header)?;
        if n == 0 {
            return Ok(None);
        }
        let line = header.trim();
        if line.is_empty() {
            break;
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
        error: Some(RpcError {
            code,
            message: message.into(),
        }),
    };
    write_message(writer, &serde_json::to_string(&resp).unwrap())
}

fn send_notification(writer: &mut impl Write, method: &str, params: Value) -> io::Result<()> {
    let notif = Notification {
        jsonrpc: "2.0".into(),
        method: method.into(),
        params,
    };
    write_message(writer, &serde_json::to_string(&notif).unwrap())
}

// ---------------------------------------------------------------------------
// Diagnostic conversion: fusec → LSP
// ---------------------------------------------------------------------------

fn span_to_lsp_range(span: Span) -> LspRange {
    // fusec Span is 1-based; LSP is 0-based.
    let line = span.line.saturating_sub(1) as u32;
    let col = span.column.saturating_sub(1) as u32;
    LspRange {
        start: LspPosition { line, character: col },
        end: LspPosition { line, character: col },
    }
}

fn severity_to_lsp(severity: Severity) -> i32 {
    match severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Note => 3,
    }
}

fn convert_diagnostic(diag: &Diagnostic) -> LspDiagnostic {
    let mut msg = diag.message().to_string();
    if let Some(hint) = diag.hint() {
        msg.push_str("\n");
        msg.push_str(hint);
    }
    LspDiagnostic {
        range: span_to_lsp_range(diag.span),
        severity: severity_to_lsp(diag.severity),
        message: msg,
        source: "fusec".into(),
    }
}

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

struct Server {
    initialized: bool,
    shutdown_requested: bool,
    documents: HashMap<String, String>, // uri → content
}

impl Server {
    fn new() -> Self {
        Self {
            initialized: false,
            shutdown_requested: false,
            documents: HashMap::new(),
        }
    }

    fn handle_message(&mut self, msg: Message, out: &mut impl Write) -> io::Result<bool> {
        let method = msg.method.as_deref().unwrap_or("");

        match method {
            "initialize" => {
                let id = msg.id.unwrap_or(Value::Null);
                let result = InitializeResult {
                    capabilities: ServerCapabilities {
                        text_document_sync: 1, // Full
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
            "initialized" => {}
            "shutdown" => {
                let id = msg.id.unwrap_or(Value::Null);
                send_response(out, id, Value::Null)?;
                self.shutdown_requested = true;
                eprintln!("[fuse-lsp] shutdown acknowledged");
            }
            "exit" => {
                eprintln!("[fuse-lsp] exit");
                return Ok(true);
            }

            // --- Document sync ---
            "textDocument/didOpen" => {
                if let Some(params) = msg.params {
                    if let (Some(uri), Some(text)) = (
                        params.pointer("/textDocument/uri").and_then(Value::as_str),
                        params.pointer("/textDocument/text").and_then(Value::as_str),
                    ) {
                        self.documents.insert(uri.to_string(), text.to_string());
                        self.publish_diagnostics(uri, out)?;
                    }
                }
            }
            "textDocument/didChange" => {
                if let Some(params) = msg.params {
                    if let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) {
                        // Full sync: contentChanges[0].text is the entire document.
                        if let Some(text) = params
                            .pointer("/contentChanges/0/text")
                            .and_then(Value::as_str)
                        {
                            self.documents.insert(uri.to_string(), text.to_string());
                            self.publish_diagnostics(uri, out)?;
                        }
                    }
                }
            }
            "textDocument/didClose" => {
                if let Some(params) = msg.params {
                    if let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) {
                        self.documents.remove(uri);
                    }
                }
            }

            _ => {
                if let Some(id) = msg.id {
                    send_error(out, id, -32601, &format!("method not found: {method}"))?;
                }
            }
        }

        Ok(false)
    }

    /// Run the checker on a document and send `publishDiagnostics`.
    fn publish_diagnostics(&self, uri: &str, out: &mut impl Write) -> io::Result<()> {
        let lsp_diags = if let Some(content) = self.documents.get(uri) {
            self.check_document(uri, content)
        } else {
            Vec::new()
        };

        let params = serde_json::json!({
            "uri": uri,
            "diagnostics": lsp_diags.iter().map(|d| serde_json::to_value(d).unwrap()).collect::<Vec<_>>(),
        });
        send_notification(out, "textDocument/publishDiagnostics", params)
    }

    /// Write content to a temp file, run fusec checker, return LSP diagnostics.
    fn check_document(&self, uri: &str, content: &str) -> Vec<LspDiagnostic> {
        let path = uri_to_path(uri);

        // Write content to a temp file next to the original so imports resolve.
        let temp_path = path
            .parent()
            .unwrap_or(&path)
            .join(format!("__fuse_lsp_check__{}", path.file_name().unwrap_or_default().to_string_lossy()));

        if std::fs::write(&temp_path, content).is_err() {
            return Vec::new();
        }

        let diagnostics = fusec::check_path(&temp_path);
        let _ = std::fs::remove_file(&temp_path);

        diagnostics.iter().map(convert_diagnostic).collect()
    }
}

/// Convert a `file:///...` URI to a local `PathBuf`.
fn uri_to_path(uri: &str) -> PathBuf {
    if let Some(rest) = uri.strip_prefix("file:///") {
        // On Windows: file:///C:/foo → C:/foo
        // On Unix:    file:///home/foo → /home/foo
        let decoded = percent_decode(rest);
        if cfg!(windows) {
            PathBuf::from(decoded)
        } else {
            PathBuf::from(format!("/{decoded}"))
        }
    } else {
        PathBuf::from(uri)
    }
}

/// Minimal percent-decoding for file URIs (spaces, common chars).
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hi = chars.next().unwrap_or('0');
            let lo = chars.next().unwrap_or('0');
            let byte = u8::from_str_radix(&format!("{hi}{lo}"), 16).unwrap_or(b'?');
            out.push(byte as char);
        } else {
            out.push(ch);
        }
    }
    out
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
            Ok(None) => break,
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

        let _ = msg.jsonrpc;

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
