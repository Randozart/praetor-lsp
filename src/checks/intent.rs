use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};
use regex::Regex;
use tree_sitter::Node;

use crate::ast::{find_child_by_path, node_text};
use crate::ast::ParsedFile;
use crate::checks::CheckDiagnostic;
use crate::config::IntentConfig;

pub fn check_intent(
    parsed: &ParsedFile,
    config: &IntentConfig,
) -> Vec<CheckDiagnostic> {
    let mut diags = Vec::new();
    let sev = match config.severity.as_str() {
        "warning" => DiagnosticSeverity::WARNING,
        "hint" => DiagnosticSeverity::HINT,
        _ => DiagnosticSeverity::ERROR,
    };

    let mut cursor = parsed.tree.root_node().walk();
    walk_intent_check(
        parsed.tree.root_node(),
        parsed.config,
        parsed.text,
        sev,
        config,
        &mut diags,
        &mut cursor,
    );
    diags
}

fn walk_intent_check<'a>(
    node: Node<'a>,
    lang: &crate::ast::LanguageConfig,
    source: &'a [u8],
    severity: DiagnosticSeverity,
    config: &IntentConfig,
    diags: &mut Vec<CheckDiagnostic>,
    cursor: &mut tree_sitter::TreeCursor<'a>,
) {
    if lang.function_types.contains(&node.kind()) {
        let name_node = find_child_by_path(node, lang.function_name_path);
        let fn_name = name_node.map(|n| node_text(n, source)).unwrap_or("");

        if config
            .exempt_patterns
            .iter()
            .any(|pat| Regex::new(pat).is_ok_and(|re| re.is_match(fn_name)))
        {
            // Skip — exempt
        } else if let Some(prev) = previous_sibling(node) {
            let has_comment = lang
                .comment_types
                .iter()
                .any(|ct| prev.kind() == *ct);
            if !has_comment {
                push_intent_diag(node, source, fn_name, severity, diags);
            }
        } else {
            push_intent_diag(node, source, fn_name, severity, diags);
        }
    }

    if node.child_count() > 0 {
        cursor.reset(node);
        while cursor.goto_first_child() {
            walk_intent_check(
                cursor.node(),
                lang,
                source,
                severity,
                config,
                diags,
                cursor,
            );
        }
        cursor.goto_parent();
    }
}

fn previous_sibling(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    if !cursor.goto_parent() {
        return None;
    }
    let parent = cursor.node();
    let mut prev: Option<Node> = None;
    let mut c = parent.walk();
    for child in parent.children(&mut c) {
        if child == node {
            return prev;
        }
        prev = Some(child);
    }
    None
}

fn push_intent_diag(
    fn_node: Node,
    _source: &[u8],
    fn_name: &str,
    severity: DiagnosticSeverity,
    diags: &mut Vec<CheckDiagnostic>,
) {
    let name = if fn_name.is_empty() { "(anonymous)" } else { fn_name };
    let start = fn_node.start_position();
    let end = fn_node.end_position();
    let msg = if severity == DiagnosticSeverity::ERROR {
        format!(
            "[Intent Required] `{}` — you MUST declare in a comment how this \
             function is expected to behave",
            name
        )
    } else {
        format!(
            "[Intent Suggested] `{}` — consider adding a doc comment \
             describing expected behaviour",
            name
        )
    };

    diags.push(CheckDiagnostic {
        range: Range {
            start: Position {
                line: start.row as u32,
                character: start.column as u32,
            },
            end: Position {
                line: end.row as u32,
                character: end.column as u32,
            },
        },
        message: msg,
        severity,
        source: "praetor/intent".into(),
    });
}
