use std::path::Path;

use crate::checks::CheckDiagnostic;
use crate::downloader::cache_root;

use super::Bridge;

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
        let cache = cache_root();
        let jar_path = cache.join("bin").join("sonarlint-language-server");
        if jar_path.exists() {
            return true;
        }
        let jar_path2 = cache.join("lib").join("sonarlint-language-server.jar");
        jar_path2.exists()
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
