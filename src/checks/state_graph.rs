use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;
use tower_lsp::lsp_types::{DiagnosticSeverity, Range};

use crate::ast::{find_child_by_path, node_text, ParsedFile};
use super::CheckDiagnostic;

#[derive(Debug, Clone, Deserialize)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StateGraph {
    #[allow(dead_code)]
    pub states: Vec<String>,
    #[allow(dead_code)]
    pub initial_state: Option<String>,
    pub transitions: Vec<StateTransition>,
    #[serde(default)]
    #[allow(dead_code)]
    pub function_patterns: Vec<FunctionPattern>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunctionPattern {
    #[allow(dead_code)]
    pub pattern: String,
    #[allow(dead_code)]
    pub transition: String,
}

impl StateGraph {
    pub fn load(path: &Path) -> Option<Self> {
        let contents = fs::read_to_string(path).ok()?;
        let graph: Self = serde_json::from_str(&contents).ok()?;
        Some(graph)
    }

    pub fn action_map(&self) -> HashMap<&str, Vec<(&str, &str)>> {
        let mut map: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for t in &self.transitions {
            map.entry(t.action.as_ref())
                .or_default()
                .push((t.from.as_ref(), t.to.as_ref()));
        }
        map
    }

    pub fn known_actions(&self) -> HashSet<&str> {
        self.transitions.iter().map(|t| t.action.as_ref()).collect()
    }
}

pub fn check_state_graph(
    parsed: &ParsedFile,
    graph: &StateGraph,
) -> Vec<CheckDiagnostic> {
    let mut diags = Vec::new();
    let amap = graph.action_map();
    let known_actions = graph.known_actions();

    let mut cursor = parsed.tree.root_node().walk();
    for child in parsed.tree.root_node().children(&mut cursor) {
        if !parsed.config.function_types.contains(&child.kind()) {
            continue;
        }
        let Some(name_node) = find_child_by_path(child, parsed.config.function_name_path) else {
            continue;
        };
        let fn_name_owned = node_text(name_node, parsed.text);
        if fn_name_owned.is_empty() {
            continue;
        }
        let fn_name: &str = &fn_name_owned;

        let body_text = node_text(child, parsed.text);
        let nrange = node_range(name_node);

        if known_actions.contains(fn_name) {
            check_exact_action(&mut diags, fn_name, &body_text, &nrange, &amap);
            continue;
        }
        let Some(matching_action) = known_actions.iter().find(|a| fn_name.contains(*a)) else {
            continue;
        };
        let Some(valid_transitions) = amap.get(matching_action) else {
            continue;
        };
        let valid_targets: Vec<&str> = valid_transitions.iter().map(|(_, to)| *to).collect();
        if !valid_targets.iter().any(|target| body_text.contains(*target)) {
            diags.push(transition_diag(
                &nrange,
                &format!("`{}` does not transition to any declared target state ({})", fn_name, valid_targets.join(", ")),
                DiagnosticSeverity::WARNING,
            ));
        }
    }
    diags
}

fn node_range(name_node: tree_sitter::Node) -> Range {
    use tower_lsp::lsp_types::Position;
    Range {
        start: Position {
            line: name_node.start_position().row as u32,
            character: name_node.start_position().column as u32,
        },
        end: Position {
            line: name_node.end_position().row as u32,
            character: name_node.end_position().column as u32,
        },
    }
}

fn check_exact_action(
    diags: &mut Vec<CheckDiagnostic>,
    fn_name: &str,
    body_text: &str,
    nrange: &Range,
    amap: &HashMap<&str, Vec<(&str, &str)>>,
) {
    let Some(txns) = amap.get(fn_name) else { return };
    let from_states: Vec<&str> = txns.iter().map(|(f, _)| *f).collect();
    let to_states: Vec<&str> = txns.iter().map(|(_, t)| *t).collect();

    push_from(diags, nrange, fn_name, &from_states, body_text);
    push_to(diags, nrange, fn_name, &to_states, body_text);
}

fn push_from(diags: &mut Vec<CheckDiagnostic>, nrange: &Range, fn_name: &str, states: &[&str], body_text: &str) {
    if states.len() == 1 && !states.iter().any(|s| body_text.contains(s)) {
        diags.push(transition_diag(nrange,
            &format!("`{}` should transition from `{}` but body does not reference that state", fn_name, states[0]),
            DiagnosticSeverity::HINT));
    }
}

fn push_to(diags: &mut Vec<CheckDiagnostic>, nrange: &Range, fn_name: &str, states: &[&str], body_text: &str) {
    if states.len() == 1 && !states.iter().any(|s| body_text.contains(s)) {
        diags.push(transition_diag(nrange,
            &format!("`{}` should transition to `{}` but body does not reference that state", fn_name, states[0]),
            DiagnosticSeverity::HINT));
    }
}

fn transition_diag(range: &Range, message: &str, severity: DiagnosticSeverity) -> CheckDiagnostic {
    CheckDiagnostic {
        range: *range,
        message: format!("[State Graph] {}", message),
        severity,
        source: "praetor/state-graph".into(),
    }
}
