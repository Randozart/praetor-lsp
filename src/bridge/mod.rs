pub mod infer;
pub mod semgrep;
pub mod sonarlint;

use std::path::{Path, PathBuf};
use std::process::Command;

use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::checks::CheckDiagnostic;
use crate::downloader::cache_root;

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

/// Check if a tool is available either on PATH or in the cache.
pub fn tool_is_available(name: &str) -> bool {
    if cache_root().join("bin").join(name).exists() {
        return true;
    }
    Command::new(name).arg("--version").output().is_ok()
}

/// Resolve the path to a tool binary. Prefers cached version, falls back to PATH.
pub fn resolve_tool(name: &str) -> PathBuf {
    let cached = cache_root().join("bin").join(name);
    if cached.exists() {
        cached
    } else {
        PathBuf::from(name)
    }
}
