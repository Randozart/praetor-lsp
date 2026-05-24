use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;
use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};

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
    /// Load a state graph from a JSON file path.
    pub fn load(path: &Path) -> Option<Self> {
        let contents = fs::read_to_string(path).ok()?;
        let graph: Self = serde_json::from_str(&contents).ok()?;
        Some(graph)
    }

    /// Build a lookup: action -> Vec<(from, to)>
    pub fn action_map(&self) -> HashMap<&str, Vec<(&str, &str)>> {
        let mut map: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for t in &self.transitions {
            map.entry(t.action.as_ref())
                .or_default()
                .push((t.from.as_ref(), t.to.as_ref()));
        }
        map
    }

    /// Build a set of all known actions
    pub fn known_actions(&self) -> HashSet<&str> {
        self.transitions.iter().map(|t| t.action.as_ref()).collect()
    }

    /// Build a set of all valid state names
    pub fn known_states(&self) -> HashSet<&str> {
        self.states.iter().map(|s| s.as_ref()).collect()
    }
}

pub fn check_state_graph(
    parsed: &ParsedFile,
    graph: &StateGraph,
) -> Vec<CheckDiagnostic> {
    let mut diags = Vec::new();
    let lang = parsed.config;
    let source = parsed.text;
    let root = parsed.tree.root_node();
    let known_actions = graph.known_actions();
    let known_states = graph.known_states();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !lang.function_types.contains(&child.kind()) {
            continue;
        }
        let name_node = match find_child_by_path(child, lang.function_name_path) {
            Some(n) => n,
            None => continue,
        };
        let fn_name_owned = node_text(name_node, source);
        if fn_name_owned.is_empty() {
            continue;
        }
        let fn_name: &str = &fn_name_owned;

        // Heuristic 1: function name matches a known action
        let amap = graph.action_map();

        if !known_actions.contains(fn_name) {
            // Heuristic 2: function name contains a known action as substring
            let has_known_action = known_actions.iter().any(|a| fn_name.contains(a));
            // Heuristic 3: function name references known states
            let has_known_state = known_states.iter().any(|s| fn_name.contains(s));

            if !has_known_action && !has_known_state {
                // Not obviously a transition function — skip
                continue;
            }
        }

        // Check function body for state-related patterns
        let body = child;
        let body_text = node_text(body, source);

        // Heuristic: if function name is a known action, check that the body
        // references one of the valid target states for that action
        if let Some(matching_action) = known_actions.iter().find(|a| fn_name.contains(*a)) {
            if let Some(valid_transitions) = amap.get(matching_action) {
                let valid_targets: Vec<&str> = valid_transitions
                    .iter()
                    .map(|(_, to)| *to)
                    .collect();

                let has_valid_target = valid_targets
                    .iter()
                    .any(|target| body_text.contains(*target));

                if !has_valid_target {
                    diags.push(CheckDiagnostic {
                        range: Range {
                            start: Position {
                                line: name_node.start_position().row as u32,
                                character: name_node.start_position().column as u32,
                            },
                            end: Position {
                                line: name_node.end_position().row as u32,
                                character: name_node.end_position().column as u32,
                            },
                        },
                        message: format!(
                            "[State Graph] `{}` does not transition to any declared target state \
                             ({})",
                            fn_name,
                            valid_targets.join(", "),
                        ),
                        severity: DiagnosticSeverity::WARNING,
                        source: "praetor/state-graph".into(),
                    });
                }
            }
        }

        if known_actions.contains(fn_name) {
            // Function name is exactly a known action — verify it's used
            // in a valid state context
            let transitions = amap.get(fn_name);
            if let Some(txns) = transitions {
                let from_states: Vec<&str> = txns.iter().map(|(f, _)| *f).collect();
                let to_states: Vec<&str> = txns.iter().map(|(_, t)| *t).collect();

                // Check if the function body references at least one valid from-state
                let has_valid_from = from_states.iter().any(|s| body_text.contains(*s));
                if !has_valid_from && from_states.len() == 1 {
                    diags.push(CheckDiagnostic {
                        range: Range {
                            start: Position {
                                line: name_node.start_position().row as u32,
                                character: name_node.start_position().column as u32,
                            },
                            end: Position {
                                line: name_node.end_position().row as u32,
                                character: name_node.end_position().column as u32,
                            },
                        },
                        message: format!(
                            "[State Graph] `{}` should transition from `{}` but \
                             body does not reference that state",
                            fn_name,
                            from_states[0],
                        ),
                        severity: DiagnosticSeverity::HINT,
                        source: "praetor/state-graph".into(),
                    });
                }

                // Check if the function body references at least one valid to-state
                let has_valid_to = to_states.iter().any(|s| body_text.contains(*s));
                if !has_valid_to && to_states.len() == 1 {
                    diags.push(CheckDiagnostic {
                        range: Range {
                            start: Position {
                                line: name_node.start_position().row as u32,
                                character: name_node.start_position().column as u32,
                            },
                            end: Position {
                                line: name_node.end_position().row as u32,
                                character: name_node.end_position().column as u32,
                            },
                        },
                        message: format!(
                            "[State Graph] `{}` should transition to `{}` but \
                             body does not reference that state",
                            fn_name,
                            to_states[0],
                        ),
                        severity: DiagnosticSeverity::HINT,
                        source: "praetor/state-graph".into(),
                    });
                }
            }
        }
    }

    diags
}
