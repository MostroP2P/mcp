//! Server configuration, read once from the environment at startup.
//!
//! Only public or local values live here: the node's pubkey, relay URLs, the
//! CLI's path and data directory. Keys and the mnemonic stay in mostro-cli's
//! own database; this server never reads them.

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::nodes;

pub const DEFAULT_CLI: &str = "mostro-cli";
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// The shortest per-command timeout: mostro-cli waits up to 15 s for the
/// node's reply (its `--reply-timeout` default) after connecting and
/// publishing, and only then reports `uncertain`. A shorter limit would stop it
/// first and lose that report.
pub const MIN_TIMEOUT_SECS: u64 = 30;

/// Where a node setting came from, for the startup log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The trusted node catalog: Mostro Global, or a catalog node's relays.
    Default,
    /// The environment variable.
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `MOSTRO_CLI_PATH`; `mostro-cli` from `$PATH` when unset.
    pub cli_path: PathBuf,
    /// `MOSTRO_PUBKEY`: the node to trade on, hex or npub; Mostro Global
    /// when unset.
    pub mostro_pubkey: String,
    pub node_source: Source,
    /// `RELAYS`: comma-separated `ws://` / `wss://` URLs; the catalog relays
    /// of the node when unset.
    pub relays: Vec<String>,
    pub relays_source: Source,
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

        let (mostro_pubkey, node_source) = node_key(&lookup)?;
        let (relays, relays_source) = node_relays(&lookup, &mostro_pubkey)?;

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
                Ok(secs) if secs >= MIN_TIMEOUT_SECS => secs,
                Ok(_) => {
                    return Err(config_error(format!(
                        "MOSTRO_MCP_COMMAND_TIMEOUT_SECS `{value}` is under {MIN_TIMEOUT_SECS} s: \
                         mostro-cli waits up to 15 s for the node's reply and must be able to \
                         report it"
                    )));
                }
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
            node_source,
            relays,
            relays_source,
            pow,
            transport: get("TRANSPORT"),
            mcli_dir: get("MCLI_DIR").map(PathBuf::from),
            allow_writes,
            timeout: Duration::from_secs(timeout_secs),
        })
    }
}

/// `MOSTRO_PUBKEY`, or Mostro Global when it is unset. Set but blank is an
/// error: an operator who wrote the variable meant a node, and silently
/// trading on another would be worse than not starting.
fn node_key(lookup: &impl Fn(&str) -> Option<String>) -> Result<(String, Source)> {
    let Some(raw) = lookup("MOSTRO_PUBKEY") else {
        return Ok((nodes::default_node().pubkey.to_string(), Source::Default));
    };
    let key = raw.trim();
    if key.is_empty() {
        return Err(config_error(
            "MOSTRO_PUBKEY is set but empty: unset it to use Mostro Global, or give a \
             node's public key (hex or npub)",
        ));
    }
    if !is_public_key(key) {
        return Err(config_error(format!(
            "MOSTRO_PUBKEY `{key}` is not a hex or npub public key"
        )));
    }
    Ok((key.to_string(), Source::Environment))
}

/// `RELAYS`, or the catalog relays of `node` when it is unset. A node outside
/// the catalog has no known relays, so it needs the variable.
fn node_relays(
    lookup: &impl Fn(&str) -> Option<String>,
    node: &str,
) -> Result<(Vec<String>, Source)> {
    let Some(raw) = lookup("RELAYS") else {
        return match nodes::by_pubkey(node) {
            Some(trusted) => Ok((
                trusted
                    .relays
                    .iter()
                    .map(|relay| relay.to_string())
                    .collect(),
                Source::Default,
            )),
            None => Err(config_error(
                "RELAYS is required for a node outside the trusted catalog: \
                 comma-separated ws:// or wss:// URLs",
            )),
        };
    };
    let relays: Vec<String> = raw
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
        return Err(config_error(
            "RELAYS is set but lists no relay: unset it to use the node's catalog relays",
        ));
    }
    Ok((relays, Source::Environment))
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
    fn with_nothing_set_the_node_is_mostro_global_and_its_relays() {
        let config = load(&[]).unwrap();
        assert_eq!(
            config.mostro_pubkey,
            "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390"
        );
        assert_eq!(
            config.relays,
            [
                "wss://relay.mostro.network",
                "wss://mostro-p2p.tech",
                "wss://relay.shadowbip.com"
            ]
        );
        assert_eq!(config.node_source, Source::Default);
        assert_eq!(config.relays_source, Source::Default);
        assert!(!config.allow_writes);
    }

    #[test]
    fn the_relays_alone_can_be_overridden() {
        let config = load(&[("RELAYS", "ws://localhost:7000")]).unwrap();
        assert_eq!(config.mostro_pubkey, crate::nodes::default_node().pubkey);
        assert_eq!(config.relays, ["ws://localhost:7000"]);
        assert_eq!(config.node_source, Source::Default);
        assert_eq!(config.relays_source, Source::Environment);
    }

    #[test]
    fn a_catalog_node_alone_brings_its_own_relays() {
        let argentina = crate::nodes::by_id("argentina").unwrap();
        let config = load(&[("MOSTRO_PUBKEY", argentina.pubkey)]).unwrap();
        assert_eq!(config.relays, argentina.relays);
        assert_eq!(config.node_source, Source::Environment);
        assert_eq!(config.relays_source, Source::Default);
    }

    #[test]
    fn a_node_outside_the_catalog_needs_its_relays() {
        let error = load(&[("MOSTRO_PUBKEY", HEX)]).unwrap_err().to_string();
        assert!(error.contains("RELAYS"), "{error}");
    }

    #[test]
    fn an_explicitly_empty_node_or_relay_list_is_an_error_not_a_default() {
        for (key, value) in [
            ("MOSTRO_PUBKEY", ""),
            ("MOSTRO_PUBKEY", "   "),
            ("RELAYS", ""),
            ("RELAYS", " , "),
        ] {
            assert!(load(&[(key, value)]).is_err(), "{key}={value:?}");
        }
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
            ("MOSTRO_MCP_COMMAND_TIMEOUT_SECS", "45"),
        ]);
        let config = load(&env).unwrap();
        assert_eq!(config.cli_path, PathBuf::from("/opt/mostro-cli"));
        assert_eq!(config.pow, Some(8));
        assert_eq!(config.transport.as_deref(), Some("nip44"));
        assert_eq!(config.mcli_dir, Some(PathBuf::from("/tmp/alice")));
        assert_eq!(config.timeout, Duration::from_secs(45));
    }

    #[test]
    fn a_zero_or_garbled_number_is_an_error() {
        for (key, value) in [
            ("MOSTRO_MCP_COMMAND_TIMEOUT_SECS", "0"),
            ("MOSTRO_MCP_COMMAND_TIMEOUT_SECS", "soon"),
            // Shorter than the CLI's wait for the node's reply.
            ("MOSTRO_MCP_COMMAND_TIMEOUT_SECS", "20"),
            ("POW", "-1"),
        ] {
            let mut env = minimal();
            env.push((key, value));
            assert!(load(&env).is_err(), "{key}={value}");
        }
    }
}
