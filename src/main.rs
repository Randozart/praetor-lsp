use std::sync::Arc;

use clap::{Parser, Subcommand};
use tower_lsp::LspService;
use tracing_subscriber::EnvFilter;

mod ast;
mod bridge;
mod checks;
mod config;
mod downloader;
mod facts;
mod lsp;
mod report;
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
}

#[tokio::main]
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
        Some(Commands::Report { target, output, format }) => {
            let engine = Arc::new(ast::AstEngine::new());
            let cfg = config::PraetorConfig::discover();
            let cache = downloader::cache_root();
            let ready = downloader::ensure_all_tools(&cache);
            tracing::info!("{}/{} external tools ready", ready.len(), 3);
            let rep = report::Report::new(engine, cfg);
            rep.generate(&target, &format, output.as_deref());
        }
        Some(Commands::Verify { file, shadow, original, threshold, iterations }) => {
            verify::run_shadow_verify(&file, shadow.as_deref(), original.as_deref(), threshold, iterations);
        }
        _ => run_lsp().await,
    }
}

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
