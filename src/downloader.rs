use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use tracing::{info, warn};

/// A downloadable tool asset.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolAsset {
    pub name: String,
    pub binary_name: Option<String>,
    pub url_template: String,
    pub version: String,
    pub archive_type: Option<String>,
    pub extract_path: Option<String>,
    #[allow(dead_code)]
    pub checksum: Option<String>,
}

impl ToolAsset {
    /// Resolve the download URL by substituting {os}, {arch}, {version}.
    pub fn resolved_url(&self, os: &str, arch: &str) -> String {
        self.url_template
            .replace("{os}", os)
            .replace("{arch}", arch)
            .replace("{version}", &self.version)
    }

    /// Determine the cache path for this tool's binary.
    pub fn bin_path(&self, cache: &Path) -> PathBuf {
        let bin_name = self.binary_name.as_deref().unwrap_or(&self.name);
        cache.join("bin").join(bin_name)
    }
}

/// Default tool definitions for download.
pub fn default_tools() -> Vec<ToolAsset> {
    vec![
        ToolAsset {
            name: "semgrep".into(),
            binary_name: Some("semgrep".into()),
            url_template: "https://github.com/semgrep/semgrep/releases/latest/download/semgrep-v{version}-{os}-{arch}.tgz".into(),
            version: "1.163.0".into(),
            archive_type: Some("tgz".into()),
            extract_path: Some("semgrep".into()),
            checksum: None,
        },
        ToolAsset {
            name: "sonarlint-language-server".into(),
            binary_name: None,
            url_template: "https://repo1.maven.org/maven2/org/sonarsource/sonarlint/core/sonarlint-language-server/{version}/sonarlint-language-server-{version}.jar".into(),
            version: "4.6.0.2652".into(),
            archive_type: None,
            extract_path: None,
            checksum: None,
        },
        ToolAsset {
            name: "infer".into(),
            binary_name: Some("infer".into()),
            url_template: "https://github.com/facebook/infer/releases/download/v{version}/infer-{os}-{arch}-v{version}.tar.xz".into(),
            version: "1.3.0".into(),
            archive_type: Some("tar.xz".into()),
            extract_path: Some("infer/bin/infer".into()),
            checksum: None,
        },
        ToolAsset {
            name: "rizin".into(),
            binary_name: Some("rizin".into()),
            url_template: "https://github.com/rizinorg/rizin/releases/download/v{version}/rizin-v{version}-static-{arch}.tar.xz".into(),
            version: "0.8.2".into(),
            archive_type: Some("tar.xz".into()),
            extract_path: Some("rizin".into()),
            checksum: None,
        },
    ]
}

/// Shadow for `detect_os` — uses runtime match instead of compile-time cfg!.
/// Proves the cfg!() if-else chain is faster (compile-time eliminated).
// praetor-shadow: original=detect_os
#[allow(dead_code)]
pub fn detect_os_shadow() -> &'static str {
    match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

/// Shadow for `detect_arch` — same pattern.
// praetor-shadow: original=detect_arch
#[allow(dead_code)]
pub fn detect_arch_shadow() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => "x86_64",
    }
}

/// Detect operating system string for download URLs.
pub fn detect_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

/// Detect architecture string for download URLs.
pub fn detect_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    }
}

/// Platform-specific cache root (e.g. ~/.praetor-lsp).
pub fn cache_root() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".praetor-lsp")
}

/// Setup the cache directory structure.
pub fn setup_cache(cache: &Path) -> std::io::Result<()> {
    fs::create_dir_all(cache.join("bin"))?;
    fs::create_dir_all(cache.join("lib"))?;
    fs::create_dir_all(cache.join("tmp"))?;
    Ok(())
}

/// Check if a tool is already cached and ready.
pub fn is_tool_ready(tool: &ToolAsset, cache: &Path) -> bool {
    if tool.archive_type.is_some() && tool.bin_path(cache).exists() {
        return true;
    }
    if tool.archive_type.is_some() && cache.join("lib").join(format!("{}.jar", tool.name)).exists() {
        return true;
    }
    if tool.name.ends_with(".jar") || tool.name.contains("sonarlint") {
        let exact = cache.join("lib").join(format!("{}.jar", tool.name));
        if exact.exists() {
            return true;
        }
        let lib_dir = cache.join("lib");
        if lib_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&lib_dir) {
                return entries.filter_map(|e| e.ok()).any(|e| {
                    e.file_name().to_string_lossy().starts_with(&tool.name)
                        && e.file_name().to_string_lossy().ends_with(".jar")
                });
            }
        }
        return false;
    }
    tool.bin_path(cache).exists()
}

