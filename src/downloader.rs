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
            url_template: "https://github.com/returntocorp/semgrep/releases/latest/download/semgrep-v{version}-{os}-{arch}.tgz".into(),
            version: "1.120.2".into(),
            archive_type: Some("tgz".into()),
            extract_path: Some("semgrep".into()),
            checksum: None,
        },
        ToolAsset {
            name: "sonarlint-language-server".into(),
            binary_name: None,
            url_template: "https://repo1.maven.org/maven2/org/sonarsource/sonarlint/core/sonarlint-language-server/{version}/sonarlint-language-server-{version}.jar".into(),
            version: "10.17.0.59868".into(),
            archive_type: None,
            extract_path: None,
            checksum: None,
        },
        ToolAsset {
            name: "infer".into(),
            binary_name: Some("infer".into()),
            url_template: "https://github.com/facebook/infer/releases/download/v{version}/infer-{os}-{arch}.tar.xz".into(),
            version: "1.2.0".into(),
            archive_type: Some("tar.xz".into()),
            extract_path: Some("infer/bin/infer".into()),
            checksum: None,
        },
    ]
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
    if tool.archive_type.is_some() {
        // For archived tools, check for the extracted binary
        let bin_path = tool.bin_path(cache);
        if bin_path.exists() {
            return true;
        }
        // Also check lib/ for JARs
        let lib_path = cache.join("lib").join(format!("{}.jar", tool.name));
        if lib_path.exists() {
            return true;
        }
        false
    } else {
        // Non-archived (plain file download) — check bin/ or lib/
        if tool.name.ends_with(".jar") || tool.name.contains("sonarlint") {
            cache.join("lib").join(format!("{}.jar", tool.name)).exists()
        } else {
            tool.bin_path(cache).exists()
        }
    }
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
            let tmp = cache.join("tmp").join(&tool.name);
            fs::create_dir_all(&tmp)
                .map_err(|e| format!("failed to create temp dir: {}", e))?;

            let status = Command::new("tar")
                .args(["-xzf"])
                .arg(archive_path)
                .arg("-C")
                .arg(&tmp)
                .status()
                .map_err(|e| format!("failed to invoke tar: {}", e))?;

            if !status.success() {
                return Err(format!("tar extraction failed for {}", tool.name));
            }

            // Move the binary to bin/
            let extract_name = tool.extract_path.as_deref().unwrap_or(&tool.name);
            let extracted = tmp.join(extract_name);
            let bin_name = tool.binary_name.as_deref().unwrap_or(&tool.name);
            let bin_dest = cache.join("bin").join(bin_name);

            if extracted.is_file() {
                fs::rename(&extracted, &bin_dest)
                    .or_else(|_| fs::copy(&extracted, &bin_dest).map(|_| ()))
                    .map_err(|e| format!("failed to install binary: {}", e))?;
            } else if extracted.is_dir() {
                // Try to find the binary inside the extracted directory
                let binary = extracted.join(bin_name);
                if binary.exists() {
                    fs::rename(&binary, &bin_dest)
                        .or_else(|_| fs::copy(&binary, &bin_dest).map(|_| ()))
                        .map_err(|e| format!("failed to install binary from dir: {}", e))?;
                } else {
                    warn!("extracted path {} not found for {}", extracted.display(), tool.name);
                }
            }

            // Set executable permission (non-Windows)
            if os != "windows" {
                let _ = Command::new("chmod")
                    .args(["+x"])
                    .arg(&bin_dest)
                    .status();
            }

            // Cleanup temp
            let _ = fs::remove_dir_all(&tmp);

            info!("installed {} to {}", tool.name, bin_dest.display());
        }

        Some("tar.xz") => {
            let tmp = cache.join("tmp").join(&tool.name);
            fs::create_dir_all(&tmp)
                .map_err(|e| format!("failed to create temp dir: {}", e))?;

            // Use tar with J flag for xz
            let status = Command::new("tar")
                .args(["-xJf"])
                .arg(archive_path)
                .arg("-C")
                .arg(&tmp)
                .status()
                .map_err(|e| format!("failed to invoke tar (xz): {}", e))?;

            if !status.success() {
                return Err(format!("tar.xz extraction failed for {}", tool.name));
            }

            let extract_name = tool.extract_path.as_deref().unwrap_or(&tool.name);
            let extracted = tmp.join(extract_name);
            let bin_name = tool.binary_name.as_deref().unwrap_or(&tool.name);
            let bin_dest = cache.join("bin").join(bin_name);

            // Walk the extracted tree to find the binary
            if let Ok(mut entries) = fs::read_dir(&extracted) {
                // Infer archive extracts with a versioned parent dir
                // e.g. infer-1.2.0-linux-x86_64/infer/bin/infer
                if let Some(entry) = entries.next().and_then(|r| r.ok()) {
                    let unwrapped = entry.path();
                    let binary = unwrapped.join(bin_name);
                    if binary.exists() {
                        fs::rename(&binary, &bin_dest)
                            .or_else(|_| fs::copy(&binary, &bin_dest).map(|_| ()))
                            .map_err(|e| format!("failed to install binary: {}", e))?;
                    }
                }
            }

            if os != "windows" {
                let _ = Command::new("chmod").args(["+x"]).arg(&bin_dest).status();
            }

            let _ = fs::remove_dir_all(&tmp);
            info!("installed {} to {}", tool.name, bin_dest.display());
        }

        _ => {
            // Plain file (e.g., JAR) — copy to lib/
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
        } else {
            warn!("{} is NOT ready — continuing without it", tool.name);
        }
    }

    ready
}
