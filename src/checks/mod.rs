pub mod complexity;
pub mod facts;
pub mod intent;
pub mod state_graph;

use std::path::Path;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range};

use crate::ast::{AstEngine, ParsedFile};
use crate::config::PraetorConfig;

use self::state_graph::StateGraph;

/// A single result from a check.
#[derive(Debug, Clone)]
pub struct CheckDiagnostic {
    pub range: Range,
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub source: String,
}

impl From<CheckDiagnostic> for Diagnostic {
    fn from(cd: CheckDiagnostic) -> Self {
        Diagnostic {
            range: cd.range,
            severity: Some(cd.severity),
            source: Some(cd.source),
            message: cd.message,
            ..Default::default()
        }
    }
}

/// Pipeline that runs all checks on a parsed file.
pub struct CheckPipeline;

impl CheckPipeline {
    /// Run all checks. Optionally pass the path to .praetor/ for state graph discovery.
    pub fn run(
        parsed: &ParsedFile,
        _engine: &AstEngine,
        config: &PraetorConfig,
        praetor_dir: Option<&Path>,
    ) -> Vec<CheckDiagnostic> {
        let mut results = Vec::new();

        if config.complexity.big_o_threshold != "disabled" {
            results.extend(complexity::check_complexity(
                parsed,
                &config.complexity,
            ));
        }

        if config.intent.enabled {
            results.extend(intent::check_intent(
                parsed,
                &config.intent,
            ));
        }

        // Datalog facts check (always runs — built-in rules)
        results.extend(facts::check_facts(parsed));

        // State graph validation (if .praetor/state-graph.json exists)
        if let Some(dir) = praetor_dir {
            let state_graph_path = dir.join("state-graph.json");
            if state_graph_path.is_file() {
                if let Some(graph) = StateGraph::load(&state_graph_path) {
                    results.extend(state_graph::check_state_graph(parsed, &graph));
                }
            }
        }

        results
    }
}
