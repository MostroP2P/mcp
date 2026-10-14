//! End to end against the local regtest network: MCP client → mostro-mcp →
//! mostro-cli → relay → mostrod and back. Ignored by default; run with
//!
//! ```text
//! regtest/devenv.sh up
//! eval "$(regtest/devenv.sh env alice)"
//! cargo test --test regtest_e2e -- --ignored
//! ```
//!
//! Read-only on purpose: the server is started with writes off whatever the
//! environment says. The write path is exercised by hand (README, "Regtest").

use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
};

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up) and its env"]
async fn list_orders_reaches_the_regtest_node() {
    let node = std::env::var("MOSTRO_PUBKEY")
        .expect("MOSTRO_PUBKEY unset: eval \"$(regtest/devenv.sh env alice)\" first");

    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_mostro-mcp")).configure(|cmd| {
            cmd.env("MOSTRO_MCP_ALLOW_WRITES", "false");
        }),
    )
    .expect("spawn mostro-mcp");
    let client = ().serve(transport).await.expect("MCP handshake");

    assert_eq!(client.list_all_tools().await.unwrap().len(), 4);

    let result = client
        .call_tool(CallToolRequestParams::new("list_orders"))
        .await
        .expect("tools/call");
    let text: String = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect();
    assert_ne!(result.is_error, Some(true), "{text}");
    assert!(text.contains("List Orders"), "{text}");
    assert!(text.contains(&node), "the answer names the node: {text}");

    client.cancel().await.unwrap();
}
