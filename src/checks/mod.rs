pub mod complexity;
pub mod facts;
pub mod intent;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range};

use crate::ast::{AstEngine, ParsedFile};
use crate::config::PraetorConfig;

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
    pub fn run(
        parsed: &ParsedFile,
        _engine: &AstEngine,
        config: &PraetorConfig,
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

        results
    }
}