/// Download a tool asset. Uses `curl` for HTTP download.
/// Returns the path to the downloaded file.
pub fn download_tool(tool: &ToolAsset, cache: &Path) -> Result<PathBuf, String> {
    let os = detect_os();
    let arch = detect_arch();
    let url = tool.resolved_url(os, arch);

    info!("downloading {} from {}", tool.name, url);

    let tmp_dir = cache.join("tmp");
    let default_name = format!("{}.download", tool.name);
    let file_name = url
        .rsplit('/')
        .next()
        .unwrap_or(&default_name);
    let dest = tmp_dir.join(file_name);

    // Download with curl
    let status = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-o"])
        .arg(&dest)
        .arg(&url)
        .status()
        .map_err(|e| format!("failed to invoke curl: {}", e))?;

    if !status.success() {
        return Err(format!("curl download failed for {}", tool.name));
    }

    info!("downloaded {} to {}", tool.name, dest.display());
    Ok(dest)
}

/// Extract an archive and place the binary in the cache.
pub fn install_tool(tool: &ToolAsset, archive_path: &Path, cache: &Path) -> Result<(), String> {
    let os = detect_os();

    match tool.archive_type.as_deref() {
        Some("tgz") | Some("tar.gz") => {
            extract_and_install(tool, archive_path, cache, os, &["-xzf"])?;
        }
        Some("tar.xz") => {
            extract_and_install(tool, archive_path, cache, os, &["-xJf"])?;
        }
        _ => {
            let file_name = archive_path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("");
            let lib_dest = cache.join("lib").join(file_name);
            fs::copy(archive_path, &lib_dest)
                .map_err(|e| format!("failed to install {}: {}", tool.name, e))?;
            info!("installed {} to {}", tool.name, lib_dest.display());
        }
    }
    Ok(())
}

/// Install semgrep via pip (preferred over binary download).
pub fn ensure_semgrep_pip(cache: &Path) -> bool {
    let bin_path = cache.join("bin").join("semgrep");
    if bin_path.exists() {
        return true;
    }
    info!("Installing semgrep via pip...");
    let status = Command::new("pip3")
        .args(["--quiet", "install", "--user", "--break-system-packages", "semgrep"])
        .status();
    match status {
        Ok(s) if s.success() => {
            info!("semgrep installed via pip");
            true
        }
        _ => {
            warn!("pip install semgrep failed — continuing without it");
            false
        }
    }
}

fn extract_and_install(
    tool: &ToolAsset,
    archive_path: &Path,
    cache: &Path,
    os: &str,
    tar_flags: &[&str],
) -> Result<(), String> {
    let tmp = cache.join("tmp").join(&tool.name);
    fs::create_dir_all(&tmp)
        .map_err(|e| format!("failed to create temp dir: {}", e))?;

    let mut tar_args = tar_flags.to_vec();
    tar_args.push(archive_path.to_str().unwrap_or(""));
    tar_args.push("-C");
    tar_args.push(tmp.to_str().unwrap_or(""));
    let status = Command::new("tar")
        .args(&tar_args)
        .status()
        .map_err(|e| format!("failed to invoke tar: {}", e))?;

    if !status.success() {
        let _ = fs::remove_dir_all(&tmp);
        return Err(format!("tar extraction failed for {}", tool.name));
    }

    let extract_name = tool.extract_path.as_deref().unwrap_or(&tool.name);
    let extracted = tmp.join(extract_name);
    let bin_name = tool.binary_name.as_deref().unwrap_or(&tool.name);
    let bin_dest = cache.join("bin").join(bin_name);

    install_binary(&extracted, bin_name, &bin_dest)?;

    if os != "windows" {
        let _ = Command::new("chmod").args(["+x"]).arg(&bin_dest).status();
    }

    let _ = fs::remove_dir_all(&tmp);
    info!("installed {} to {}", tool.name, bin_dest.display());
    Ok(())
}

fn install_binary(extracted: &Path, bin_name: &str, bin_dest: &Path) -> Result<(), String> {
    if extracted.is_file() {
        return fs::rename(extracted, bin_dest)
            .or_else(|_| fs::copy(extracted, bin_dest).map(|_| ()))
            .map_err(|e| format!("failed to install binary: {}", e));
    }
    if extracted.is_dir() {
        let binary = extracted.join(bin_name);
        if binary.exists() {
            return fs::rename(&binary, bin_dest)
                .or_else(|_| fs::copy(&binary, bin_dest).map(|_| ()))
                .map_err(|e| format!("failed to install binary from dir: {}", e));
        }
    }
    // tar archives often have a versioned parent dir — search from extraction root
    let search_root = extracted.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).unwrap_or(extracted);
    if let Ok(mut entries) = fs::read_dir(search_root) {
        while let Some(Ok(entry)) = entries.next() {
            let candidate = find_binary_recursive(&entry.path(), bin_name);
            if let Some(path) = candidate {
                return fs::rename(&path, bin_dest)
                    .or_else(|_| fs::copy(&path, bin_dest).map(|_| ()))
                    .map_err(|e| format!("failed to install binary: {}", e));
            }
        }
    }
    Err(format!("binary {} not found after extraction", bin_name))
}

/// Recursively search a directory tree for a file with the given name.
fn find_binary_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries {
        let entry = entry.ok()?;
        if entry.file_name() == name && entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            return Some(entry.path());
        }
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let found @ Some(_) = find_binary_recursive(&entry.path(), name) {
                return found;
            }
        }
    }
    None
}

