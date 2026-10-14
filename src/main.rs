//! `mostro-mcp`: serves the Mostro tools over MCP on stdio.
//!
//! stdout carries the protocol, so every log line goes to stderr.

use anyhow::Context;
use mostro_mcp::{cli_backend::MostroCliBackend, config::Config, server::MostroServer};
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_env("MOSTRO_MCP_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env().context("invalid mostro-mcp configuration")?;
    tracing::info!(
        cli = %config.cli_path.display(),
        node = %config.mostro_pubkey,
        relays = ?config.relays,
        writes = config.allow_writes,
        "starting mostro-mcp"
    );

    let server = MostroServer::new(MostroCliBackend::new(&config), config.allow_writes);
    let service = server
        .serve(stdio())
        .await
        .context("MCP handshake failed")?;
    service.waiting().await?;
    Ok(())
}
