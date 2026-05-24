use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};
use tree_sitter::Node;

use crate::ast::{find_child_by_path, node_text, ParsedFile};
use super::CheckDiagnostic;

const CLASS_TYPES: &[&str] = &[
    "class_definition",
    "class_declaration",
    "class_specifier",
    "struct_specifier",
];

const METHOD_TYPES: &[&str] = &[
    "function_definition",
    "method_definition",
    "function_declaration",
    "method_declaration",
    "function_item",
];

const INHERITANCE_KINDS: &[&str] = &[
    "base_class",
    "superclass",
    "extends_clause",
    "implements_clause",
    "base_class_clause",
];

const GOD_METHOD_THRESHOLD: u32 = 15;
const GOD_FIELD_THRESHOLD: u32 = 10;

pub fn check_architecture(parsed: &ParsedFile) -> Vec<CheckDiagnostic> {
    let mut diags = Vec::new();
    let source = parsed.text;
    let root = parsed.tree.root_node();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let kind = child.kind();

        // Detect classes
        if !CLASS_TYPES.contains(&kind) {
            continue;
        }

        // Get class name
        let class_name = find_child_by_path(child, &["identifier"])
            .map(|n| node_text(n, source))
            .unwrap_or_default();

        if class_name.is_empty() {
            continue;
        }

        let class_start = child.start_position();
        let class_range = Range {
            start: Position {
                line: class_start.row as u32,
                character: class_start.column as u32,
            },
            end: Position {
                line: child.end_position().row as u32,
                character: child.end_position().column as u32,
            },
        };

        // Count methods and fields
        let mut method_count = 0u32;
        let mut non_init_methods = 0u32;
        let mut field_count = 0u32;
        let mut body_node = child;

        // Try to find the class body
        for body_kind in &["body", "class_body", "declaration_list", "block"] {
            if let Some(body) = find_child_by_path(child, &[body_kind]) {
                body_node = body;
                break;
            }
        }

        let mut bc = body_node.walk();
        for member in body_node.children(&mut bc) {
            let mk = member.kind();
            if METHOD_TYPES.contains(&mk) {
                method_count += 1;
                let method_name = find_child_by_path(member, &["identifier"])
                    .map(|n| node_text(n, source))
                    .unwrap_or("");
                if !method_name.is_empty() && method_name != "__init__" && !method_name.starts_with("__") {
                    non_init_methods += 1;
                }
            } else if !is_punctuation(mk) {
                // Count anything non-trivial as a potential field/member
                // Exclude docstrings and blank lines
                let is_docstring = mk == "expression_statement"
                    && member.named_child_count() == 1
                    && member.child(0).map(|c| c.kind()).unwrap_or("") == "string";
                if member.start_position().row != member.end_position().row
                    || member.end_position().column - member.start_position().column > 2
                {
                    if !is_docstring {
                        field_count += 1;
                    }
                }
            }
        }

        // Detect inheritance depth
        let inheritance_depth = count_inheritance_depth(&child);

        // God class: too many methods
        if method_count > GOD_METHOD_THRESHOLD {
            diags.push(CheckDiagnostic {
                range: class_range,
                message: format!(
                    "[Architecture] `{}` has {} methods — may violate the \
                     Single Responsibility Principle (god object)",
                    class_name, method_count,
                ),
                severity: DiagnosticSeverity::WARNING,
                source: "praetor/architecture".into(),
            });
        }

        // God class: too many fields
        if field_count > GOD_FIELD_THRESHOLD {
            diags.push(CheckDiagnostic {
                range: class_range,
                message: format!(
                    "[Architecture] `{}` has {} fields — consider splitting \
                     into smaller domain objects",
                    class_name, field_count,
                ),
                severity: DiagnosticSeverity::WARNING,
                source: "praetor/architecture".into(),
            });
        }

        // Data class: fields but no non-initializer methods
        if field_count > 2 && non_init_methods == 0 {
            diags.push(CheckDiagnostic {
                range: class_range,
                message: format!(
                    "[Architecture] `{}` has {} fields but no methods — \
                     data class; consider adding behaviour",
                    class_name, field_count,
                ),
                severity: DiagnosticSeverity::HINT,
                source: "praetor/architecture".into(),
            });
        }

        // Deep inheritance hierarchy
        if inheritance_depth > 3 {
            diags.push(CheckDiagnostic {
                range: class_range,
                message: format!(
                    "[Architecture] `{}` has inheritance depth {} — deep \
                     hierarchies violate the Liskov Substitution Principle",
                    class_name, inheritance_depth,
                ),
                severity: DiagnosticSeverity::HINT,
                source: "praetor/architecture".into(),
            });
        }
    }

    diags
}

/// Count the depth of inheritance by following base-class references.
fn count_inheritance_depth(class_node: &Node) -> u32 {
    for ik in INHERITANCE_KINDS {
        if find_child_by_path(*class_node, &[ik]).is_some() {
            return 1;
        }
    }
    0
}

fn is_punctuation(kind: &str) -> bool {
    matches!(kind, ";" | "," | ":" | "(" | ")" | "{" | "}" | "[" | "]")
}