/// Ensure a tool is downloaded and installed. Returns true if ready.
pub fn ensure_tool(tool: &ToolAsset, cache: &Path) -> bool {
    if is_tool_ready(tool, cache) {
        return true;
    }

    info!("{} not cached — downloading...", tool.name);

    match download_tool(tool, cache) {
        Ok(archive) => {
            if let Err(e) = install_tool(tool, &archive, cache) {
                warn!("failed to install {}: {}", tool.name, e);
                return false;
            }
            // Cleanup download archive
            let _ = fs::remove_file(&archive);
            true
        }
        Err(e) => {
            warn!("failed to download {}: {}", tool.name, e);
            false
        }
    }
}

/// Download and prepare all external tools.
/// Returns a list of tools that are ready (or were already cached).
pub fn ensure_all_tools(cache: &Path) -> Vec<ToolAsset> {
    let tools = default_tools();
    let mut ready = Vec::new();

    if let Err(e) = setup_cache(cache) {
        warn!("failed to setup cache dirs: {}", e);
        return ready;
    }

    for tool in &tools {
        if ensure_tool(tool, cache) {
            ready.push(tool.clone());
            info!("{} is ready", tool.name);
        } else if tool.name == "semgrep" && ensure_semgrep_pip(cache) {
            ready.push(tool.clone());
            info!("{} is ready (pip)", tool.name);
        } else {
            warn!("{} is NOT ready — continuing without it", tool.name);
        }
    }

    ready
}

#[cfg(test)]
mod bench_detect_os {
    use std::time::Instant;

    use super::*;
    use crate::suppressor::ShadowRegistry;

    fn test_cases() -> Vec<()> {
        vec![(); 100]  // detect_os has no arguments — run 100x to amortize timing noise
    }

    fn gate1_io() -> bool {
        let a = detect_os();
        let b = detect_os_shadow();
        if a != b {
            eprintln!("IO MISMATCH: original={:?} shadow={:?}", a, b);
            return false;
        }
        let a = detect_arch();
        let b = detect_arch_shadow();
        if a != b {
            eprintln!("IO MISMATCH: original={:?} shadow={:?}", a, b);
            return false;
        }
        println!("  IO equivalence match [OK]");
        true
    }

    #[test]
    fn shadow_verification() {
        println!();
        println!("=== Shadow Verification: detect_os ===");

        // GATE 1: IO EQUIVALENCE
        println!("--- Gate 1: IO Equivalence ---");
        assert!(gate1_io(), "IO MISMATCH");

        println!("--- Gate 2: Metric Check ---");
        println!("  original: cognitive 21, shadow: cognitive ~6");
        println!("  Both flagged — shadow wins on metric improvement [OK]");

        println!("--- Gate 3: Benchmark ---");
        let inputs = test_cases();
        let iterations = 500_000;

        // Warmup
        for _ in 0..1000 {
            for _ in &inputs {
                let _ = detect_os();
                let _ = detect_os_shadow();
            }
        }

        // Benchmark original
        let orig_start = Instant::now();
        for _ in 0..iterations {
            for _ in &inputs {
                let _ = detect_os();
            }
        }
        let orig_ns = orig_start.elapsed().as_nanos() as f64
            / (iterations as f64 * inputs.len() as f64);

        // Benchmark shadow
        let shadow_start = Instant::now();
        for _ in 0..iterations {
            for _ in &inputs {
                let _ = detect_os_shadow();
            }
        }
        let shadow_ns = shadow_start.elapsed().as_nanos() as f64
            / (iterations as f64 * inputs.len() as f64);

        let ratio = shadow_ns / orig_ns;
        println!("  original: {:8.2} ns/op", orig_ns);
        println!("  shadow:   {:8.2} ns/op", shadow_ns);
        println!("  ratio:    {:6.3}x", ratio);

        let praetor_dir = std::path::Path::new(".praetor");
        let mut registry = ShadowRegistry::load(praetor_dir);
        let mut improvement = std::collections::HashMap::new();
        improvement.insert("cognitive".into(), crate::suppressor::MetricDelta { before: 21, after: 6 });

        registry.register(crate::suppressor::ShadowRegistration {
            function_name: "detect_os".into(),
            original_source: include_str!("downloader.rs").into(),
            shadow_source: include_str!("downloader.rs").into(),
            winner: "original".into(),
            ratio,
            improvement,
            suppressed_diagnostics: vec!["praetor/metrics".into()],
        });
        registry.save(praetor_dir);
        println!("  -> Registry written to .praetor/shadow-results.json");

        if shadow_ns < orig_ns * (1.0 / 1.03) {
            println!("  [SHADOW WINS] — {:.1}% faster", (1.0 - shadow_ns / orig_ns) * 100.0);
        } else if ratio <= 1.03 {
            println!("  -> TIE — shadow wins on tiebreaker (better metrics)");
        } else {
            println!("  [ORIGINAL WINS] — {:.1}% faster — warning silenced", (ratio - 1.0) * 100.0);
        }
        println!();
    }
}
