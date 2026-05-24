use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};

use crate::ast::ParsedFile;
use crate::config::DatalogConfig;
use crate::facts::FactEngine;

use super::CheckDiagnostic;

pub fn check_facts(
    parsed: &ParsedFile,
    datalog_config: Option<&DatalogConfig>,
) -> Vec<CheckDiagnostic> {
    let results = FactEngine::analyze(parsed, datalog_config);
    results.into_iter().map(|fd| {
        let severity = match fd.rule_id {
            1 | 5 => DiagnosticSeverity::ERROR,
            4 => DiagnosticSeverity::WARNING,
            _ => DiagnosticSeverity::HINT,
        };
        let range = Range {
            start: Position {
                line: fd.line,
                character: fd.character,
            },
            end: Position {
                line: fd.line,
                character: fd.character + 1,
            },
        };
        CheckDiagnostic {
            range,
            message: fd.message,
            severity,
            source: format!("praetor/datalog-rule-{}", fd.rule_id),
        }
    }).collect()
}
