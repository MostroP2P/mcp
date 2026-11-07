//! `mostro-mcp`: serves the Mostro tools over MCP on stdio.
//!
//! stdout carries the protocol, so every log line goes to stderr.

use anyhow::Context;
use mostro_mcp::{
    cli_backend::{CliRunner, MostroCliBackend},
    config::Config,
    contract,
    outcome::Outcome,
    server::MostroServer,
};
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

    let backend = MostroCliBackend::new(&config);
    check_contract(&backend, config.allow_writes).await?;
    let server = MostroServer::new(backend, config.allow_writes);
    let service = server
        .serve(stdio())
        .await
        .context("MCP handshake failed")?;
    service.waiting().await?;
    Ok(())
}

/// Stop here, with the missing commands named, if the installed `mostro-cli`
/// does not offer JSON output for every command the enabled tools need.
async fn check_contract(backend: &MostroCliBackend, allow_writes: bool) -> anyhow::Result<()> {
    let run = backend
        .run(vec!["schema".to_string()])
        .await
        .context("could not run `mostro-cli schema` to check the installed version")?;
    let schema = Outcome::parse(&run.stdout, run.code).map_err(|reason| {
        anyhow::anyhow!(
            "the installed mostro-cli does not answer `schema` with a JSON outcome ({reason}); \
             it predates the JSON interface this server needs: {}",
            run.stderr.trim()
        )
    })?;
    contract::verify(&schema, &contract::required(allow_writes))?;
    tracing::info!("mostro-cli offers the JSON commands this server needs");
    Ok(())
}
