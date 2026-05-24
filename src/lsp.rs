use std::collections::HashMap;
use std::sync::Arc;

use tower_lsp::lsp_types::*;
use tower_lsp::{jsonrpc::Result, Client, LanguageServer};

use crate::ast::{AstEngine, find_child_by_path, max_loop_depth, node_text};
use crate::ast::has_recursion;
use crate::checks::CheckPipeline;
use crate::config::PraetorConfig;
use crate::facts::SymbolTable;

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

    /// Return the .praetor/ directory path if config has a path we can derive from.
    fn praetor_dir(&self) -> Option<std::path::PathBuf> {
        self.config.as_ref().and_then(|cfg| {
            cfg.path.as_ref().and_then(|p| {
                p.parent().map(|dir| dir.join(".praetor"))
            })
        })
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

        let results = CheckPipeline::run(&parsed, &self.engine, &cfg, self.praetor_dir().as_deref());
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

    fn compute_code_lenses(&self, uri: &str, text: &str) -> Vec<CodeLens> {
        let ext = extension_from_uri(uri);
        if ext.is_empty() || !self.engine.supports_extension(ext) {
            return vec![];
        }

        let cfg = self.config.as_ref().cloned().unwrap_or_default();
        let parsed = match self.engine.parse(ext, text.as_bytes()) {
            Some(p) => p,
            None => return vec![],
        };

        let results = CheckPipeline::run(&parsed, &self.engine, &cfg, self.praetor_dir().as_deref());

        // Collect all function nodes with their ranges
        let mut funcs: Vec<(Range, String)> = Vec::new();
        let mut cursor = parsed.tree.root_node().walk();
        collect_functions(
            parsed.tree.root_node(),
            parsed.config,
            &mut funcs,
            &mut cursor,
        );

        let mut lenses = Vec::new();

        for (fn_range, _fn_name) in &funcs {
            let fn_diags: Vec<_> = results
                .iter()
                .filter(|d| ranges_overlap(&d.range, fn_range))
                .collect();

            let (symbol, status) = if fn_diags.is_empty() {
                ("✅", "verified".to_string())
            } else {
                let mut parts = Vec::new();
                let errors = fn_diags.iter().filter(|d| {
                    d.severity == tower_lsp::lsp_types::DiagnosticSeverity::ERROR
                }).count();
                let warnings = fn_diags.iter().filter(|d| {
                    d.severity == tower_lsp::lsp_types::DiagnosticSeverity::WARNING
                }).count();
                let hints = fn_diags.iter().filter(|d| {
                    d.severity == tower_lsp::lsp_types::DiagnosticSeverity::HINT
                }).count();

                if errors > 0 {
                    parts.push(format!("{} error(s)", errors));
                }
                if warnings > 0 {
                    parts.push(format!("{} warning(s)", warnings));
                }
                if hints > 0 {
                    parts.push(format!("{} hint(s)", hints));
                }

                let icon = if errors > 0 { "⛔" } else if warnings > 0 { "⚠️" } else { "💡" };
                (icon, parts.join(", "))
            };

            lenses.push(CodeLens {
                range: *fn_range,
                command: Some(Command {
                    title: format!("{} {}", symbol, status),
                    command: String::new(),
                    arguments: None,
                }),
                data: None,
            });
        }

        lenses
    }

    fn compute_hover(
        &self,
        uri: &str,
        text: &str,
        position: Position,
    ) -> Option<String> {
        let ext = extension_from_uri(uri);
        if ext.is_empty() || !self.engine.supports_extension(ext) {
            return None;
        }
        let parsed = self.engine.parse(ext, text.as_bytes())?;
        let lang = parsed.config;
        let source = parsed.text;
        let root = parsed.tree.root_node();

        // Find the function node containing the hover position
        let mut target_fn: Option<(String, u32, u32, i32)> = None;
        let mut fn_cursor = root.walk();
        for child in root.children(&mut fn_cursor) {
            if !lang.function_types.contains(&child.kind()) {
                continue;
            }
            let start_row = child.start_position().row as i32;
            let end_row = child.end_position().row as i32;
            let pos_row = position.line as i32;
            if pos_row >= start_row && pos_row <= end_row {
                let name_node = find_child_by_path(child, lang.function_name_path)?;
                let fn_name = node_text(name_node, source);
                let loop_depth = max_loop_depth(child, lang.loop_types, 0);
                let recursive = has_recursion(
                    child, &fn_name, lang.call_type, lang.call_target_path, source,
                );
                target_fn = Some((fn_name.to_string(), loop_depth, if recursive { 1 } else { 0 }, 0));
                break;
            }
        }
        let (fn_name, loop_depth, recursive, _) = target_fn?;

        // Complexity classification
        let label = if recursive > 0 {
            "O(2ⁿ)".to_string()
        } else {
            match loop_depth {
                0 => "O(1)".to_string(),
                1 => "O(n)".to_string(),
                2 => "O(n²)".to_string(),
                _ => "O(n^k)".to_string(),
            }
        };

        // Intent — find preceding comment
        let mut intent_text = String::new();
        let mut fn_cursor2 = root.walk();
        for child in root.children(&mut fn_cursor2) {
            if !lang.function_types.contains(&child.kind()) {
                continue;
            }
            if let Some(name_node) = find_child_by_path(child, lang.function_name_path) {
                let name = node_text(name_node, source);
                if name == fn_name {
                    let mut prev: Option<tree_sitter::Node> = None;
                    let mut c = child.walk();
                    if c.goto_parent() {
                        let parent = c.node();
                        let mut pc = parent.walk();
                        for sib in parent.children(&mut pc) {
                            if sib == child { break; }
                            prev = Some(sib);
                        }
                    }
                    if let Some(prev_node) = prev {
                        if lang.comment_types.contains(&prev_node.kind()) {
                            intent_text = prev_node.utf8_text(source).unwrap_or("").to_string();
                        }
                    }
                    break;
                }
            }
        }

        // Datalog facts for this function
        let mut fact_lines: Vec<String> = Vec::new();
        {
            let mut sym = SymbolTable::new();
            let mut calls = Vec::new();
            let mut accesses = Vec::new();
            let mut declares = Vec::new();
            let mut annotated = Vec::new();
            let mut param_counts = Vec::new();
            let mut positions: HashMap<u32, (u32, u32)> = HashMap::new();
            let mut fn_cursor3 = root.walk();
            for child in root.children(&mut fn_cursor3) {
                if !lang.function_types.contains(&child.kind()) {
                    continue;
                }
                if find_child_by_path(child, lang.function_name_path)
                    .is_some_and(|n| node_text(n, source) == fn_name)
                {
                    crate::facts::collect_facts_inner(
                        child, lang, source, &mut sym,
                        &mut calls, &mut accesses, &mut declares,
                        &mut annotated, &mut param_counts, &mut positions,
                    );
                    break;
                }
            }

            let ds = crate::facts::evaluate_facts(
                &mut sym, &calls, &accesses, &declares,
                &annotated, &param_counts, &positions,
            );
            for d in &ds {
                if d.function == fn_name {
                    fact_lines.push(format!("- {} (line {})", d.message, d.line + 1));
                }
            }
        }

        // Build hover markdown
        let mut md = format!("## `{}`\n\n", fn_name);

        if !intent_text.is_empty() {
            let trimmed = intent_text.trim_start_matches("//").trim_start_matches("#").trim_start_matches("/*").trim_start_matches("*/").trim();
            md.push_str(&format!("**Intent:** {}\n\n", trimmed));
        } else {
            md.push_str("⚠️ **Missing intent comment**\n\n");
        }

        md.push_str(&format!("**Complexity:** {} (loop depth {})\n\n", label, loop_depth));

        if !fact_lines.is_empty() {
            md.push_str("**Datalog Facts:**\n\n");
            for line in &fact_lines {
                md.push_str(line);
                md.push('\n');
            }
        } else {
            md.push_str("✅ **No Datalog violations**\n\n");
        }

        // Run check pipeline to show other diagnostics for this function
        {
            let cfg = self.config.as_ref().cloned().unwrap_or_default();
            let results = CheckPipeline::run(&parsed, &self.engine, &cfg, self.praetor_dir().as_deref());
            let mut fn_diags: Vec<String> = Vec::new();
            let mut fn_cursor4 = root.walk();
            for child in root.children(&mut fn_cursor4) {
                if !lang.function_types.contains(&child.kind()) {
                    continue;
                }
                if find_child_by_path(child, lang.function_name_path)
                    .is_some_and(|n| node_text(n, source) == fn_name)
                {
                    let rng = Range {
                        start: Position {
                            line: child.start_position().row as u32,
                            character: child.start_position().column as u32,
                        },
                        end: Position {
                            line: child.end_position().row as u32,
                            character: child.end_position().column as u32,
                        },
                    };
                    for d in &results {
                        if ranges_overlap(&d.range, &rng) {
                            fn_diags.push(format!(
                                "- [{}] {}",
                                d.source,
                                d.message
                            ));
                        }
                    }
                    break;
                }
            }
            if !fn_diags.is_empty() {
                md.push_str("**Check Results:**\n\n");
                for line in &fn_diags {
                    md.push_str(line);
                    md.push('\n');
                }
            }
        }

        let state_path = std::path::Path::new(".praetor").join("state-graph.json");
        if state_path.exists() {
            md.push_str("\n---\n⚠️ State graph not yet validated (Phase 7B)\n");
        }
        md.push_str(&format!("\n---\n🤖 5 Datalog rules active\n"));

        Some(md)
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

        let results = CheckPipeline::run(&parsed, &self.engine, &cfg, self.praetor_dir().as_deref());
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
                name: "praetor".into(),
                version: Some("0.1.0".into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                inlay_hint_provider: Some(OneOf::Left(true)),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("praetor initialized");

        let manifesto = format!(
            r#"╔══════════════════════════════════════════════════════════════╗
║  PRAETOR VERIFICATION ACTIVE — quadruple bookkeeping      ║
║                                                              ║
║  Code   → {} languages, {} extensions watched              ║
║  Docs   → Intent comments required (severity: {})        ║
║  Graph  → State transitions verified (in progress)        ║
║  Facts  → Datalog invariants enforced (5 rules active)    ║
║                                                              ║
║  AI: All generated code must satisfy all four pillars.     ║
║  Violations appear as editor diagnostics immediately.      ║
╚══════════════════════════════════════════════════════════════╝"#,
            self.engine.loaded_count(),
            crate::ast::languages::all_extensions().len(),
            self.config.as_ref().map(|c| c.intent.severity.as_str()).unwrap_or("error"),
        );

        self.client
            .show_message(MessageType::INFO, &manifesto)
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

    async fn code_lens(
        &self,
        params: CodeLensParams,
    ) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri.to_string();
        let text = {
            let docs = self.documents.lock().unwrap();
            docs.get(&uri).map(|s| s.text.clone())
        };
        match text {
            Some(text) => {
                let lenses = self.compute_code_lenses(&uri, &text);
                Ok(Some(lenses))
            }
            None => Ok(None),
        }
    }

    async fn hover(
        &self,
        params: HoverParams,
    ) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let pos = params.text_document_position_params.position;
        let text = {
            let docs = self.documents.lock().unwrap();
            docs.get(&uri).map(|s| s.text.clone())
        };
        match text {
            Some(text) => {
                let content = self.compute_hover(&uri, &text, pos);
                Ok(content.map(|c| Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: c,
                    }),
                    range: None,
                }))
            }
            None => Ok(None),
        }
    }
}

fn collect_functions<'a>(
    node: tree_sitter::Node<'a>,
    lang: &crate::ast::LanguageConfig,
    funcs: &mut Vec<(Range, String)>,
    cursor: &mut tree_sitter::TreeCursor<'a>,
) {
    if lang.function_types.contains(&node.kind()) {
        if let Some(name_node) = crate::ast::find_child_by_path(node, lang.function_name_path) {
            let name_bytes = name_node.utf8_text(&[]).unwrap_or("");
            let start = Position {
                line: node.start_position().row as u32,
                character: node.start_position().column as u32,
            };
            let end = Position {
                line: node.end_position().row as u32,
                character: node.end_position().column as u32,
            };
            funcs.push((Range { start, end }, name_bytes.to_string()));
        }
    }
    if node.child_count() > 0 {
        cursor.reset(node);
        while cursor.goto_first_child() {
            collect_functions(cursor.node(), lang, funcs, cursor);
        }
        cursor.goto_parent();
    }
}

fn ranges_overlap(a: &Range, b: &Range) -> bool {
    !(a.end.line < b.start.line || (a.end.line == b.start.line && a.end.character <= b.start.character))
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
