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
mod instruct;
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
    /// Print AI instructions for using Praetor
    Instruct,
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
        Some(Commands::Instruct) => {
            instruct::print_instruct();
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
    let mut patches = parse_nop_patches(nop);
    patches.extend(parse_jump_patches(jump));

    match binary::patch::apply_patches(&data, &patches, 0) {
        Ok(result) => {
            if let Err(e) = std::fs::write(output, &result) {
                eprintln!("[ERR] failed to write {}: {}", output, e);
            } else {
                println!("[OK] applied {} patches, wrote {} bytes to {}", patches.len(), result.len(), output);
            }
        }
        Err(e) => eprintln!("[ERR] patch application failed: {}", e),
    }
}

fn parse_nop_patches(nop: Option<&str>) -> Vec<binary::patch::Patch> {
    let mut patches = Vec::new();
    let nop_str = match nop {
        Some(s) => s,
        None => return patches,
    };
    for addr_str in nop_str.split(',') {
        let cleaned = addr_str.trim().trim_start_matches("0x");
        match u64::from_str_radix(cleaned, 16) {
            Ok(addr) => patches.push(binary::patch::Patch::nop(addr, 5)),
            Err(_) => eprintln!("[WARN] invalid NOP address: {}", cleaned),
        }
    }
    patches
}

fn parse_jump_patches(jump: Option<&str>) -> Vec<binary::patch::Patch> {
    let mut patches = Vec::new();
    let jump_str = match jump {
        Some(s) => s,
        None => return patches,
    };
    for pair in jump_str.split(',') {
        let parts: Vec<&str> = pair.trim().split(|c| c == ':' || c == '-' || c == ' ').collect();
        if parts.len() < 2 {
            continue;
        }
        let from_str = parts[0].trim().trim_start_matches("0x");
        let to_str = parts[1].trim().trim_start_matches("0x");
        let from = match u64::from_str_radix(from_str, 16) {
            Ok(a) => a,
            Err(_) => { eprintln!("[WARN] invalid jump 'from' address: {}", from_str); continue; }
        };
        let to = match u64::from_str_radix(to_str, 16) {
            Ok(a) => a,
            Err(_) => { eprintln!("[WARN] invalid jump 'to' address: {}", to_str); continue; }
        };
        match binary::patch::Patch::near_jump(from, to, true) {
            Ok(p) => patches.push(p),
            Err(e) => eprintln!("[WARN] jump patch error: {}", e),
        }
    }
    patches
}

/// Start the LSP server on stdio with the Praetor backend.
async fn run_lsp() {
    let cfg = config::PraetorConfig::discover();
    match &cfg {
        Some(c) => tracing::info!("using config from {:?}", c.path),
        None => tracing::info!("no .praetor.toml found, using defaults"),
    }

    let engine = Arc::new(ast::AstEngine::new());
    tracing::info!("loaded {} languages", engine.loaded_count());

    init_tool_cache();
    let bridges = build_bridges();
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(move |client| {
        lsp::Backend::new(client, engine.clone(), cfg.clone(), bridges)
    });

    tracing::info!("praetor starting on stdio");
    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}

fn init_tool_cache() {
    let cache_path = downloader::cache_root();
    if let Err(e) = downloader::setup_cache(&cache_path) {
        tracing::warn!("failed to setup tool cache: {}", e);
    }
    let cache_clone = cache_path.clone();
    tokio::spawn(async move {
        downloader::ensure_all_tools(&cache_clone);
    });
}

fn build_bridges() -> Vec<Box<dyn bridge::Bridge + Send + Sync>> {
    vec![
        Box::new(bridge::semgrep::SemgrepBridge),
        Box::new(bridge::infer::InferBridge),
        Box::new(bridge::sonarlint::SonarLintBridge),
    ]
}
