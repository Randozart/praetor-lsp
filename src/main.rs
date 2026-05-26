use std::sync::Arc;

use clap::{Parser, Subcommand};
use tower_lsp::LspService;
use tracing_subscriber::EnvFilter;

mod ast;
mod binary;
mod bridge;
mod checks;
mod config;
mod downloader;
mod facts;
mod init;
mod lsp;
mod report;
mod setup;
mod suppressor;
mod validate;
mod verify;

#[derive(Parser)]
#[command(name = "praetor", version, about = "Quadruple-bookkeeping verification LSP")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the LSP server (default)
    Lsp,
    /// Generate a full project verification report
    Report {
        /// Target directory to analyze
        #[arg(long, default_value = ".")]
        target: String,
        /// Output file (stdout if omitted)
        #[arg(long)]
        output: Option<String>,
        /// Output format: html or markdown
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Analyze binary files (dll, exe, so, elf, o, bin, sys)
        #[arg(long)]
        binary: bool,
    },
    /// Run shadow verification benchmarks
    Verify {
        /// Source file containing original and shadow functions
        file: String,

        /// Name of the shadow function (auto-detected if omitted)
        #[arg(long)]
        shadow: Option<String>,

        /// Name of the original function (auto-detected if omitted)
        #[arg(long)]
        original: Option<String>,

        /// Performance regression threshold as percentage (default: 3)
        #[arg(long, default_value = "3")]
        threshold: f64,

        /// Minimum benchmark iterations (default: 10000)
        #[arg(long, default_value = "10000")]
        iterations: u64,
    },
    /// Install external dependencies (Java 17, Python packages, Semgrep, Infer, SonarLint)
    Setup,
    /// Initialize Praetor in the current project
    Init {
        /// Overwrite existing files without prompting
        #[arg(long)]
        force: bool,
    },
    /// Validate project — exit 1 if unproven diagnostics exist
    Validate {
        /// Target directory to analyze
        #[arg(long, default_value = ".")]
        target: String,
        /// Only ERROR-level diagnostics cause failure (allow WARNING)
        #[arg(long)]
        warn: bool,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
    /// Binary surgery and verification
    #[command(subcommand)]
    Binary(BinaryCommands),
}

#[derive(Subcommand)]
enum BinaryCommands {
    /// Compare original and patched binary CFGs
    Verify {
        /// Original binary file
        #[arg(long)]
        original: String,
        /// Patched binary file
        #[arg(long)]
        patched: String,
    },
    /// Apply patches to a binary
    Apply {
        /// Original binary file
        #[arg(long)]
        input: String,
        /// Output binary file
        #[arg(long)]
        output: String,
        /// Addresses to NOP (comma-separated hex)
        #[arg(long)]
        nop: Option<String>,
        /// Jump redirects (format: from,to; comma-separated pairs)
        #[arg(long)]
        jump: Option<String>,
    },
}

#[tokio::main]
/// Entry point: dispatch to subcommand or run the LSP server.
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Report { target, output, format, binary }) => {
            let engine = Arc::new(ast::AstEngine::new());
            let cfg = config::PraetorConfig::discover();
            let cache = downloader::cache_root();
            let ready = downloader::ensure_all_tools(&cache);
            tracing::info!("{}/{} external tools ready", ready.len(), 4);
            let rep = report::Report::new(engine, cfg);
            rep.generate(&target, &format, output.as_deref(), binary);
        }
        Some(Commands::Verify { file, shadow, original, threshold, iterations }) => {
            verify::run_shadow_verify(&file, shadow.as_deref(), original.as_deref(), threshold, iterations);
        }
        Some(Commands::Init { force }) => {
            init::run_init(force);
        }
        Some(Commands::Setup) => {
            setup::run_setup();
        }
        Some(Commands::Validate { target, warn, json }) => {
            validate::run_validate(&target, warn, json);
        }
        Some(Commands::Binary(BinaryCommands::Verify { original, patched })) => {
            run_binary_verify(&original, &patched);
        }
        Some(Commands::Binary(BinaryCommands::Apply { input, output, nop, jump })) => {
            run_binary_apply(&input, &output, nop.as_deref(), jump.as_deref());
        }
        _ => run_lsp().await,
    }
}

