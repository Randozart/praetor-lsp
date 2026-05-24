use std::sync::Arc;

use tower_lsp::lsp_types::*;
use tower_lsp::{jsonrpc::Result, Client, LanguageServer};

use crate::ast::AstEngine;
use crate::checks::CheckPipeline;
use crate::config::PraetorConfig;

#[derive(Clone)]
struct DocumentState {
    #[allow(dead_code)]
    uri: String,
    text: String,
    version: i32,
}

fn extension_from_uri(uri: &str) -> &str {
    let path = uri.trim_start_matches("file://");
    let dot = path.rfind('.');
    match dot {
        Some(pos) => &path[pos..],
        None => "",
    }
}

pub struct Backend {
    client: Client,
    engine: Arc<AstEngine>,
    config: Option<PraetorConfig>,
    documents: std::sync::Mutex<std::collections::HashMap<String, DocumentState>>,
}

impl Backend {
    pub fn new(client: Client, engine: Arc<AstEngine>, config: Option<PraetorConfig>) -> Self {
        Self {
            client,
            engine,
            config,
            documents: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn run_checks(&self, uri: &str, text: &str) -> Vec<Diagnostic> {
        let ext = extension_from_uri(uri);
        if ext.is_empty() || !self.engine.supports_extension(ext) {
            return vec![];
        }

        let cfg = self.config.as_ref().cloned().unwrap_or_default();

        let parsed = match self.engine.parse(ext, text.as_bytes()) {
            Some(p) => p,
            None => return vec![],
        };

        let results = CheckPipeline::run(&parsed, &self.engine, &cfg);
        results.into_iter().map(|cd| {
            let severity = match cd.severity {
                DiagnosticSeverity::ERROR => Some(DiagnosticSeverity::ERROR),
                DiagnosticSeverity::WARNING => Some(DiagnosticSeverity::WARNING),
                DiagnosticSeverity::INFORMATION => Some(DiagnosticSeverity::INFORMATION),
                DiagnosticSeverity::HINT => Some(DiagnosticSeverity::HINT),
                _ => Some(DiagnosticSeverity::HINT),
            };
            Diagnostic {
                range: cd.range,
                severity,
                source: Some(cd.source),
                message: cd.message,
                ..Default::default()
            }
        }).collect()
    }

    fn compute_inlay_hints(&self, uri: &str, text: &str) -> Vec<InlayHint> {
        let ext = extension_from_uri(uri);
        if ext.is_empty() || !self.engine.supports_extension(ext) {
            return vec![];
        }

        let cfg = self.config.as_ref().cloned().unwrap_or_default();

        let parsed = match self.engine.parse(ext, text.as_bytes()) {
            Some(p) => p,
            None => return vec![],
        };

        let results = CheckPipeline::run(&parsed, &self.engine, &cfg);
        results
            .into_iter()
            .filter_map(|d| {
                let is_hint_or_warning = matches!(
                    d.severity,
                    DiagnosticSeverity::HINT | DiagnosticSeverity::WARNING
                );
                if !is_hint_or_warning {
                    return None;
                }
                let msg = d.message.clone();
                let label = d.message.split(" — ").next().unwrap_or(&msg).to_string();
                Some(InlayHint {
                    position: Position {
                        line: d.range.start.line,
                        character: d.range.start.character,
                    },
                    label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                        value: format!(" ⚡ {}", label),
                        tooltip: Some(InlayHintLabelPartTooltip::String(msg)),
                        ..Default::default()
                    }]),
                    kind: Some(InlayHintKind::TYPE),
                    padding_right: Some(true),
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    data: None,
                })
            })
            .collect()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "praetor-lsp".into(),
                version: Some("0.1.0".into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                inlay_hint_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("praetor-lsp initialized");
        self.client
            .show_message(MessageType::INFO, "praetor-lsp ready — verification checks active")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("praetor-lsp shutting down");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        let uri = doc.uri.to_string();
        let text = doc.text;
        let version = doc.version;

        {
            let mut docs = self.documents.lock().unwrap();
            docs.insert(
                uri.clone(),
                DocumentState {
                    uri: uri.clone(),
                    text: text.clone(),
                    version,
                },
            );
        }

        let diags = self.run_checks(&uri, &text);
        self.client.publish_diagnostics(doc.uri, diags, Some(version)).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let version = params.text_document.version;

        let new_text = {
            let mut docs = self.documents.lock().unwrap();
            if let Some(state) = docs.get_mut(&uri) {
                for change in &params.content_changes {
                    if change.range.is_some() {
                        state.text = apply_incremental_change(&state.text, change);
                    } else {
                        state.text = change.text.clone();
                    }
                }
                state.version = version;
                state.text.clone()
            } else {
                return;
            }
        };

        let diags = self.run_checks(&uri, &new_text);
        self.client
            .publish_diagnostics(params.text_document.uri, diags, Some(version))
            .await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let text = {
            let docs = self.documents.lock().unwrap();
            docs.get(&uri).map(|s| s.text.clone())
        };
        if let Some(text) = text {
            let diags = self.run_checks(&uri, &text);
            self.client
                .publish_diagnostics(params.text_document.uri, diags, None)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.documents.lock().unwrap().remove(&uri);
    }

    async fn inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.to_string();
        let text = {
            let docs = self.documents.lock().unwrap();
            docs.get(&uri).map(|s| s.text.clone())
        };
        match text {
            Some(text) => {
                let hints = self.compute_inlay_hints(&uri, &text);
                Ok(Some(hints))
            }
            None => Ok(None),
        }
    }
}

fn apply_incremental_change(text: &str, change: &TextDocumentContentChangeEvent) -> String {
    if let Some(range) = change.range {
        let start = range.start;
        let end = range.end;
        let lines: Vec<&str> = text.split('\n').collect();
        let mut result = String::new();

        for (i, line) in lines.iter().enumerate() {
            let line_num = i as u32;
            if line_num < start.line {
                result.push_str(line);
                result.push('\n');
            } else if line_num == start.line {
                let prefix = &line[..start.character as usize];
                result.push_str(prefix);
                result.push_str(&change.text);
                if end.line == start.line {
                    let suffix = &line[end.character as usize..];
                    result.push_str(suffix);
                }
                result.push('\n');
            } else if line_num > end.line {
                result.push_str(line);
                if line_num < lines.len() as u32 - 1 {
                    result.push('\n');
                }
            }
        }
        result
    } else {
        change.text.clone()
    }
}
