use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::checks::CheckDiagnostic;
use crate::downloader::cache_root;

use super::{bridge_diagnostic, Bridge};

#[derive(Debug, Deserialize)]
struct InferReport(Vec<InferBug>);

#[derive(Debug, Deserialize)]
struct InferBug {
    bug_type: String,
    qualifier: String,
    severity: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
    file: String,
    procedure: Option<String>,
}

pub struct InferBridge;

impl Bridge for InferBridge {
    fn name(&self) -> &str {
        "infer"
    }

    fn is_available(&self) -> bool {
        let cache = cache_root();
        let bin_path = cache.join("bin").join("infer");
        bin_path.exists()
    }

    fn run(&self, file_path: &Path, _source: &[u8]) -> Vec<CheckDiagnostic> {
        let cache = cache_root();
        let bin_path = cache.join("bin").join("infer");
        let parent = file_path.parent().unwrap_or_else(|| Path::new("."));

        // Run infer on the file
        let _status = Command::new(&bin_path)
            .args(["run", "--"])
            .arg(file_path)
            .current_dir(parent)
            .output();

        // infer writes results to infer-out/report.json
        let report_path = parent.join("infer-out").join("report.json");
        if !report_path.exists() {
            return vec![];
        }

        let contents = match std::fs::read_to_string(&report_path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let report: InferReport = match serde_json::from_str(&contents) {
            Ok(r) => r,
            Err(_) => return vec![],
        };

        let diags: Vec<CheckDiagnostic> = report
            .0
            .into_iter()
            .filter(|b| {
                let target = file_path.to_string_lossy();
                b.file == target.as_ref() || b.file.ends_with(target.as_ref())
            })
            .map(|b| {
                let sev = match b.severity.as_deref() {
                    Some("ERROR") => DiagnosticSeverity::ERROR,
                    Some("WARNING") => DiagnosticSeverity::WARNING,
                    _ => DiagnosticSeverity::WARNING,
                };
                let proc = b.procedure.as_deref().unwrap_or("<unknown>");
                bridge_diagnostic(
                    b.line.unwrap_or(0).saturating_sub(1),
                    b.column.unwrap_or(0).saturating_sub(1),
                    &format!("[Infer {}] {} (in `{}`)", b.bug_type, b.qualifier, proc),
                    sev,
                    "infer",
                )
            })
            .collect();

        // Cleanup infer-out directory
        let _ = std::fs::remove_dir_all(parent.join("infer-out"));

        diags
    }
}
