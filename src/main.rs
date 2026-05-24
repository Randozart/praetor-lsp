use std::sync::Arc;

use tower_lsp::LspService;
use tracing_subscriber::EnvFilter;

mod ast;
mod checks;
mod config;
mod lsp;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

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

    tracing::info!("praetor-lsp starting on stdio");
    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}
