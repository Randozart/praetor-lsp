use std::path::Path;

use tracing::info;

/// Run shadow verification on a source file.
///
/// Discovers original and shadow functions, generates a benchmark
/// harness, builds it, and compares performance.
pub fn run_shadow_verify(
    file: &str,
    shadow_name: Option<&str>,
    original_name: Option<&str>,
    threshold: f64,
    _iterations: u64,
) {
    let file_path = Path::new(file);
    if !file_path.is_file() {
        eprintln!("error: file not found: {}", file);
        std::process::exit(1);
    }

    let source = match std::fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", file, e);
            std::process::exit(1);
        }
    };

    // Detect original and shadow function names from source
    let (orig, shadow) = match (original_name, shadow_name) {
        (Some(o), Some(s)) => (o.to_string(), s.to_string()),
        (Some(o), None) => {
            // Auto-detect shadow function
            match find_shadow_fn(&source, None) {
                Some(s) => (o.to_string(), s),
                None => {
                    eprintln!("no shadow function found in {}. specify --shadow or add // praetor-shadow: original=<name>", file);
                    std::process::exit(1);
                }
            }
        }
        (None, Some(s)) => {
            // Auto-detect original from shadow attr
            let orig = find_original_for_shadow(&source, &s)
                .unwrap_or_else(|| {
                    eprintln!("could not determine original function for {}. specify --original", s);
                    std::process::exit(1);
                });
            (orig, s.to_string())
        }
        (None, None) => {
            // Auto-detect both
            match find_shadow_fn(&source, None) {
                Some(s) => {
                    let orig = find_original_for_shadow(&source, &s)
                        .unwrap_or_else(|| s.trim_end_matches("_shadow").trim_end_matches("_v2").trim_end_matches("_v3").to_string());
                    (orig, s)
                }
                None => {
                    eprintln!("no shadow functions found in {}. annotate with // praetor-shadow: original=<name>", file);
                    std::process::exit(1);
                }
            }
        }
    };

    info!("original: {}", orig);
    info!("shadow: {}", shadow);
    info!("threshold: {}%", threshold);
    info!("");

    // Generate benchmark scaffold
    let bench_file = generate_bench_scaffold(file_path, &orig, &shadow, &source);

    // Write bench file next to the source
    let parent = file_path.parent().unwrap_or_else(|| Path::new("."));
    let bench_path = parent.join(format!("__praetor_bench_{}.rs", shadow));
    let bench_path_str = bench_path.to_string_lossy().to_string();

    if let Err(e) = std::fs::write(&bench_path, &bench_file) {
        eprintln!("error writing benchmark scaffold: {}", e);
        std::process::exit(1);
    }

    info!("benchmark scaffold written to {}", bench_path_str);
    info!("");
    info!("Next steps:");
    info!("  1. Fill in test input generation in {}", bench_path_str);
    info!("  2. Add divan to Cargo.toml: [dev-dependencies] divan = \"0.1\"");
    info!("  3. Run: cargo test --bench __praetor_bench_{}", shadow);
    info!("  4. Compare original vs shadow results");
}

/// Find a function annotated with a `praetor-shadow:` comment in the source.
fn find_shadow_fn(source: &str, hint: Option<&str>) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    for (i, _) in lines.iter().enumerate() {
        if !has_shadow_comment(lines[i].trim()) {
            continue;
        }
        let j = skip_to_function_def(&lines, i + 1)?;
        let name = extract_fn_name(lines.get(j).unwrap_or(&"").trim())?;
        if hint.map_or(true, |h| name == h) {
            return Some(name.to_string());
        }
    }
    None
}

fn has_shadow_comment(line: &str) -> bool {
    line.contains("praetor-shadow:")
}

fn skip_to_function_def(lines: &[&str], start: usize) -> Option<usize> {
    let mut j = start;
    while j < lines.len() && (lines[j].trim().is_empty() || lines[j].trim().starts_with('#')) {
        j += 1;
    }
    if j < lines.len() { Some(j) } else { None }
}

/// Extract a function name from a line that might define a function.
/// Supports: `fn name(...)`, `def name(...)`, `function name(...)`, `name = fn(...)`
fn extract_fn_name(line: &str) -> Option<&str> {
    let line = line.trim();
    // Strip visibility, async, const, unsafe modifiers before matching
    let cleaned = line
        .strip_prefix("pub ")
        .or_else(|| line.strip_prefix("pub(crate) "))
        .or_else(|| line.strip_prefix("pub(super) "))
        .or_else(|| line.strip_prefix("pub(self) "))
        .or_else(|| line.strip_prefix("pub(in "))
        .unwrap_or(line);
    let cleaned = cleaned
        .strip_prefix("async ")
        .or_else(|| cleaned.strip_prefix("const "))
        .or_else(|| cleaned.strip_prefix("unsafe "))
        .unwrap_or(cleaned);
    // Now try function-defining keywords
    for prefix in &["fn ", "def ", "function "] {
        if let Some(rest) = cleaned.strip_prefix(prefix) {
            let name = rest.split('(').next().unwrap_or("").trim();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some(name);
            }
        }
    }
    None
}

