//! mostro-mcp: an MCP server that exposes Mostro P2P operations to AI agents
//! by driving `mostro-cli` as a subprocess.
//!
//! ```text
//! MCP client ──stdio──▶ mostro-mcp ──subprocess──▶ mostro-cli ──Nostr──▶ mostrod
//! ```

pub mod cli_backend;
pub mod config;
pub mod contract;
pub mod error;
pub mod outcome;
pub mod server;
pub mod tools;
