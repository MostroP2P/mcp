//! Server configuration, read once from the environment at startup.
//!
//! Only public or local values live here: the node's pubkey, relay URLs, the
//! CLI's path and data directory. Keys and the mnemonic stay in mostro-cli's
//! own database; this server never reads them.

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{Error, Result};

pub const DEFAULT_CLI: &str = "mostro-cli";
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `MOSTRO_CLI_PATH`; `mostro-cli` from `$PATH` when unset.
    pub cli_path: PathBuf,
    /// `MOSTRO_PUBKEY`: the node to trade on, hex or npub.
    pub mostro_pubkey: String,
    /// `RELAYS`: comma-separated `ws://` / `wss://` URLs.
    pub relays: Vec<String>,
    /// `POW`: proof-of-work bits to put on outgoing events.
    pub pow: Option<u32>,
    /// `TRANSPORT`: passed through to the CLI, which validates it.
    pub transport: Option<String>,
    /// `MCLI_DIR`: the CLI's data directory (identity + trade keys).
    pub mcli_dir: Option<PathBuf>,
    /// `MOSTRO_MCP_ALLOW_WRITES`: off unless explicitly `true`.
    pub allow_writes: bool,
    /// `MOSTRO_MCP_COMMAND_TIMEOUT_SECS`.
    pub timeout: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Builds the configuration from any key → value source, so it is tested
    /// without touching the process environment.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let get = |key: &str| {
            lookup(key)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };

        let mostro_pubkey = get("MOSTRO_PUBKEY").ok_or_else(|| {
            config_error("MOSTRO_PUBKEY is required: the Mostro node's public key (hex or npub)")
        })?;
        if !is_public_key(&mostro_pubkey) {
            return Err(config_error(format!(
                "MOSTRO_PUBKEY `{mostro_pubkey}` is not a hex or npub public key"
            )));
        }

        let relays: Vec<String> = get("RELAYS")
            .ok_or_else(|| {
                config_error("RELAYS is required: comma-separated ws:// or wss:// URLs")
            })?
            .split(',')
            .map(str::trim)
            .filter(|relay| !relay.is_empty())
            .map(String::from)
            .collect();
        if let Some(bad) = relays
            .iter()
            .find(|relay| !relay.starts_with("ws://") && !relay.starts_with("wss://"))
        {
            return Err(config_error(format!(
                "relay `{bad}` is not a ws:// or wss:// URL"
            )));
        }
        if relays.is_empty() {
            return Err(config_error("RELAYS lists no relay"));
        }

        let pow = get("POW")
            .map(|value| {
                value.parse::<u32>().map_err(|_| {
                    config_error(format!("POW `{value}` is not a non-negative integer"))
                })
            })
            .transpose()?;

        let timeout_secs = match get("MOSTRO_MCP_COMMAND_TIMEOUT_SECS") {
            None => DEFAULT_TIMEOUT_SECS,
            Some(value) => match value.parse::<u64>() {
                Ok(secs) if secs > 0 => secs,
                _ => {
                    return Err(config_error(format!(
                        "MOSTRO_MCP_COMMAND_TIMEOUT_SECS `{value}` is not a positive number of seconds"
                    )));
                }
            },
        };

        let allow_writes = match get("MOSTRO_MCP_ALLOW_WRITES") {
            None => false,
            Some(value) => parse_switch("MOSTRO_MCP_ALLOW_WRITES", &value)?,
        };

        Ok(Self {
            cli_path: PathBuf::from(get("MOSTRO_CLI_PATH").unwrap_or_else(|| DEFAULT_CLI.into())),
            mostro_pubkey,
            relays,
            pow,
            transport: get("TRANSPORT"),
            mcli_dir: get("MCLI_DIR").map(PathBuf::from),
            allow_writes,
            timeout: Duration::from_secs(timeout_secs),
        })
    }
}

fn config_error(message: impl Into<String>) -> Error {
    Error::Config(message.into())
}

/// 64 hex characters, or a bech32 `npub1…` (63 characters).
fn is_public_key(key: &str) -> bool {
    let hex = key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit());
    let npub = key.len() == 63
        && key.starts_with("npub1")
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    hex || npub
}

