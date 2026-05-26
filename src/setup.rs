use std::path::PathBuf;
use std::process::Command;

use tracing::{info, warn};

use crate::downloader;

/// Orchestrate the full Praetor setup: install Python deps, Java 17, external tools, and PATH symlinks.
pub fn run_setup() {
    info!("Starting Praetor setup...");

    let cache = downloader::cache_root();
    let _ = downloader::setup_cache(&cache);

    let bin_dir = SetupDirs::bin_dir();

    install_python_deps();
    install_java17(&cache, &bin_dir);
    let ready = downloader::ensure_all_tools(&cache);
    info!("{}/{} external tools ready", ready.len(), 4);
    symlink_tools(&cache, &bin_dir, &ready);
    install_slopguard();

    info!(
        r#"
╔══════════════════════════════════════════════════════════════╗
║  Praetor setup complete.                                     ║
║                                                              ║
║  Next steps:                                                 ║
║    1. Restart OpenCode for new LSP servers to connect       ║
║    2. Run: praetor report --target .   (baseline report)    ║
║    3. Run: praetor validate --warn     (verify gate)        ║
╚══════════════════════════════════════════════════════════════╝"#
    );
}

/// Paths used during setup.
struct SetupDirs;

impl SetupDirs {
    /// Return the standard user-local bin directory (~/.local/bin).
    fn bin_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(&home).join(".local").join("bin")
    }
}

/// Install Python pip packages required by the complexity and bridge LSP scripts.
fn install_python_deps() {
    info!("Installing Python dependencies...");
    let packages = [
        "pygls",
        "lsprotocol",
        "tree-sitter",
        "tree-sitter-python",
        "tree-sitter-javascript",
        "tree-sitter-typescript",
        "tree-sitter-go",
        "tree-sitter-c",
        "tree-sitter-cpp",
        "tree-sitter-java",
        "tree-sitter-rust",
        "tree-sitter-vhdl",
        "tree-sitter-systemverilog",
        "tree-sitter-ruby",
        "tree-sitter-lua",
        "tree-sitter-php",
        "tree-sitter-kotlin",
        "tree-sitter-swift",
        "tree-sitter-zig",
        "tree-sitter-dart",
        "tree-sitter-perl",
        "tree-sitter-haskell",
        "r2pipe",
    ];

    let pip_args: &[&str] = if cfg!(target_os = "linux") {
        &["--quiet", "install", "--user", "--break-system-packages"]
    } else {
        &["--quiet", "install", "--user"]
    };

    let status = Command::new("pip3")
        .args(pip_args)
        .args(&packages)
        .status();

    match status {
        Ok(s) if s.success() => info!("Python dependencies installed"),
        Ok(_) => warn!("pip3 install returned non-zero — some packages may be missing"),
        Err(e) => warn!(
            "failed to run pip3: {} — install manually: pip3 install --user {}",
            e,
            packages.join(" ")
        ),
    }
}

/// Download and install Adoptium Temurin JRE 17 into the Praetor cache, then symlink to PATH.
fn install_java17(cache: &PathBuf, bin_dir: &PathBuf) {
    let java_home = cache.join("lib").join("java17");

    if java_home.join("bin").join("java").exists() {
        info!("Java 17 already installed at {}", java_home.display());
        ensure_symlink(&java_home.join("bin").join("java"), bin_dir, "java17");
        return;
    }

    info!("Downloading Java 17 (Adoptium Temurin JRE)...");
    let url = "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.14%2B7/OpenJDK17U-jre_x64_linux_hotspot_17.0.14_7.tar.gz";

    let tmp_dir = cache.join("tmp");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let archive = tmp_dir.join("java17.tar.gz");

    let dl_status = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-o"])
        .arg(&archive)
        .arg(url)
        .status();

    match dl_status {
        Ok(s) if s.success() => {}
        _ => {
            warn!("Failed to download Java 17 — install manually from https://adoptium.net/");
            return;
        }
    }

    let extract_dir = tmp_dir.join("java17_extract");
    let _ = std::fs::create_dir_all(&extract_dir);

    let tar_status = Command::new("tar")
        .args([
            "-xzf",
            &archive.to_string_lossy(),
            "-C",
            &extract_dir.to_string_lossy(),
        ])
        .status();

    if tar_status.is_err() || !tar_status.unwrap().success() {
        warn!("Failed to extract Java 17 archive");
        let _ = std::fs::remove_file(&archive);
        let _ = std::fs::remove_dir_all(&extract_dir);
        return;
    }

    let jdk_dir = match std::fs::read_dir(&extract_dir) {
        Ok(mut entries) => entries.find_map(|e| e.ok()).map(|e| e.path()),
        Err(_) => None,
    };

    if let Some(jdk) = jdk_dir {
        let _ = std::fs::rename(&jdk, &java_home);
        let _ = std::fs::remove_file(&archive);
        let _ = std::fs::remove_dir_all(&extract_dir);

        if java_home.join("bin").join("java").exists() {
            info!("Java 17 installed to {}", java_home.display());
            ensure_symlink(&java_home.join("bin").join("java"), bin_dir, "java17");
        }
    } else {
        warn!("Java 17 archive extracted but JDK directory not found");
    }
}

/// Create symlinks in ~/.local/bin/ for all ready tools downloaded by the cache.
fn symlink_tools(cache: &PathBuf, bin_dir: &PathBuf, ready: &[downloader::ToolAsset]) {
    for tool in ready {
        if let Some(bin_name) = &tool.binary_name {
            let src = tool.bin_path(cache);
            if src.exists() {
                ensure_symlink(&src, bin_dir, bin_name);
            }
        }
    }
}

/// Create a symlink at `bin_dir/name` pointing to `src`. Removes any existing file at the link path.
fn ensure_symlink(src: &PathBuf, bin_dir: &PathBuf, name: &str) {
    let link = bin_dir.join(name);
    let _ = std::fs::remove_file(&link);
    if let Err(e) = std::os::unix::fs::symlink(src, &link) {
        warn!(
            "Failed to create symlink {} -> {}: {}",
            link.display(),
            src.display(),
            e
        );
    } else {
        info!("Symlinked {} -> {}", name, src.display());
    }
}

/// Install slopguard via npm (AI-generated code pattern detector for JS/TS).
/// v0.0.1 is a stub (no executable code), but we install it in the project for future use.
fn install_slopguard() {
    info!("Installing slopguard via npm (local)...");
    let status = Command::new("npm")
        .args(["install", "--save-dev", "slopguard"])
        .current_dir(crate::downloader::cache_root().parent().unwrap_or(&std::env::current_dir().unwrap_or_default()))
        .status();
    match status {
        Ok(s) if s.success() => info!("slopguard installed (v0.0.1 stub — waiting for real release)"),
        Ok(_) => warn!("npm install slopguard returned non-zero"),
        Err(e) => warn!("failed to run npm: {} — install manually: npm install slopguard", e),
    }
}
