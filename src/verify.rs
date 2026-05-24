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
///
/// Works across languages — the comment marker is the same:
/// - `// praetor-shadow: original=fn_name`
/// - `# praetor-shadow: original=fn_name`
/// - `/* praetor-shadow: original=fn_name */`
fn find_shadow_fn(source: &str, hint: Option<&str>) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.contains("praetor-shadow:") {
            let mut j = i + 1;
            while j < lines.len() && (lines[j].trim().is_empty() || lines[j].trim().starts_with('#')) {
                j += 1;
            }
            // Try Rust-like function syntax
            if let Some(name) = extract_fn_name(lines.get(j).unwrap_or(&"").trim()) {
                if hint.map_or(true, |h| name == h) {
                    return Some(name.to_string());
                }
            }
        }
        i += 1;
    }
    None
}

/// Extract a function name from a line that might define a function.
/// Supports: `fn name(...)`, `def name(...)`, `function name(...)`, `name = fn(...)`
fn extract_fn_name(line: &str) -> Option<&str> {
    let line = line.trim();
    // Rust/Go/C/C++/Java/JS/TS: fn name(..) or function name(..)
    for prefix in &["fn ", "def ", "function ", "pub fn ", "pub(crate) fn "] {
        if let Some(rest) = line.strip_prefix(prefix) {
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
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        // Match lines containing `praetor-shadow:` and `original=`
        if line.contains("praetor-shadow:") && line.contains("original=") {
            // Extract original = "name" or original=name
            let after_keyword = line.split("original=").nth(1)?;
            let name = after_keyword
                .trim()
                .trim_start_matches('"')
                .split(|c: char| c == '"' || c == ' ' || c == ',' || c == ')' || c == '*' || c == '/')
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                // Verify this comment precedes the shadow function
                let mut j = i + 1;
                while j < lines.len() && (lines[j].trim().is_empty() || lines[j].trim().starts_with('#')) {
                    j += 1;
                }
                if let Some(fn_name) = extract_fn_name(lines.get(j).unwrap_or(&"").trim()) {
                    if fn_name == shadow_fn {
                        return Some(name.to_string());
                    }
                }
            }
        }
        i += 1;
    }
    None
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

    format!(
        r#"// Auto-generated by `praetor verify --shadow`
// Fill in the test input generation below.
//
// To run this benchmark, add `divan` to your Cargo.toml dev-dependencies
// and add a `[[bench]]` or use `cargo test --bench`:
//
//   [dev-dependencies]
//   divan = "0.1"
//
// Then run:
//   cargo test --bench __praetor_bench_{shadow}

mod __praetor_bench_{shadow} {{
    use super::*;

    /// Benchmark the ORIGINAL function `{original}`.
    /// TODO: generate realistic test inputs for `{original}`.
    ///
    /// Example with divan:
    ///   use divan::Bencher;
    ///   #[divan::bench(name = "{original} (original)")]
    ///   fn bench_original(bencher: Bencher) {{
    ///       let input = generate_test_input();
    ///       bencher.bench_local(|| {module_path}::{original}(input));
    ///   }}
    pub fn _bench_original() {{
        let _ = ();
        unimplemented!("generate test inputs for {original}")
    }}

    /// Benchmark the SHADOW function `{shadow}`.
    /// TODO: generate realistic test inputs for `{shadow}`.
    pub fn _bench_shadow() {{
        let _ = ();
        unimplemented!("generate test inputs for {shadow}")
    }}
}}
"#,
        original = original,
        shadow = shadow,
        module_path = module_path,
    )
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