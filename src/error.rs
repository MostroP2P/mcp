//! Errors a tool call can end in. Every one is shown to the agent as a tool
//! error, so each message says what to change.

use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid argument `{field}`: {reason}")]
    InvalidArgument { field: &'static str, reason: String },

    #[error(
        "state-changing tools are disabled on this server; restart it with \
         MOSTRO_MCP_ALLOW_WRITES=true to enable create_order and take_order"
    )]
    WritesDisabled,

    #[error("mostro-cli not found at `{0}`; set MOSTRO_CLI_PATH to the binary")]
    CliNotFound(String),

    #[error(
        "mostro-cli did not finish within {0:?} (MOSTRO_MCP_COMMAND_TIMEOUT_SECS) and was \
         stopped, but its message may have reached the node: check get_order or \
         list_orders before retrying"
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
