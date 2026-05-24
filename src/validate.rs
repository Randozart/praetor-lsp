use std::sync::Arc;

use tower_lsp::lsp_types::DiagnosticSeverity;

/// Run `praetor validate` — CI gate that exits 1 on unproven diagnostics.
pub fn run_validate(target: &str, warn_only: bool, json_output: bool) {
    let engine = Arc::new(crate::ast::AstEngine::new());
    let cfg = crate::config::PraetorConfig::discover();
    let rep = crate::report::Report::new(engine, cfg);
    let analysis = rep.analyze_project(target);

    let failures = collect_failures(&analysis, warn_only);

    if json_output {
        print_json(&analysis, &failures);
    } else {
        print_human(&failures);
    }

    if !failures.is_empty() {
        std::process::exit(1);
    }
}

fn collect_failures<'a>(
    analysis: &'a crate::report::ProjectAnalysis,
    warn_only: bool,
) -> Vec<(&'a str, &'a crate::report::FileResult, &'a crate::checks::CheckDiagnostic)> {
    let mut failures = Vec::new();
    for fr in &analysis.file_results {
        for d in &fr.diagnostics {
            let sev = d.severity;
            if sev == DiagnosticSeverity::ERROR || (!warn_only && sev == DiagnosticSeverity::WARNING) {
                failures.push((fr.path.as_str(), fr, d));
            }
        }
    }
    failures
}

fn print_json(
    analysis: &crate::report::ProjectAnalysis,
    failures: &[(&str, &crate::report::FileResult, &crate::checks::CheckDiagnostic)],
) {
    let json = serde_json::json!({
        "passed": failures.is_empty(),
        "total_diagnostics": analysis.diagnostics.iter().map(|(_, c)| c).sum::<usize>(),
        "failures": failures.iter().map(|(path, _, d)| {
            serde_json::json!({
                "file": path,
                "line": d.range.start.line + 1,
                "severity": format!("{:?}", d.severity),
                "source": d.source,
                "message": d.message,
            })
        }).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
}

fn print_human(
    failures: &[(&str, &crate::report::FileResult, &crate::checks::CheckDiagnostic)],
) {
    if failures.is_empty() {
        println!("[PASS] Praetor validation passed — no unproven diagnostics found");
    } else {
        println!("[FAIL] Praetor validation failed — {} unproven diagnostic(s):", failures.len());
        for (path, _, d) in failures {
            println!("  {}:{} | {} | {}", path, d.range.start.line + 1, d.source, d.message);
        }
    }
}
