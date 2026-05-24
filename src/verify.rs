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
                    eprintln!("no shadow function found in {}. specify --shadow or add #[praetor::shadow]", file);
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
                    eprintln!("no shadow functions found in {}. annotate with #[praetor::shadow]", file);
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
    info!("  2. Run: cargo bench --features praetor_bench");
    info!("  3. Compare results manually, or:");
    info!("     praetor verify --shadow {} --run", file);
}

/// Find a function annotated with #[praetor::shadow] in the source.
fn find_shadow_fn(source: &str, hint: Option<&str>) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.contains("#[praetor::shadow") || line.contains("#[shadow") ||
           line.contains("#[crate::shadow") ||
           line.contains("// praetor:shadow") || line.contains("/* praetor:shadow")
        {
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < lines.len() {
                let fn_line = lines[j].trim();
                // Extract function name: fn name(...)
                if let Some(name) = fn_line.strip_prefix("fn ") {
                    let name = name.split('(').next().unwrap_or("").trim();
                    if !name.is_empty() {
                        if hint.map_or(true, |h| name == h) {
                            return Some(name.to_string());
                        }
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// Find the original function name from a #[praetor::shadow(original = "...")] attribute.
fn find_original_for_shadow(source: &str, shadow_fn: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if (line.contains("#[praetor::shadow") || line.contains("#[shadow") ||
            line.contains("#[crate::shadow")) &&
           line.contains("original =")
        {
            if let Some(eq) = line.find("original =") {
                let after_eq = &line[eq + 10..];
                let name = after_eq.trim().trim_start_matches('"').split('"').next().unwrap_or("").trim();
                if !name.is_empty() {
                    let mut j = i + 1;
                    while j < lines.len() && lines[j].trim().is_empty() {
                        j += 1;
                    }
                    if j < lines.len() {
                        let fn_line = lines[j].trim();
                        if let Some(fn_name) = fn_line.strip_prefix("fn ") {
                            let fn_name = fn_name.split('(').next().unwrap_or("").trim();
                            if fn_name == shadow_fn {
                                return Some(name.to_string());
                            }
                        }
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
// Fill in the test input generation below, then run:
//   cargo bench --features praetor_bench

#[cfg(feature = "praetor_bench")]
mod __praetor_bench_{shadow} {{
    use super::*;
    use divan::Bencher;

    /// Benchmark the ORIGINAL function `{original}`.
    /// TODO: generate realistic test inputs for `{original}`.
    #[divan::bench(name = "{original} (original)")]
    fn bench_original(bencher: Bencher) {{
        // Example (replace with actual input generation):
        //   let input = generate_test_input();
        //   bencher.bench_local(|| {module_path}::{original}(input));
        let _ = bencher;
        todo!("generate test inputs for {original}")
    }}

    /// Benchmark the SHADOW function `{shadow}`.
    /// TODO: generate realistic test inputs for `{shadow}`.
    #[divan::bench(name = "{shadow} (shadow)")]
    fn bench_shadow(bencher: Bencher) {{
        // Example:
        //   let input = generate_test_input();
        //   bencher.bench_local(|| {module_path}::{shadow}(input));
        let _ = bencher;
        todo!("generate test inputs for {shadow}")
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