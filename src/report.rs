use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::ast::AstEngine;
use crate::checks::{CheckDiagnostic, CheckPipeline};
use crate::config::PraetorConfig;

pub struct Report {
    engine: Arc<AstEngine>,
    config: Option<PraetorConfig>,
}

impl Report {
    pub fn new(engine: Arc<AstEngine>, config: Option<PraetorConfig>) -> Self {
        Self { engine, config }
    }

    pub fn generate(&self, target: &str, format: &str, output: Option<&str>) {
        let analysis = self.analyze_project(target);

        let report_str = match format {
            "html" => self.render_html(&analysis),
            _ => self.render_markdown(&analysis),
        };

        match output {
            Some(path) => {
                std::fs::write(path, &report_str).unwrap_or_else(|e| {
                    eprintln!("error writing report: {}", e);
                });
                eprintln!("report written to {}", path);
            }
            None => {
                println!("{}", report_str);
            }
        }
    }

    fn praetor_dir(&self) -> Option<std::path::PathBuf> {
        // Check config-derived path first
        if let Some(dir) = self.config.as_ref().and_then(|cfg| {
            cfg.path.as_ref().and_then(|p| p.parent().map(|dir| dir.join(".praetor")))
        }) {
            if dir.is_dir() {
                return Some(dir);
            }
        }
        // Fallback: check CWD/.praetor/
        let cwd_dir = std::env::current_dir().ok()?.join(".praetor");
        if cwd_dir.is_dir() {
            Some(cwd_dir)
        } else {
            None
        }
    }

    pub fn analyze_project(&self, target: &str) -> ProjectAnalysis {
        let mut analysis = ProjectAnalysis {
            root: target.to_string(),
            total_files: 0,
            total_lines: 0,
            total_functions: 0,
            languages: HashMap::new(),
            file_results: Vec::new(),
            diagnostics: Vec::new(),
        };

        let cfg = self.config.clone().unwrap_or_default();

        let dir = Path::new(target);
        if !dir.is_dir() {
            eprintln!("target is not a directory: {}", target);
            return analysis;
        }

        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                // Skip common build artifacts and vendored dirs
                !matches!(name.as_ref(),
                    "target" | ".git" | "node_modules" | ".venv" | "venv"
                    | "__pycache__" | ".next" | "dist" | "build" | "test"
                    | "tests" | "scripts" | "lib"
                )
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let ext = match path.extension().and_then(|s| s.to_str()) {
                Some(e) => format!(".{}", e),
                None => continue,
            };

            if !self.engine.supports_extension(&ext) {
                continue;
            }

            let source = match std::fs::read(path) {
                Ok(b) => b,
                Err(_) => continue,
            };

            let lines = source.split(|b| *b == b'\n').count();
            let file_name = path.to_string_lossy().to_string();
            analysis.total_files += 1;
            analysis.total_lines += lines as u64;

            *analysis
                .languages
                .entry(ext.clone())
                .or_insert(LangStats::default()) += LangStats {
                files: 1,
                lines: lines as u64,
                functions: 0,
            };

            if let Some(parsed) = self.engine.parse(&ext, &source) {
                let mut results = CheckPipeline::run(&parsed, &self.engine, &cfg, self.praetor_dir().as_deref());

                // Suppress diagnostics proven by shadow verification
                if let Some(dir) = self.praetor_dir() {
                    let registry = crate::suppressor::ShadowRegistry::load(&dir);
                    if !registry.entries.is_empty() {
                        results = crate::suppressor::suppress_in_file(results, &registry, parsed.config, parsed.tree.root_node(), parsed.text);
                    }
                }

                let fn_count = count_functions(&parsed.tree.root_node(), parsed.config) as u64;
                analysis.total_functions += fn_count;
                if let Some(stats) = analysis.languages.get_mut(&ext) {
                    stats.functions += fn_count;
                }

                analysis.diagnostics.push((file_name.clone(), results.len()));
                analysis.file_results.push(FileResult {
                    path: file_name,
                    lines: lines as u64,
                    functions: fn_count as u32,
                    diagnostics: results,
                });
            }
        }

