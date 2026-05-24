use tracing::info;

/// Run `praetor init` — set up .praetor/ directory, config, and pre-commit hook.
pub fn run_init(force: bool) {
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let praetor_dir = root.join(".praetor");

    // 1. Create .praetor/ directory
    if !praetor_dir.is_dir() {
        std::fs::create_dir_all(&praetor_dir).expect("failed to create .praetor/ directory");
        info!("Created {}", praetor_dir.display());
    } else {
        info!("Already exists: {}", praetor_dir.display());
    }

    // 2. Create .praetor/shadow-results.json if missing
    let registry_path = praetor_dir.join("shadow-results.json");
    if !registry_path.is_file() {
        std::fs::write(&registry_path, "{}\n").expect("failed to write shadow-results.json");
        info!("Created {}", registry_path.display());
    } else {
        info!("Already exists: {}", registry_path.display());
    }

    // 3. Create .praetor.toml if missing
    let config_path = root.join(".praetor.toml");
    if !config_path.is_file() {
        let default_config = r#"[intent]
enabled = true
severity = "error"
exempt_patterns = ["fn get_.*", "fn set_.*", "fn new\\(", "fn main\\(", "fn test_.*"]

[complexity]
big_o_threshold = "O(n²)"
cyclomatic_max = 15
cognitive_max = 15
max_function_lines = 100
max_nesting_depth = 6
max_params = 6

[state_graph]
enabled = false
path = ".praetor/state-graph.json"

[datalog]
auth_functions = ["authenticate", "authorize", "login"]
private_data_labels = ["private", "secret", "password", "token"]
entry_points = ["main", "run", "start", "handle"]
log_functions = ["log", "log_access", "audit"]
"#;
        std::fs::write(&config_path, default_config).expect("failed to write .praetor.toml");
        info!("Created {}", config_path.display());
    } else {
        info!("Already exists: {}", config_path.display());
    }

    // 4. Install pre-commit hook
    let git_dir = root.join(".git");
    let hooks_dir = git_dir.join("hooks");
    let hook_path = hooks_dir.join("pre-commit");

    if hooks_dir.is_dir() {
        if hook_path.is_file() && !force {
            info!("Pre-commit hook already exists at {}", hook_path.display());
            info!("Use --force to overwrite");
        } else {
            let hook_script = include_str!("../scripts/pre-commit.sh");
            std::fs::write(&hook_path, hook_script).expect("failed to write pre-commit hook");
            let _ = std::process::Command::new("chmod")
                .args(["+x", &hook_path.to_string_lossy()])
                .status();
            info!("Installed pre-commit hook at {}", hook_path.display());
        }
    } else {
        info!("No .git directory found — skipping pre-commit hook installation");
    }

    info!("");
    info!("Praetor initialized successfully in {}", root.display());
    info!("Next steps:");
    info!("  1. Review .praetor.toml and adjust thresholds for your project");
    info!("  2. Run: praetor report --target .   (baseline report)");
    info!("  3. Run: praetor validate --warn     (verify gate passes)");
}

/// Find the project root by looking for version control or build files.
fn find_project_root() -> Option<std::path::PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut dir = Some(cwd.as_path());
    while let Some(d) = dir {
        for marker in &[".git", "Cargo.toml", "package.json", "pyproject.toml", "go.mod"] {
            if d.join(marker).is_file() || (marker == &".git" && d.join(".git").is_dir()) {
                return Some(d.to_path_buf());
            }
        }
        dir = d.parent();
    }
    None
}
