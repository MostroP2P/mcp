//! `mostro-mcp`: serves the Mostro tools over MCP on stdio.
//!
//! stdout carries the protocol, so every log line goes to stderr.

use anyhow::Context;
use mostro_mcp::{
    cli_backend::{CliRunner, MostroCliBackend},
    config::Config,
    contract,
    diagnostics::{self, LogSettings},
    outcome::Outcome,
    server::MostroServer,
};
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log = LogSettings::from_env().context("invalid mostro-mcp configuration")?;
    diagnostics::init(&log).context("invalid mostro-mcp configuration")?;
    if let Some(interval) = log.heartbeat {
        tokio::spawn(diagnostics::heartbeat(interval));
    }

    // Installed first, so a signal at any point is logged: a client that kills
    // the server otherwise leaves no trace (SIGKILL cannot be caught).
    let stopped = stop_signal()?;
    tokio::select! {
        result = serve() => {
            // Logged too: with MOSTRO_MCP_LOG_FILE the file must say why the
            // server stopped, even when the host discards stderr.
            if let Err(error) = &result {
                tracing::error!("mostro-mcp stopped: {error:#}");
            }
            result
        }
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
        node_name = mostro_mcp::nodes::by_pubkey(&config.mostro_pubkey).map_or("outside the trusted catalog", |node| node.name),
        node_source = ?config.node_source,
        relays = ?config.relays,
        relays_source = ?config.relays_source,
        writes = config.allow_writes,
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        ppid = parent_pid(),
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

/// The parent's process id, which only Unix exposes without extra crates.
#[cfg(unix)]
fn parent_pid() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

#[cfg(not(unix))]
fn parent_pid() -> Option<u32> {
    None
}

/// Resolves to the name and number of the first termination signal received.
#[cfg(unix)]
fn stop_signal() -> anyhow::Result<impl Future<Output = (&'static str, i32)>> {
    use tokio::signal::unix::{SignalKind, signal};
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

/// Windows has no Unix signals: Ctrl+C (or Ctrl+Break) is reported as SIGINT.
#[cfg(windows)]
fn stop_signal() -> anyhow::Result<impl Future<Output = (&'static str, i32)>> {
    use tokio::signal::windows::{ctrl_break, ctrl_c};
    let mut int = ctrl_c().context("listen for Ctrl+C")?;
    let mut brk = ctrl_break().context("listen for Ctrl+Break")?;
    Ok(async move {
        tokio::select! {
            _ = int.recv() => ("SIGINT", 2),
            _ = brk.recv() => ("SIGINT", 2),
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