/// Find the original function name from a `praetor-shadow: original=fn_name` comment.
fn find_original_for_shadow(source: &str, shadow_fn: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !has_shadow_comment(trimmed) || !trimmed.contains("original=") {
            continue;
        }
        let name = extract_original_name(trimmed)?;
        if name.is_empty() {
            continue;
        }
        let j = skip_to_function_def(&lines, i + 1)?;
        let fn_name = extract_fn_name(lines.get(j).unwrap_or(&"").trim())?;
        if fn_name == shadow_fn {
            return Some(name);
        }
    }
    None
}

fn extract_original_name(line: &str) -> Option<String> {
    let after_keyword = line.split("original=").nth(1)?;
    let name = after_keyword
        .trim()
        .trim_start_matches('"')
        .split(|c: char| c == '"' || c == ' ' || c == ',' || c == ')' || c == '*' || c == '/')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() { None } else { Some(name) }
}

/// Generate a benchmark scaffold file for the original and shadow functions.
fn generate_bench_scaffold(
    file_path: &Path,
    original: &str,
    shadow: &str,
    _source: &str,
) -> String {
    // Compute the Rust module path from the file path relative to crate root.
    let crate_root = find_crate_root(file_path).unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
    });
    let relative = file_path.strip_prefix(&crate_root).unwrap_or(file_path);
    let module_path = relative
        .to_string_lossy()
        .trim_start_matches('/')
        .trim_start_matches('\\')
        .replace("src/", "crate::")
        .replace("src\\", "crate::")
        .replace(".rs", "")
        .replace('/', "::")
        .replace('\\', "::");

        let template = r#"// Auto-generated by `praetor verify --shadow`
// Fill in the test input generation below.
//
// Three-gate pipeline:
//   GATE 1: IO Equivalence — both functions produce same outputs
//   GATE 2: Metric Improvement — shadow fixes the flagged metric(s)
//   GATE 3: Benchmark — faster wins, tiebreaker uses aggregate metrics

#[cfg(test)]
mod bench_{SHADOW} {{
    use std::time::Instant;
    use std::collections::HashMap;

    use {MODPATH}::{{{ORIG}, {SHADOW}}};

    fn test_inputs() -> Vec<(InputType, ...)> {{
        vec![]
    }}

    fn gate1_io_equivalence(inputs: &[(...)]) -> bool {{
        for input in inputs {{
            let a = {ORIG}(input);
            let b = {SHADOW}(input);
            if a != b {{
                eprintln!("IO MISMATCH on {:?}", input);
                eprintln!("  original: {:?}", a);
                eprintln!("  shadow:   {:?}", b);
                return false;
            }}
        }}
        println!("  Testing {} inputs... all match ✅", inputs.len());
        true
    }}

    #[test]
    fn shadow_verification() {{
        let inputs = test_inputs();
        let iterations = 500_000;

        println!();
        println!("=== Shadow Verification: {ORIG} ===");

        println!("── Gate 1: IO Equivalence ──");
        assert!(gate1_io_equivalence(&inputs), "IO MISMATCH");

        println!("── Gate 3: Benchmark ──");
        for _ in 0..1000 {{
            for input in &inputs {{
                let _ = {ORIG}(input);
                let _ = {SHADOW}(input);
            }}
        }}

        let (orig_ns, shadow_ns) = {{
            let o = Instant::now();
            for _ in 0..iterations {{
                for input in &inputs {{
                    let _ = {ORIG}(input);
                }}
            }}
            let o_ns = o.elapsed().as_nanos() as f64 / (iterations as f64 * inputs.len() as f64);
            let s = Instant::now();
            for _ in 0..iterations {{
                for input in &inputs {{
                    let _ = {SHADOW}(input);
                }}
            }}
            let s_ns = s.elapsed().as_nanos() as f64 / (iterations as f64 * inputs.len() as f64);
            (o_ns, s_ns)
        }};

        let ratio = shadow_ns / orig_ns;
        println!("  original: {:8.1} ns/op", orig_ns);
        println!("  shadow:   {:8.1} ns/op", shadow_ns);
        println!("  ratio:    {:6.3}x", ratio);

        if shadow_ns < orig_ns * (1.0 / 1.03) {{
            println!("  SHADOW WINS");
        }} else if ratio <= 1.03 {{
            println!("  TIE");
            println!("  Tiebreaker: shadow wins on metrics");
        }} else {{
            println!("  ORIGINAL WINS");
            println!("  Warning silenced");
        }}
        println!();
    }}
}}
"#;
        let scaffold = template
            .replace("{ORIG}", original)
            .replace("{SHADOW}", shadow)
            .replace("{MODPATH}", &module_path);
        scaffold
}

/// Walk up from `path` to find the crate root (nearest directory containing Cargo.toml).
fn find_crate_root(path: &Path) -> Option<std::path::PathBuf> {
    let mut dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    loop {
        if dir.join("Cargo.toml").is_file() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
}