/// Verify that a patched binary preserves the original CFG topology.
fn run_binary_verify(original: &str, patched: &str) {
    let orig_path = std::path::Path::new(original);
    let patched_path = std::path::Path::new(patched);

    if !orig_path.exists() {
        eprintln!("[ERR] original binary not found: {}", original);
        return;
    }
    if !patched_path.exists() {
        eprintln!("[ERR] patched binary not found: {}", patched);
        return;
    }

    match binary::verify::compare_binaries(orig_path, patched_path) {
        Ok(report) => {
            println!("{}", binary::verify::format_topology_report(&report));
        }
        Err(e) => {
            eprintln!("[ERR] verification failed: {}", e);
        }
    }
}

/// Apply patches to a binary and write the result.
fn run_binary_apply(input: &str, output: &str, nop: Option<&str>, jump: Option<&str>) {
    let data = match std::fs::read(input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[ERR] failed to read {}: {}", input, e);
            return;
        }
    };

    let mut patches = Vec::new();

    // Parse NOP addresses
    if let Some(nop_str) = nop {
        for addr_str in nop_str.split(',') {
            let addr_str = addr_str.trim().trim_start_matches("0x");
            if let Ok(addr) = u64::from_str_radix(addr_str, 16) {
                patches.push(binary::patch::Patch::nop(addr, 5));
            } else {
                eprintln!("[WARN] invalid NOP address: {}", addr_str);
            }
        }
    }

    // Parse jump redirects (from,to)
    if let Some(jump_str) = jump {
        for pair in jump_str.split(',') {
            let parts: Vec<&str> = pair.trim().split(|c| c == ':' || c == '-' || c == ' ').collect();
            if parts.len() >= 2 {
                let from_str = parts[0].trim().trim_start_matches("0x");
                let to_str = parts[1].trim().trim_start_matches("0x");
                if let (Ok(from), Ok(to)) = (u64::from_str_radix(from_str, 16), u64::from_str_radix(to_str, 16)) {
                    match binary::patch::Patch::near_jump(from, to, true) {
                        Ok(p) => patches.push(p),
                        Err(e) => eprintln!("[WARN] jump patch error: {}", e),
                    }
                }
            }
        }
    }

    match binary::patch::apply_patches(&data, &patches, 0) {
        Ok(result) => {
            if let Err(e) = std::fs::write(output, &result) {
                eprintln!("[ERR] failed to write {}: {}", output, e);
            } else {
                println!("[OK] applied {} patches, wrote {} bytes to {}", patches.len(), result.len(), output);
            }
        }
        Err(e) => {
            eprintln!("[ERR] patch application failed: {}", e);
        }
    }
}

/// Start the LSP server on stdio with the Praetor backend.
async fn run_lsp() {
    let cfg = config::PraetorConfig::discover();
    if let Some(ref c) = cfg {
        tracing::info!("using config from {:?}", c.path);
    } else {
        tracing::info!("no .praetor.toml found, using defaults");
    }

    let engine = Arc::new(ast::AstEngine::new());
    tracing::info!("loaded {} languages", engine.loaded_count());

    // Start downloading external tools in the background
    let cache_path = downloader::cache_root();
    let _ = downloader::setup_cache(&cache_path);
    if let Err(e) = downloader::setup_cache(&cache_path) {
        tracing::warn!("failed to setup tool cache: {}", e);
    }
    let cache_clone = cache_path.clone();
    tokio::spawn(async move {
        downloader::ensure_all_tools(&cache_clone);
    });

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    // Build external tool bridges
    let bridges: Vec<Box<dyn bridge::Bridge + Send + Sync>> = vec![
        Box::new(bridge::semgrep::SemgrepBridge),
        Box::new(bridge::infer::InferBridge),
        Box::new(bridge::sonarlint::SonarLintBridge),
    ];

    let (service, socket) = LspService::new(move |client| {
        lsp::Backend::new(client, engine.clone(), cfg.clone(), bridges)
    });

    tracing::info!("praetor starting on stdio");
    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}
