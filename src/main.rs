use std::sync::Arc;

use clap::{Parser, Subcommand};
use tower_lsp::LspService;
use tracing_subscriber::EnvFilter;

mod ast;
mod checks;
mod config;
mod facts;
mod lsp;
mod report;

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
            let rep = report::Report::new(engine, cfg);
            rep.generate(&target, &format, output.as_deref());
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

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(move |client| {
        lsp::Backend::new(client, engine.clone(), cfg.clone())
    });

    tracing::info!("praetor starting on stdio");
    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}
