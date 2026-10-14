//! A minimal MCP client for trying mostro-mcp by hand: it starts the server
//! over stdio, lists the tools, and calls one.
//!
//! ```text
//! cargo run --example mcp_call -- --list
//! cargo run --example mcp_call -- list_orders '{"kind":"sell"}'
//! ```
//!
//! The server inherits this process's environment (MOSTRO_PUBKEY, RELAYS,
//! MCLI_DIR, MOSTRO_MCP_ALLOW_WRITES, …); `regtest/devenv.sh env alice` prints
//! a ready set. `MOSTRO_MCP_BIN` overrides the server binary.

use anyhow::{Context, bail};
use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::Value;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let tool = args
        .next()
        .context("usage: mcp_call --list | <tool> [json-arguments]")?;
    let arguments = match args.next() {
        Some(raw) => match serde_json::from_str(&raw).context("arguments must be JSON")? {
            Value::Object(map) => map,
            _ => bail!("arguments must be a JSON object"),
        },
        None => Default::default(),
    };

    let binary =
        std::env::var("MOSTRO_MCP_BIN").unwrap_or_else(|_| "target/debug/mostro-mcp".to_string());
    let transport = TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|_| {}))
        .with_context(|| format!("starting {binary}"))?;
    let client = ().serve(transport).await.context("MCP handshake")?;

    let tools = client.list_all_tools().await?;
    eprintln!("discovered {} tools:", tools.len());
    for tool in &tools {
        let read_only = tool.annotations.as_ref().and_then(|a| a.read_only_hint);
        eprintln!("  - {} (read_only: {read_only:?})", tool.name);
    }

    if tool != "--list" {
        eprintln!("\ncalling {tool} {}", Value::Object(arguments.clone()));
        let result = client
            .call_tool(CallToolRequestParams::new(tool).with_arguments(arguments))
            .await?;
        eprintln!("is_error: {:?}\n", result.is_error);
        for content in &result.content {
            if let Some(text) = content.as_text() {
                println!("{}", text.text);
            }
        }
    }

    client.cancel().await?;
    Ok(())
}
