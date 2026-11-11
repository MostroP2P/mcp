//! What the log records so a stopped server can be explained afterwards: an
//! optional log file of its own (a host may discard stderr), and an optional
//! heartbeat whose last line bounds when a process that left no other trace
//! (SIGKILL) was still alive.

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::error::{Error, Result};

/// The longest heartbeat interval accepted, in seconds (one hour).
pub const MAX_HEARTBEAT_SECS: u64 = 3600;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogSettings {
    /// `MOSTRO_MCP_LOG_FILE`: also append the log to this file.
    pub file: Option<PathBuf>,
    /// `MOSTRO_MCP_HEARTBEAT_SECS`: log a line this often while serving.
    pub heartbeat: Option<Duration>,
}

impl LogSettings {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let get = |key: &str| {
            lookup(key)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let heartbeat = get("MOSTRO_MCP_HEARTBEAT_SECS")
            .map(|value| match value.parse::<u64>() {
                Ok(secs) if (1..=MAX_HEARTBEAT_SECS).contains(&secs) => {
                    Ok(Duration::from_secs(secs))
                }
                _ => Err(Error::Config(format!(
                    "MOSTRO_MCP_HEARTBEAT_SECS `{value}` is not 1 to {MAX_HEARTBEAT_SECS} seconds"
                ))),
            })
            .transpose()?;
        Ok(Self {
            file: get("MOSTRO_MCP_LOG_FILE").map(PathBuf::from),
            heartbeat,
        })
    }
}

/// Sends the log to stderr and, when set, appends it to the log file, with
/// the filter of `MOSTRO_MCP_LOG` (default `info`). A panic is logged too, so
/// it reaches the file.
pub fn init(settings: &LogSettings) -> Result<()> {
    let filter =
        || EnvFilter::try_from_env("MOSTRO_MCP_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let file = settings
        .file
        .as_ref()
        .map(|path| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| {
                    Error::Config(format!(
                        "MOSTRO_MCP_LOG_FILE `{}` cannot be opened: {e}",
                        path.display()
                    ))
                })
        })
        .transpose()?;
    let stderr = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_filter(filter());
    let file = file.map(|file| {
        fmt::layer()
            .with_writer(Arc::new(file))
            .with_ansi(false)
            .with_filter(filter())
    });
    tracing_subscriber::registry()
        .with(stderr)
        .with(file)
        .init();
    // Logged, then handed to the previous hook: it still prints the panic and
    // the backtrace RUST_BACKTRACE asks for, whatever the log filter.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "panic");
        previous(info);
    }));
    Ok(())
}

/// Logs `alive` every `interval`, forever.
pub async fn heartbeat(interval: Duration) {
    let started = Instant::now();
    let mut ticks = tokio::time::interval(interval);
    ticks.tick().await;
    loop {
        ticks.tick().await;
        tracing::info!(uptime_secs = started.elapsed().as_secs(), "alive");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(pairs: &[(&str, &str)]) -> Result<LogSettings> {
        LogSettings::from_lookup(|key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        })
    }

    #[test]
    fn nothing_is_set_by_default() {
        assert_eq!(load(&[]).unwrap(), LogSettings::default());
        assert_eq!(
            load(&[
                ("MOSTRO_MCP_LOG_FILE", "  "),
                ("MOSTRO_MCP_HEARTBEAT_SECS", "")
            ])
            .unwrap(),
            LogSettings::default()
        );
    }

    #[test]
    fn a_file_and_a_heartbeat_are_read() {
        let settings = load(&[
            ("MOSTRO_MCP_LOG_FILE", " /tmp/mostro-mcp.log "),
            ("MOSTRO_MCP_HEARTBEAT_SECS", "30"),
        ])
        .unwrap();
        assert_eq!(settings.file, Some(PathBuf::from("/tmp/mostro-mcp.log")));
        assert_eq!(settings.heartbeat, Some(Duration::from_secs(30)));
    }

    #[test]
    fn a_heartbeat_outside_one_second_to_one_hour_is_an_error() {
        for bad in ["0", "3601", "-1", "often"] {
            let error = load(&[("MOSTRO_MCP_HEARTBEAT_SECS", bad)])
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("MOSTRO_MCP_HEARTBEAT_SECS"),
                "{bad}: {error}"
            );
        }
    }
}