/// A yes/no switch. Anything but a clear answer is an error, so a typo can
/// never turn writes on — or silently leave them off when they were wanted.
fn parse_switch(key: &str, value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(config_error(format!(
            "{key} `{value}` must be true or false"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const HEX: &str = "724497f34e6c4f55f9aa90f5e9dcc326c76a9f2ed817b0f0eb940ee7218ac8d5";

    fn load(pairs: &[(&str, &str)]) -> Result<Config> {
        let env: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|key| env.get(key).cloned())
    }

    fn minimal() -> Vec<(&'static str, &'static str)> {
        vec![("MOSTRO_PUBKEY", HEX), ("RELAYS", "ws://localhost:7000")]
    }

    #[test]
    fn defaults_are_read_only_with_a_sixty_second_timeout() {
        let config = load(&minimal()).unwrap();
        assert_eq!(config.cli_path, PathBuf::from(DEFAULT_CLI));
        assert_eq!(config.relays, ["ws://localhost:7000"]);
        assert!(!config.allow_writes);
        assert_eq!(config.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECS));
        assert_eq!(config.pow, None);
        assert_eq!(config.mcli_dir, None);
    }

    #[test]
    fn the_node_and_relays_are_required() {
        assert!(load(&[("RELAYS", "ws://localhost:7000")]).is_err());
        assert!(load(&[("MOSTRO_PUBKEY", HEX)]).is_err());
    }

    #[test]
    fn the_node_is_hex_or_npub() {
        let mut env = minimal();
        env[0] = ("MOSTRO_PUBKEY", "not-a-key");
        assert!(load(&env).is_err());
        env[0] = (
            "MOSTRO_PUBKEY",
            "npub1wfzf0u6wd384t7d2jr67nhxrymrk48ewmqtmpu8tjs8wwgv2er2skuq97p",
        );
        assert!(load(&env).is_ok());
    }

    #[test]
    fn relays_are_split_trimmed_and_must_be_websockets() {
        let mut env = minimal();
        env[1] = ("RELAYS", "wss://relay.mostro.network, ws://localhost:7000");
        assert_eq!(
            load(&env).unwrap().relays,
            ["wss://relay.mostro.network", "ws://localhost:7000"]
        );
        env[1] = ("RELAYS", "https://example.com");
        assert!(load(&env).is_err());
    }

    #[test]
    fn writes_need_an_explicit_true_and_anything_unclear_is_an_error() {
        for (value, expected) in [("true", true), ("1", true), ("false", false), ("0", false)] {
            let mut env = minimal();
            env.push(("MOSTRO_MCP_ALLOW_WRITES", value));
            assert_eq!(load(&env).unwrap().allow_writes, expected, "{value}");
        }
        let mut env = minimal();
        env.push(("MOSTRO_MCP_ALLOW_WRITES", "maybe"));
        assert!(load(&env).is_err());
    }

    #[test]
    fn optional_values_are_parsed() {
        let mut env = minimal();
        env.extend([
            ("MOSTRO_CLI_PATH", "/opt/mostro-cli"),
            ("POW", "8"),
            ("TRANSPORT", "nip44"),
            ("MCLI_DIR", "/tmp/alice"),
            ("MOSTRO_MCP_COMMAND_TIMEOUT_SECS", "15"),
        ]);
        let config = load(&env).unwrap();
        assert_eq!(config.cli_path, PathBuf::from("/opt/mostro-cli"));
        assert_eq!(config.pow, Some(8));
        assert_eq!(config.transport.as_deref(), Some("nip44"));
        assert_eq!(config.mcli_dir, Some(PathBuf::from("/tmp/alice")));
        assert_eq!(config.timeout, Duration::from_secs(15));
    }

    #[test]
    fn a_zero_or_garbled_number_is_an_error() {
        for (key, value) in [
            ("MOSTRO_MCP_COMMAND_TIMEOUT_SECS", "0"),
            ("MOSTRO_MCP_COMMAND_TIMEOUT_SECS", "soon"),
            ("POW", "-1"),
        ] {
            let mut env = minimal();
            env.push((key, value));
            assert!(load(&env).is_err(), "{key}={value}");
        }
    }
}
