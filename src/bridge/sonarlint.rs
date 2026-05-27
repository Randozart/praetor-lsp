use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde::Deserialize;
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::checks::CheckDiagnostic;

use super::{bridge_diagnostic, tool_is_available, Bridge};

#[derive(Debug, Deserialize)]
struct LspMessage {
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<serde_json::Value>,
    #[serde(default)]
    id: Option<serde_json::Value>,
}

pub struct SonarLintBridge;

impl SonarLintBridge {
    fn find_script_path() -> Option<std::path::PathBuf> {
        if std::path::Path::new("scripts/sonar_bridge.py").is_file() {
            return Some(std::path::PathBuf::from("scripts/sonar_bridge.py"));
        }
        if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
            let p = std::path::PathBuf::from(manifest).join("scripts/sonar_bridge.py");
            if p.is_file() {
                return Some(p);
            }
        }
        None
    }

    fn send_message(child: &mut Child, msg: &str) {
        let header = format!("Content-Length: {}\r\n\r\n", msg.len());
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(header.as_bytes());
            let _ = stdin.write_all(msg.as_bytes());
            let _ = stdin.flush();
        }
    }

    fn read_message(child: &mut Child) -> Option<String> {
        let stdout = child.stdout.as_mut()?;
        let mut reader = BufReader::new(stdout);
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let len = header.trim().strip_prefix("Content-Length: ")?.parse::<usize>().ok()?;
        let mut blank = String::new();
        reader.read_line(&mut blank).ok()?;
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).ok()?;
        String::from_utf8(body).ok()
    }
}

impl Bridge for SonarLintBridge {
    fn name(&self) -> &str { "sonarlint" }

    fn is_available(&self) -> bool {
        tool_is_available("java") && Self::find_script_path().is_some()
    }

    fn run(&self, file_path: &Path, source: &[u8]) -> Vec<CheckDiagnostic> {
        let script_path = match Self::find_script_path() {
            Some(p) => p,
            None => {
                tracing::warn!("SonarLint bridge: sonar_bridge.py not found");
                return vec![];
            }
        };

        let mut child = match Command::new("python3")
            .arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("SonarLint bridge: failed to spawn: {}", e);
                return vec![];
            }
        };

        std::thread::sleep(Duration::from_millis(3000));

        match child.try_wait() {
            Ok(Some(status)) => {
                tracing::warn!("SonarLint bridge: process exited early with {}", status);
                return vec![];
            }
            _ => {}
        }

        Self::send_message(&mut child, "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"processId\":null,\"capabilities\":{},\"rootUri\":null}}");

        let _init_resp = match Self::read_message(&mut child) {
            Some(r) => r,
            None => {
                tracing::warn!("SonarLint bridge: no initialize response");
                let _ = child.kill();
                return vec![];
            }
        };

        Self::send_message(&mut child, "{\"jsonrpc\":\"2.0\",\"method\":\"initialized\",\"params\":{}}");

let source_text = String::from_utf8_lossy(source);
        let escaped_source = format!("\"{}\"", source_text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t"));
        let uri = format!("file://{}", file_path.display());
        let did_open = format!("{{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{{\"textDocument\":{{\"uri\":\"{}\",\"languageId\":\"\",\"version\":1,\"text\":{}}}}}}}", uri, escaped_source);
        Self::send_message(&mut child, &did_open);

        let mut diags = Vec::new();
        for _ in 0..5 {
            let msg = match Self::read_message(&mut child) {
                Some(m) => m,
                None => break,
            };
            let parsed: LspMessage = match serde_json::from_str(&msg) {
                Ok(m) => m,
                Err(_) => continue,
            };

            if parsed.method.as_deref() == Some("textDocument/publishDiagnostics") {
                if let Some(params) = &parsed.params {
                    if let Some(diag_array) = params.get("diagnostics").and_then(|d| d.as_array()) {
                        for d in diag_array {
                            let line = d.get("range")
                                .and_then(|r| r.get("start"))
                                .and_then(|s| s.get("line").and_then(|l| l.as_u64()))
                                .unwrap_or(0) as u32;
                            let col = d.get("range")
                                .and_then(|r| r.get("start"))
                                .and_then(|s| s.get("character").and_then(|c| c.as_u64()))
                                .unwrap_or(0) as u32;
                            let message = d.get("message").and_then(|m| m.as_str()).unwrap_or("unknown");
                            let code_str = d.get("code").and_then(|c| c.as_str()).unwrap_or("");
                            let severity_val = d.get("severity").and_then(|s| s.as_u64()).unwrap_or(3);
                            let severity = match severity_val {
                                1 => DiagnosticSeverity::ERROR,
                                2 => DiagnosticSeverity::WARNING,
                                3 => DiagnosticSeverity::INFORMATION,
                                _ => DiagnosticSeverity::HINT,
                            };
                            let source = if !code_str.is_empty() {
                                format!("SonarComplexity[{}]", code_str)
                            } else {
                                "SonarComplexity".to_string()
                            };
                            diags.push(bridge_diagnostic(line, col, message, severity, &source));
                        }
                    }
                }
                break;
            }
        }

        Self::send_message(&mut child, "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"shutdown\",\"params\":null}");
        let _ = Self::read_message(&mut child);

        Self::send_message(&mut child, "{\"jsonrpc\":\"2.0\",\"method\":\"exit\",\"params\":null}");

        std::thread::sleep(Duration::from_millis(500));
        let _ = child.kill();
        let _ = child.wait();

        diags
    }
}