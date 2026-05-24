use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};
use tree_sitter::Node;

use crate::ast::{find_child_by_path, max_loop_depth, node_text};
use crate::ast::ParsedFile;
use crate::checks::CheckDiagnostic;
use crate::config::ComplexityConfig;

const LINEAR_OPS: &[&str] = &[
    "indexOf", "find", "contains", "includes", "search", "index", "count",
];

pub fn check_complexity(
    parsed: &ParsedFile,
    config: &ComplexityConfig,
) -> Vec<CheckDiagnostic> {
    let mut diags = Vec::new();
    walk_functions(
        parsed.tree.root_node(),
        parsed.config,
        parsed.text,
        config,
        &mut diags,
    );
    diags
}

fn walk_functions<'a>(
    node: Node<'a>,
    lang: &crate::ast::LanguageConfig,
    source: &'a [u8],
    config: &ComplexityConfig,
    diags: &mut Vec<CheckDiagnostic>,
) {
    if lang.function_types.contains(&node.kind()) {
        if let Some(result) = analyze_function(node, lang, source, config) {
            diags.push(result);
        }
    }
    if node.child_count() > 0 {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_functions(child, lang, source, config, diags);
        }
    }
}

fn analyze_function(
    fn_node: Node,
    lang: &crate::ast::LanguageConfig,
    source: &[u8],
    config: &ComplexityConfig,
) -> Option<CheckDiagnostic> {
    let name_node = find_child_by_path(fn_node, lang.function_name_path)?;
    let fn_name = node_text(name_node, source);

    let loop_depth = max_loop_depth(fn_node, lang.loop_types, 0);
    let rec = crate::ast::has_recursion(
        fn_node,
        fn_name,
        lang.call_type,
        lang.call_target_path,
        source,
    );
    let linear_ops = count_linear_ops_in_loops(fn_node, lang, source);

    let (label, detail) = classify_complexity(loop_depth, rec, linear_ops, config);

    let pos = Position {
        line: name_node.start_position().row as u32,
        character: name_node.end_position().column as u32,
    };

    Some(CheckDiagnostic {
        range: Range {
            start: pos,
            end: Position {
                line: pos.line,
                character: pos.character + 1,
            },
        },
        message: format!("[Complexity] {} — {}", label, detail),
        severity: if loop_depth >= 2 || rec {
            DiagnosticSeverity::WARNING
        } else {
            DiagnosticSeverity::HINT
        },
        source: "praetor/complexity".into(),
    })
}

fn count_linear_ops_in_loops(
    node: Node,
    lang: &crate::ast::LanguageConfig,
    source: &[u8],
) -> u32 {
    let mut count = 0;
    if lang.loop_types.contains(&node.kind()) {
        count += count_linear_ops_in_subtree(node, lang, source);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_linear_ops_in_loops(child, lang, source);
    }
    count
}

fn count_linear_ops_in_subtree(
    node: Node,
    lang: &crate::ast::LanguageConfig,
    source: &[u8],
) -> u32 {
    let mut count = 0;
    if node.kind() == lang.call_type {
        if let Some(target) = find_child_by_path(node, lang.call_target_path) {
            let name = node_text(target, source);
            if LINEAR_OPS.contains(&name) {
                count += 1;
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_linear_ops_in_subtree(child, lang, source);
    }
    count
}

fn classify_complexity(
    depth: u32,
    recursive: bool,
    linear_ops: u32,
    _config: &ComplexityConfig,
) -> (String, String) {
    let mut detail_parts = Vec::new();

    if recursive {
        return ("O(2ⁿ)".into(), "recursive call, no memoization".into());
    }

    let label = match depth {
        0 => {
            if linear_ops > 0 {
                detail_parts.push(format!("{} linear op(s)", linear_ops));
                "O(n)"
            } else {
                "O(1)"
            }
        }
        1 => {
            if linear_ops > 0 {
                detail_parts.push(format!("{} linear op(s) in loop", linear_ops));
                "O(n·m)"
            } else {
                detail_parts.push("single loop".into());
                "O(n)"
            }
        }
        2 => {
            if linear_ops > 0 {
                detail_parts.push(format!("{} linear op(s) in nested loops", linear_ops));
                "O(n²·m)"
            } else {
                detail_parts.push("loop depth 2".into());
                "O(n²)"
            }
        }
        d => {
            detail_parts.push(format!("loop depth {}", d));
            "O(n^k)"
        }
    }
    .to_string();

    if detail_parts.is_empty() {
        detail_parts.push("constant time".into());
    }

    (label, detail_parts.join("; "))
}
