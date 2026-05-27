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
    let source = parsed.text;
    let root = parsed.tree.root_node();
    let mut diags = Vec::new();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !CLASS_TYPES.contains(&child.kind()) {
            continue;
        }
        let Some(metrics) = analyze_class(child, source) else {
            continue;
        };
        let rng = class_range(child);
        let diagnostics = class_diagnostics(&metrics, &rng);
        diags.extend(diagnostics);
    }
    diags
}

struct ClassMetrics<'a> {
    name: &'a str,
    methods: u32,
    non_init_methods: u32,
    fields: u32,
    inheritance_depth: u32,
}

fn analyze_class<'a>(class_node: Node<'a>, source: &'a [u8]) -> Option<ClassMetrics<'a>> {
    let name = find_child_by_path(class_node, &["identifier"])
        .map(|n| node_text(n, source)).unwrap_or_default();
    if name.is_empty() {
        return None;
    }
    let body = find_class_body(class_node);
    let (methods, non_init_methods, fields) = count_body(body, source);
    let inheritance_depth = count_inheritance_depth(&class_node);
    Some(ClassMetrics { name, methods, non_init_methods, fields, inheritance_depth })
}

fn find_class_body(node: Node) -> Node {
    for body_kind in &["body", "class_body", "declaration_list", "block"] {
        if let Some(body) = find_child_by_path(node, &[body_kind]) {
            return body;
        }
    }
    node
}

fn count_body(body: Node, source: &[u8]) -> (u32, u32, u32) {
    let mut methods = 0u32;
    let mut non_init_methods = 0u32;
    let mut fields = 0u32;
    let mut bc = body.walk();
    for member in body.children(&mut bc) {
        let mk = member.kind();
        if METHOD_TYPES.contains(&mk) {
            methods += 1;
            if is_meaningful_method(member, source) {
                non_init_methods += 1;
            }
            continue;
        }
        if is_field_like(member, mk) {
            fields += 1;
        }
    }
    (methods, non_init_methods, fields)
}

fn is_field_like(member: Node, kind: &str) -> bool {
    if is_punctuation(kind) || is_docstring(member) {
        return false;
    }
    member.start_position().row != member.end_position().row
        || member.end_position().column - member.start_position().column > 2
}

fn is_meaningful_method(member: Node, source: &[u8]) -> bool {
    let method_name = find_child_by_path(member, &["identifier"])
        .map(|n| node_text(n, source)).unwrap_or("");
    if method_name.is_empty() { return false; }
    if method_name == "__init__" { return false; }
    !method_name.starts_with("__")
}

fn is_docstring(member: Node) -> bool {
    member.kind() == "expression_statement"
        && member.named_child_count() == 1
        && member.child(0).map(|c| c.kind()).unwrap_or("") == "string"
}

fn class_range(node: Node) -> Range {
    let start = node.start_position();
    Range {
        start: Position { line: start.row as u32, character: start.column as u32 },
        end: Position { line: node.end_position().row as u32, character: node.end_position().column as u32 },
    }
}

fn class_diagnostics(metrics: &ClassMetrics, rng: &Range) -> Vec<CheckDiagnostic> {
    let mut diags = Vec::new();
    if metrics.methods > GOD_METHOD_THRESHOLD {
        diags.push(CheckDiagnostic {
            range: *rng,
            message: format!(
                "[Architecture] `{}` has {} methods — may violate the \
                 Single Responsibility Principle (god object)",
                metrics.name, metrics.methods,
            ),
            severity: DiagnosticSeverity::WARNING,
            source: "praetor/architecture".into(),
            code: None,
        });
    }
    if metrics.fields > GOD_FIELD_THRESHOLD {
        diags.push(CheckDiagnostic {
            range: *rng,
            message: format!(
                "[Architecture] `{}` has {} fields — consider splitting \
                 into smaller domain objects",
                metrics.name, metrics.fields,
            ),
            severity: DiagnosticSeverity::WARNING,
            source: "praetor/architecture".into(),
            code: None,
        });
    }
    if metrics.fields > 2 && metrics.non_init_methods == 0 {
        diags.push(CheckDiagnostic {
            range: *rng,
            message: format!(
                "[Architecture] `{}` has {} fields but no methods — \
                 data class; consider adding behaviour",
                metrics.name, metrics.fields,
            ),
            severity: DiagnosticSeverity::HINT,
            source: "praetor/architecture".into(),
            code: None,
        });
    }
    if metrics.inheritance_depth > 3 {
        diags.push(CheckDiagnostic {
            range: *rng,
            message: format!(
                "[Architecture] `{}` has inheritance depth {} — deep \
                 hierarchies violate the Liskov Substitution Principle",
                metrics.name, metrics.inheritance_depth,
            ),
            severity: DiagnosticSeverity::HINT,
            source: "praetor/architecture".into(),
            code: None,
        });
    }
    diags
}

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
