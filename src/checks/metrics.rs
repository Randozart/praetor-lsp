use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};
use tree_sitter::Node;

use crate::ast::{find_child_by_path, node_text, ParsedFile};
use crate::config::ComplexityConfig;
use super::CheckDiagnostic;

const DECISION_KINDS: &[&str] = &[
    "if_statement",
    "if_expression",
    "else_clause",
    "elif_clause",
    "for_statement",
    "for_expression",
    "for_in_statement",
    "while_statement",
    "while_expression",
    "do_statement",
    "switch_statement",
    "switch_case",
    "case_statement",
    "catch_clause",
    "conditional_expression",
    "ternary_expression",
    "throw_statement",
    "raise_statement",
];

const LOGICAL_OPS: &[&str] = &[
    "&&",
    "||",
    "and",
    "or",
];

pub fn check_metrics(
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
        if let Some((fn_name, fn_range)) = compute_fn_range(node, lang, source) {
            check_single_function(node, config, &fn_name, fn_range, diags);
        }
    }
    if node.child_count() > 0 {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_functions(child, lang, source, config, diags);
        }
    }
}

fn compute_fn_range<'a>(
    fn_node: Node<'a>,
    lang: &crate::ast::LanguageConfig,
    source: &'a [u8],
) -> Option<(&'a str, Range)> {
    let name_node = find_child_by_path(fn_node, lang.function_name_path)?;
    let fn_name = node_text(name_node, source);
    if fn_name.is_empty() {
        return None;
    }
    let range = Range {
        start: Position {
            line: name_node.start_position().row as u32,
            character: name_node.start_position().column as u32,
        },
        end: Position {
            line: fn_node.end_position().row as u32,
            character: fn_node.end_position().column as u32,
        },
    };
    Some((fn_name, range))
}

fn check_single_function(
    fn_node: Node,
    config: &ComplexityConfig,
    fn_name: &str,
    fn_range: Range,
    diags: &mut Vec<CheckDiagnostic>,
) {
    let decision_count = count_decisions(fn_node);
    let cyclomatic = 1 + decision_count;
    let cognitive = compute_cognitive(fn_node, 0);
    let start_row = fn_node.start_position().row as u32;
    let end_row = fn_node.end_position().row as u32;
    let fn_lines = end_row.saturating_sub(start_row) + 1;
    let max_nesting = max_nesting_depth(fn_node, 0);
    let param_count = count_params(fn_node);

    let mut violations: Vec<String> = Vec::new();

    if cyclomatic > config.cyclomatic_max as u32 {
        diags.push(CheckDiagnostic {
            range: fn_range,
            message: format!("[Metrics] `{}` has cyclomatic complexity {} (max {}) — consider splitting into sub-functions",
                fn_name, cyclomatic, config.cyclomatic_max),
            severity: DiagnosticSeverity::WARNING,
            source: "praetor/metrics".into(),
            code: Some("S3776".into()),
        });
        violations.push(format!("Cyclomatic complexity {} (max {})", cyclomatic, config.cyclomatic_max));
    }

    if cognitive > config.cognitive_max as u32 {
        diags.push(CheckDiagnostic {
            range: fn_range,
            message: format!("[Metrics] `{}` has cognitive complexity {} (max {}) — consider early returns instead of else-if chains",
                fn_name, cognitive, config.cognitive_max),
            severity: DiagnosticSeverity::WARNING,
            source: "praetor/metrics".into(),
            code: Some("S3776".into()),
        });
        violations.push(format!("Cognitive complexity {} (max {})", cognitive, config.cognitive_max));
    }

    if fn_lines > config.max_function_lines {
        diags.push(CheckDiagnostic {
            range: fn_range,
            message: format!("[Metrics] `{}` is {} lines long (max {})", fn_name, fn_lines, config.max_function_lines),
            severity: DiagnosticSeverity::HINT,
            source: "praetor/metrics".into(),
            code: None,
        });
        violations.push(format!("Function exceeds {} lines ({} > {})", config.max_function_lines, fn_lines, config.max_function_lines));
    }

    if max_nesting > config.max_nesting_depth {
        diags.push(CheckDiagnostic {
            range: fn_range,
            message: format!("[Metrics] `{}` has nesting depth {} (max {}) — consider flattening with early returns",
                fn_name, max_nesting, config.max_nesting_depth),
            severity: DiagnosticSeverity::HINT,
            source: "praetor/metrics".into(),
            code: Some("S134".into()),
        });
        violations.push(format!("Nesting depth {} exceeds maximum of {}", max_nesting, config.max_nesting_depth));
    }

    if param_count > config.max_params {
        diags.push(CheckDiagnostic {
            range: fn_range,
            message: format!("[Metrics] `{}` has {} parameters (max {})", fn_name, param_count, config.max_params),
            severity: DiagnosticSeverity::HINT,
            source: "praetor/metrics".into(),
            code: None,
        });
        violations.push(format!("Parameter count {} (max {})", param_count, config.max_params));
    }

    if !violations.is_empty() {
        let summary = violations.join("\n");
        let msg = format!("[Function Complexity] `{}` has {} issue(s):\n{}", fn_name, violations.len(), summary);
        diags.push(CheckDiagnostic {
            range: fn_range, message: msg,
            severity: DiagnosticSeverity::WARNING,
            source: "function-complexity".into(),
            code: None,
        });
    }
}

fn count_decisions(node: Node) -> u32 {
    let mut count = 0;
    let kind = node.kind();
    if DECISION_KINDS.contains(&kind) || LOGICAL_OPS.contains(&kind) {
        count += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_decisions(child);
    }
    count
}

fn compute_cognitive(node: Node, depth: u32) -> u32 {
    let mut complexity = 0;
    let kind = node.kind();
    let is_decision = DECISION_KINDS.contains(&kind) || LOGICAL_OPS.contains(&kind);
    if is_decision {
        complexity += 1 + depth;
    }
    let new_depth = if is_decision { depth + 1 } else { depth };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        complexity += compute_cognitive(child, new_depth);
    }
    complexity
}

fn max_nesting_depth(node: Node, depth: u32) -> u32 {
    let kind = node.kind();
    let is_nesting = DECISION_KINDS.contains(&kind);
    let current_depth = if is_nesting { depth + 1 } else { depth };
    let mut max_depth = current_depth;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_depth = max_nesting_depth(child, current_depth);
        if child_depth > max_depth {
            max_depth = child_depth;
        }
    }
    max_depth
}

fn count_params(fn_node: Node) -> u32 {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if is_params_node(&child) {
            return count_param_children(&child);
        }
    }
    0
}

fn is_params_node(child: &Node) -> bool {
    let k = child.kind();
    k == "parameters" || k == "formal_parameters" || k.ends_with("parameters")
}

fn count_param_children(params_node: &Node) -> u32 {
    let mut count = 0;
    let mut pc = params_node.walk();
    for param in params_node.children(&mut pc) {
        if is_actual_param(&param) {
            count += 1;
        }
    }
    count
}

fn is_actual_param(param: &Node) -> bool {
    let pk = param.kind();
    !matches!(pk, "," | ":" | "(" | ")" | "->" | "=>")
        && !pk.ends_with("type") && !pk.ends_with("annotation")
        && !pk.ends_with("pattern")
}
