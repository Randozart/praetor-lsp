use std::path::Path;

use crate::checks::CheckDiagnostic;

use super::{tool_is_available, Bridge};

/// SonarLint bridge.
///
/// The SonarLint Language Server runs as an LSP server (not a one-shot CLI).
/// Full integration requires spawning the JAR as a subprocess and communicating
/// via the LSP protocol to receive diagnostics.
///
/// Until that is implemented, this bridge reports as unavailable.
pub struct SonarLintBridge;

impl Bridge for SonarLintBridge {
    fn name(&self) -> &str {
        "sonarlint"
    }

    fn is_available(&self) -> bool {
        tool_is_available("java")
    }

    fn run(&self, _file_path: &Path, _source: &[u8]) -> Vec<CheckDiagnostic> {
        // TODO: Spawn SonarLint LSP server as subprocess.
        // Steps:
        //   1. Start java -jar sonarlint-language-server.jar --stdio
        //   2. Send LSP initialize, textDocument/didOpen
        //   3. Read diagnostics from publishDiagnostics notifications
        //   4. Shut down
        // For now, return empty.
        tracing::warn!("SonarLint bridge not yet implemented — skipping analysis");
        vec![]
    }
}
