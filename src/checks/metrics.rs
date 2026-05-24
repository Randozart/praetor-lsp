use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};
use tree_sitter::Node;

use crate::ast::{find_child_by_path, node_text, ParsedFile};
use crate::config::ComplexityConfig;
use super::CheckDiagnostic;

/// Node kinds that represent decision points for cyclomatic complexity.
const DECISION_KINDS: &[&str] = &[
    "if_statement",
    "if_expression",
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

/// Binary operator kinds that add to complexity.
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
    let lang = parsed.config;
    let source = parsed.text;
    let root = parsed.tree.root_node();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !lang.function_types.contains(&child.kind()) {
            continue;
        }
        let name_node = match find_child_by_path(child, lang.function_name_path) {
            Some(n) => n,
            None => continue,
        };
        let fn_name = node_text(name_node, source);
        if fn_name.is_empty() {
            continue;
        }

        let start = name_node.start_position();
        let fn_range = Range {
            start: Position {
                line: start.row as u32,
                character: start.column as u32,
            },
            end: Position {
                line: child.end_position().row as u32,
                character: child.end_position().column as u32,
            },
        };

        // Cyclomatic complexity
        let decision_count = count_decisions(child);
        let cyclomatic = 1 + decision_count;

        // Cognitive complexity
        let cognitive = compute_cognitive(child, 0);

        // Function line count
        let start_row = child.start_position().row as u32;
        let end_row = child.end_position().row as u32;
        let fn_lines = end_row.saturating_sub(start_row) + 1;

        // Nesting depth
        let max_nesting = max_nesting_depth(child, 0);

        // Parameter count
        let param_count = count_params(child);

        // Cyclomatic complexity check
        if cyclomatic > config.cyclomatic_max as u32 {
            diags.push(CheckDiagnostic {
                range: fn_range,
                message: format!(
                    "[Metrics] `{}` has cyclomatic complexity {} (max {})",
                    fn_name, cyclomatic, config.cyclomatic_max,
                ),
                severity: DiagnosticSeverity::WARNING,
                source: "praetor/metrics".into(),
            });
        }

        // Cognitive complexity check
        if cognitive > config.cognitive_max as u32 {
            diags.push(CheckDiagnostic {
                range: fn_range,
                message: format!(
                    "[Metrics] `{}` has cognitive complexity {} (max {})",
                    fn_name, cognitive, config.cognitive_max,
                ),
                severity: DiagnosticSeverity::WARNING,
                source: "praetor/metrics".into(),
            });
        }

        // Max function lines check
        if fn_lines > config.max_function_lines {
            diags.push(CheckDiagnostic {
                range: fn_range,
                message: format!(
                    "[Metrics] `{}` is {} lines long (max {})",
                    fn_name, fn_lines, config.max_function_lines,
                ),
                severity: DiagnosticSeverity::HINT,
                source: "praetor/metrics".into(),
            });
        }

        // Nesting depth check
        if max_nesting > config.max_nesting_depth {
            diags.push(CheckDiagnostic {
                range: fn_range,
                message: format!(
                    "[Metrics] `{}` has nesting depth {} (max {})",
                    fn_name, max_nesting, config.max_nesting_depth,
                ),
                severity: DiagnosticSeverity::HINT,
                source: "praetor/metrics".into(),
            });
        }

        // Parameter count check (complementary to Datalog rule 4)
        if param_count > config.max_params {
            diags.push(CheckDiagnostic {
                range: fn_range,
                message: format!(
                    "[Metrics] `{}` has {} parameters (max {})",
                    fn_name, param_count, config.max_params,
                ),
                severity: DiagnosticSeverity::HINT,
                source: "praetor/metrics".into(),
            });
        }
    }

    diags
}

/// Count decision points in a node's subtree.
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

/// Compute cognitive complexity with nesting weighting.
fn compute_cognitive(node: Node, depth: u32) -> u32 {
    let mut complexity = 0;
    let kind = node.kind();

    let is_decision = DECISION_KINDS.contains(&kind) || LOGICAL_OPS.contains(&kind);

    if is_decision {
        // Base complexity: 1 + depth (nesting penalty)
        complexity += 1 + depth;
    }

    let new_depth = if is_decision { depth + 1 } else { depth };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        complexity += compute_cognitive(child, new_depth);
    }
    complexity
}

/// Find the maximum nesting depth of control structures.
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

/// Count the number of parameters in a function's parameter list.
fn count_params(fn_node: Node) -> u32 {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        let k = child.kind();
        if k == "parameters" || k == "formal_parameters" || k.ends_with("parameters") {
            let mut count = 0;
            let mut pc = child.walk();
            for param in child.children(&mut pc) {
                let pk = param.kind();
                if !matches!(pk, "," | ":" | "(" | ")" | "->" | "=>")
                    && !pk.ends_with("type") && !pk.ends_with("annotation")
                    && !pk.ends_with("pattern")
                {
                    count += 1;
                }
            }
            return count;
        }
    }
    0
}
