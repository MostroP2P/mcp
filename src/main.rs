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
use tokio::signal::unix::{SignalKind, signal};
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

    // Installed first, so a signal at any point is logged: a client that kills
    // the server otherwise leaves no trace (SIGKILL cannot be caught).
    let stopped = stop_signal()?;
    tokio::select! {
        result = serve() => result,
        (name, number) = stopped => {
            tracing::warn!(signal = name, "stopped by a signal: {name}");
            std::process::exit(128 + number);
        }
    }
}

async fn serve() -> anyhow::Result<()> {
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
    tracing::info!("MCP session started");
    // `Closed`: the client closed stdin; `Cancelled`: the service stopped itself.
    let reason = service.waiting().await?;
    tracing::info!(?reason, "MCP session ended");
    Ok(())
}

/// Resolves to the name and number of the first termination signal received.
fn stop_signal() -> anyhow::Result<impl Future<Output = (&'static str, i32)>> {
    let mut term = signal(SignalKind::terminate()).context("listen for SIGTERM")?;
    let mut int = signal(SignalKind::interrupt()).context("listen for SIGINT")?;
    let mut hup = signal(SignalKind::hangup()).context("listen for SIGHUP")?;
    Ok(async move {
        tokio::select! {
            _ = term.recv() => ("SIGTERM", 15),
            _ = int.recv() => ("SIGINT", 2),
            _ = hup.recv() => ("SIGHUP", 1),
        }
    })
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
