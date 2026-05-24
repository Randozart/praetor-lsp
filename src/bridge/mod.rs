pub mod infer;
pub mod semgrep;
pub mod sonarlint;

use std::path::Path;

use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::checks::CheckDiagnostic;

/// A bridge to an external verification tool.
pub trait Bridge {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn run(&self, file_path: &Path, source: &[u8]) -> Vec<CheckDiagnostic>;
}

/// Runs all available bridges and collects their diagnostics.
pub fn run_all_bridges(
    bridges: &[Box<dyn Bridge + Send + Sync>],
    file_path: &Path,
    source: &[u8],
) -> Vec<CheckDiagnostic> {
    let mut results = Vec::new();
    for bridge in bridges {
        if bridge.is_available() {
            results.extend(bridge.run(file_path, source));
        }
    }
    results
}

/// Build a CheckDiagnostic from a bridge result.
pub fn bridge_diagnostic(
    line: u32,
    column: u32,
    message: &str,
    severity: DiagnosticSeverity,
    source: &str,
) -> CheckDiagnostic {
    CheckDiagnostic {
        range: tower_lsp::lsp_types::Range {
            start: tower_lsp::lsp_types::Position { line, character: column },
            end: tower_lsp::lsp_types::Position {
                line,
                character: column + 1,
            },
        },
        message: message.to_string(),
        severity,
        source: format!("praetor/bridge-{}", source),
    }
}
