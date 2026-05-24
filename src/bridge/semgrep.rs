use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::checks::CheckDiagnostic;

use super::{bridge_diagnostic, resolve_tool, tool_is_available, Bridge};

#[derive(Debug, Deserialize)]
struct SemgrepOutput {
    results: Vec<SemgrepResult>,
}

#[derive(Debug, Deserialize)]
struct SemgrepResult {
    check_id: String,
    #[allow(dead_code)]
    path: String,
    start: SemgrepPosition,
    #[allow(dead_code)]
    end: SemgrepPosition,
    extra: SemgrepExtra,
}

#[derive(Debug, Deserialize)]
struct SemgrepPosition {
    line: u32,
    col: u32,
}

#[derive(Debug, Deserialize)]
struct SemgrepExtra {
    message: String,
    severity: String,
}

pub struct SemgrepBridge;

impl Bridge for SemgrepBridge {
    fn name(&self) -> &str {
        "semgrep"
    }

    fn is_available(&self) -> bool {
        tool_is_available("semgrep")
    }

    fn run(&self, file_path: &Path, _source: &[u8]) -> Vec<CheckDiagnostic> {
        let bin_path = resolve_tool("semgrep");

        let output = match Command::new(&bin_path)
            .args(["--json", "--no-git-ignore", "--no-autofix"])
            .arg(file_path)
            .output()
        {
            Ok(o) => o,
            Err(_) => return vec![],
        };

        if !output.status.success() && output.stdout.is_empty() {
            return vec![];
        }

        let semgrep_out: SemgrepOutput = match serde_json::from_slice(&output.stdout) {
            Ok(o) => o,
            Err(_) => return vec![],
        };

        semgrep_out
            .results
            .into_iter()
            .map(|r| {
                let sev = match r.extra.severity.to_lowercase().as_str() {
                    "error" => DiagnosticSeverity::ERROR,
                    "warning" | "warn" => DiagnosticSeverity::WARNING,
                    _ => DiagnosticSeverity::INFORMATION,
                };
                bridge_diagnostic(
                    r.start.line.saturating_sub(1),
                    r.start.col.saturating_sub(1),
                    &format!("[Semgrep Rule {}] {}", r.check_id, r.extra.message),
                    sev,
                    "semgrep",
                )
            })
            .collect()
    }
}