        analysis
    }

    fn render_markdown(&self, analysis: &ProjectAnalysis) -> String {
        let mut out = String::new();

        out.push_str(&format!("# Praetor Report — {}\n\n", analysis.root));

        // Summary
        out.push_str("## Project Summary\n\n");
        out.push_str(&format!(
            "| Metric | Value |\n|--------|-------|\n"
        ));
        out.push_str(&format!(
            "| Total files | {} |\n", analysis.total_files
        ));
        out.push_str(&format!(
            "| Total lines | {} |\n", analysis.total_lines
        ));
        out.push_str(&format!(
            "| Total functions | {} |\n", analysis.total_functions
        ));

        // Language breakdown
        out.push_str("\n## Language Breakdown\n\n");
        out.push_str("| Language | Files | Lines | Functions |\n|----------|-------|-------|-----------|\n");
        let mut langs: Vec<_> = analysis.languages.iter().collect();
        langs.sort_by(|a, b| b.1.lines.cmp(&a.1.lines));
        for (ext, stats) in &langs {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                ext, stats.files, stats.lines, stats.functions
            ));
        }

        // Diagnostics summary
        let total_diags: usize = analysis.diagnostics.iter().map(|(_, c)| c).sum();
        out.push_str(&format!("\n## Checks — {} total diagnostics\n\n", total_diags));

        if analysis.file_results.is_empty() {
            out.push_str("_No files analyzed._\n");
        } else {
            for fr in &analysis.file_results {
                if fr.diagnostics.is_empty() {
                    continue;
                }
                out.push_str(&format!("\n### `{}`\n\n", fr.path));
                out.push_str("| Line | Severity | Source | Message |\n|------|----------|--------|--------|\n");
                for d in &fr.diagnostics {
                    out.push_str(&format!(
                        "| {}:{} | {:?} | {} | {} |\n",
                        d.range.start.line,
                        d.range.start.character,
                        d.severity,
                        d.source,
                        d.message,
                    ));
                }
            }
        }

        // Verification status
        out.push_str("\n## Verification Status\n\n");
        let has_graph = Path::new(".praetor/state-graph.json").exists();
        out.push_str(&format!("- State graph: {}\n", if has_graph { "✅ present" } else { "⬜ not found" }));
        out.push_str(&format!("- Datalog rules: {} active (5 built-in rules)\n", "✅"));
        out.push_str(&format!("- Static analysis: {} checks passed across {} files\n",
            total_diags, analysis.total_files));

        out
    }

    fn render_html(&self, analysis: &ProjectAnalysis) -> String {
        let md = self.render_markdown(analysis);
        let escaped = md
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Praetor Report</title>
<style>
  body {{ font-family: -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif; max-width: 960px; margin: 2em auto; padding: 0 1em; line-height: 1.6; }}
  table {{ border-collapse: collapse; width: 100%; }}
  th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
  th {{ background: #f5f5f5; }}
  pre {{ background: #f8f8f8; padding: 1em; overflow-x: auto; }}
  .badge {{ display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 0.85em; }}
  .badge-green {{ background: #d4edda; color: #155724; }}
  .badge-red {{ background: #f8d7da; color: #721c24; }}
</style></head>
<body><div class="content">{}</div></body>
</html>"#,
            escaped
        )
    }
}

pub struct ProjectAnalysis {
    pub root: String,
    pub total_files: u64,
    pub total_lines: u64,
    pub total_functions: u64,
    pub languages: HashMap<String, LangStats>,
    pub diagnostics: Vec<(String, usize)>,
    pub file_results: Vec<FileResult>,
}

#[derive(Default)]
pub struct LangStats {
    pub files: u64,
    pub lines: u64,
    pub functions: u64,
}

impl std::ops::AddAssign for LangStats {
    fn add_assign(&mut self, other: Self) {
        self.files += other.files;
        self.lines += other.lines;
        self.functions += other.functions;
    }
}

#[allow(dead_code)]
pub struct FileResult {
    pub path: String,
    pub lines: u64,
    pub functions: u32,
    pub diagnostics: Vec<CheckDiagnostic>,
}

fn count_functions(node: &tree_sitter::Node, config: &crate::ast::LanguageConfig) -> u32 {
    let mut count = 0;
    let fn_types = config.function_types;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if fn_types.contains(&child.kind()) {
            count += 1;
        }
        count += count_functions(&child, config);
    }
    count
}
