pub mod architecture;
pub mod complexity;
pub mod facts;
pub mod intent;
pub mod metrics;
pub mod state_graph;

use std::path::Path;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range};

use crate::ast::{AstEngine, ParsedFile};
use crate::config::PraetorConfig;

use self::state_graph::StateGraph;

#[derive(Debug, Clone)]
pub struct CheckDiagnostic {
    pub range: Range,
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub source: String,
    pub code: Option<String>,
}

impl CheckDiagnostic {
    pub fn new(range: Range, message: String, severity: DiagnosticSeverity, source: String, code: Option<String>) -> Self {
        Self { range, message, severity, source, code }
    }
}

impl From<CheckDiagnostic> for Diagnostic {
    fn from(cd: CheckDiagnostic) -> Self {
        Diagnostic {
            range: cd.range,
            severity: Some(cd.severity),
            source: Some(cd.source),
            message: cd.message,
            code: cd.code.map(|c| tower_lsp::lsp_types::NumberOrString::String(c)),
            ..Default::default()
        }
    }
}

pub struct CheckPipeline;

impl CheckPipeline {
    pub fn run(
        parsed: &ParsedFile,
        _engine: &AstEngine,
        config: &PraetorConfig,
        praetor_dir: Option<&Path>,
    ) -> Vec<CheckDiagnostic> {
        let mut results = Vec::new();
        if config.complexity.big_o_threshold != "disabled" {
            results.extend(complexity::check_complexity(parsed, &config.complexity));
        }
        if config.complexity.cyclomatic_max > 0 || config.complexity.cognitive_max > 0 {
            results.extend(metrics::check_metrics(parsed, &config.complexity));
        }
        if config.intent.enabled {
            results.extend(intent::check_intent(parsed, &config.intent));
        }
        results.extend(facts::check_facts(parsed, Some(&config.datalog)));
        if config.state_graph.enabled {
            if let Some(dir) = praetor_dir {
                let state_graph_path = dir.join(&config.state_graph.path);
                if state_graph_path.is_file() {
                    if let Some(graph) = StateGraph::load(&state_graph_path) {
                        results.extend(state_graph::check_state_graph(parsed, &graph));
                    }
                }
            }
        }
        // Architecture/SOLID heuristics (always runs)
        results.extend(architecture::check_architecture(parsed));
        results
    }
}