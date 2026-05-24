use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};
use regex::Regex;
use tree_sitter::Node;

use crate::ast::{find_child_by_path, node_text, previous_sibling};
use crate::ast::ParsedFile;
use crate::checks::CheckDiagnostic;
use crate::config::IntentConfig;

struct IntentContext<'a> {
    lang: &'a crate::ast::LanguageConfig,
    source: &'a [u8],
    severity: DiagnosticSeverity,
    config: &'a IntentConfig,
    diags: &'a mut Vec<CheckDiagnostic>,
}

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
    let mut ctx = IntentContext {
        lang: parsed.config,
        source: parsed.text,
        severity: sev,
        config,
        diags: &mut diags,
    };

    walk_intent_check(parsed.tree.root_node(), &mut ctx);
    diags
}

fn walk_intent_check<'a>(node: Node<'a>, ctx: &mut IntentContext<'a>) {
    if ctx.lang.function_types.contains(&node.kind()) {
        check_function_intent(node, ctx);
    }
    if node.child_count() > 0 {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_intent_check(child, ctx);
        }
    }
}

fn check_function_intent(fn_node: Node, ctx: &mut IntentContext) {
    let fn_name = find_child_by_path(fn_node, ctx.lang.function_name_path)
        .map(|n| node_text(n, ctx.source)).unwrap_or("");

    if is_exempt(fn_name, ctx.config) {
        return;
    }
    if !previous_sibling(fn_node)
        .is_some_and(|prev| ctx.lang.comment_types.contains(&prev.kind()))
    {
        push_intent_diag(fn_node, ctx.source, fn_name, ctx.severity, ctx.diags);
    }
}

fn is_exempt(fn_name: &str, config: &IntentConfig) -> bool {
    config.exempt_patterns
        .iter()
        .any(|pat| Regex::new(pat).is_ok_and(|re| re.is_match(fn_name)))
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
