use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::checks::CheckDiagnostic;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowResult {
    pub original_hash: String,
    pub shadow_hash: String,
    pub winner: String,
    pub ratio: f64,
    pub improvement: HashMap<String, MetricDelta>,
    pub suppressed_diagnostics: Vec<String>,
    pub verified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDelta {
    pub before: u32,
    pub after: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShadowRegistry {
    pub entries: HashMap<String, ShadowResult>,
}

#[allow(dead_code)]
impl ShadowRegistry {
    /// Load the registry from `.praetor/shadow-results.json`.
    pub fn load(praetor_dir: &Path) -> Self {
        let path = praetor_dir.join("shadow-results.json");
        if !path.is_file() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save the registry to `.praetor/shadow-results.json`.
    pub fn save(&self, praetor_dir: &Path) {
        let path = praetor_dir.join("shadow-results.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, &content);
        }
    }

    /// Check if a diagnostic should be suppressed for a given function.
    /// `function_body` is the current source text of the function — used
    /// to verify the registry entry is not stale.
    pub fn is_suppressed(
        &self,
        function_name: &str,
        diagnostic_source: &str,
        function_body: &str,
    ) -> bool {
        let Some(entry) = self.entries.get(function_name) else {
            return false;
        };
        if entry.winner != "original" {
            return false;
        }
        // Stale entry — function body changed since verification
        if entry.original_hash != hash_source(function_body) {
            return false;
        }
        entry.suppressed_diagnostics.iter().any(|s| diagnostic_source.contains(s))
    }

    /// Get the list of suppressed diagnostics for a function.
    pub fn suppressed_sources(&self, function_name: &str) -> Vec<&str> {
        self.entries
            .get(function_name)
            .map(|e| e.suppressed_diagnostics.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Register a shadow verification result.
    pub fn register(
        &mut self,
        function_name: &str,
        original_source: &str,
        shadow_source: &str,
        winner: &str,
        ratio: f64,
        improvement: HashMap<String, MetricDelta>,
        suppressed_diagnostics: Vec<String>,
    ) {
        let original_hash = hash_source(original_source);
        let shadow_hash = hash_source(shadow_source);
        self.entries.insert(
            function_name.to_string(),
            ShadowResult {
                original_hash,
                shadow_hash,
                winner: winner.to_string(),
                ratio,
                improvement,
                suppressed_diagnostics,
                verified_at: chrono_now(),
            },
        );
    }
}

/// Hash a function body for cache invalidation.
#[allow(dead_code)]
pub fn hash_source(source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    format!("sha256-{:x}", hasher.finalize())
}

/// Get the current timestamp as an ISO-like string.
fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = now.as_secs();
    let secs = total_secs % 60;
    let mins = (total_secs / 60) % 60;
    let hours = (total_secs / 3600) % 24;
    let days = total_secs / 86400;
    // Approximate from 2024-01-01 to keep it simple
    let year = 2024u64 + days / 366;
    let day_of_year = days % 366;
    let month = day_of_year / 31 + 1;
    let day = day_of_year % 31 + 1;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month.min(12), day.min(28), hours, mins, secs)
}

/// Given a parsed file + its diagnostics, cross-reference with the registry
/// and filter out any diagnostics that belong to functions with valid
/// shadow verification entries.
pub fn suppress_in_file(
    diagnostics: Vec<CheckDiagnostic>,
    registry: &ShadowRegistry,
    file_config: &crate::ast::LanguageConfig,
    root: tree_sitter::Node,
    source: &[u8],
) -> Vec<CheckDiagnostic> {
    if registry.entries.is_empty() {
        return diagnostics;
    }

    let mut line_to_fn: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    let mut fn_bodies: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    collect_function_lines(root, file_config, source, &mut line_to_fn, &mut fn_bodies);

    diagnostics
        .into_iter()
        .filter(|d| {
            let line = d.range.start.line;
            match line_to_fn.get(&line) {
                Some(fn_name) => {
                    let body = fn_bodies.get(fn_name).map(|s| s.as_str()).unwrap_or("");
                    !registry.is_suppressed(fn_name, &d.source, body)
                }
                None => true,
            }
        })
        .collect()
}

/// Recursively walk the AST and map every line to the enclosing function name.
fn collect_function_lines(
    node: tree_sitter::Node,
    config: &crate::ast::LanguageConfig,
    source: &[u8],
    line_to_fn: &mut std::collections::HashMap<u32, String>,
    fn_bodies: &mut std::collections::HashMap<String, String>,
) {
    if config.function_types.contains(&node.kind()) {
        if let Some(name_node) = crate::ast::find_child_by_path(node, config.function_name_path) {
            let fn_name = crate::ast::node_text(name_node, source);
            let start_line = node.start_position().row as u32;
            let end_line = node.end_position().row as u32;
            for line in start_line..=end_line {
                line_to_fn.entry(line).or_insert_with(|| fn_name.to_string());
            }
            // Store function body text for hash verification
            let fn_source = crate::ast::node_text(node, source).to_string();
            fn_bodies.entry(fn_name.to_string()).or_insert(fn_source);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_function_lines(child, config, source, line_to_fn, fn_bodies);
    }
}
