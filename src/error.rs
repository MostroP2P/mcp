//! Errors a tool call can end in. Every one is shown to the agent as a tool
//! error, so each message says what to change.

use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid argument `{field}`: {reason}")]
    InvalidArgument { field: &'static str, reason: String },

    #[error(
        "state-changing tools are disabled on this server; restart it with \
         MOSTRO_MCP_ALLOW_WRITES=true to enable them"
    )]
    WritesDisabled,

    #[error("mostro-cli not found at `{0}`; set MOSTRO_CLI_PATH to the binary")]
    CliNotFound(String),

    #[error(
        "mostro-cli did not finish within {0:?} (MOSTRO_MCP_COMMAND_TIMEOUT_SECS) and was \
         stopped, but its message may have reached the node: do not retry before \
         checking get_order, list_my_orders or list_messages"
    )]
    Timeout(Duration),

    #[error("mostro-cli failed ({status}): {stderr}")]
    CliFailed {
        status: String,
        stderr: String,
        stdout: String,
    },

    #[error("could not run mostro-cli: {0}")]
    Io(#[from] std::io::Error),

    /// mostro-cli started, then capturing, reading or waiting for it failed:
    /// unlike [`Error::Io`], it may have sent its request. Same text, so a
    /// tool that does not tell the two apart reads as before.
    #[error("could not run mostro-cli: {0}")]
    IoAfterStart(std::io::Error),

    #[error(
        "mostro-cli does not offer JSON output for: {}. Install a mostro-cli whose \
         `mostro-cli --output json schema` lists these commands",
        .missing.join(", ")
    )]
    ContractMismatch { missing: Vec<String> },

    #[error("configuration: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn invalid(field: &'static str, reason: impl Into<String>) -> Error {
    Error::InvalidArgument {
        field,
        reason: reason.into(),
    }
}
