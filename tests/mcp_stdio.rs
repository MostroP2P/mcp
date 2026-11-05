//! The real binary, driven by a real MCP client over stdio. mostro-cli is a
//! stand-in script that echoes what it received, so these run anywhere; the
//! node itself is exercised by `regtest_e2e.rs`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult},
    service::RunningService,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::{Value, json};

const NODE: &str = "724497f34e6c4f55f9aa90f5e9dcc326c76a9f2ed817b0f0eb940ee7218ac8d5";

const OUTCOMES: &str = include_str!("fixtures/outcomes.json");

/// A stand-in `mostro-cli`. It answers with `reply.json` (printed as is, ending with the exit code in
/// `reply.code`) when the test wrote one, else with an `ok` document that
/// echoes the arguments and the node configuration it was handed.
fn fake_cli(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("mostro-cli");
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         here=$(dirname \"$0\")\n\
         if [ -f \"$here/reply.json\" ]; then cat \"$here/reply.json\"; exit \"$(cat \"$here/reply.code\")\"; fi\n\
         args=''; for a in \"$@\"; do args=\"$args\\\"$a\\\",\"; done\n\
         printf '{\"schema_version\":\"1\",\"command\":\"echo\",\"status\":\"ok\",\"data\":{\"node\":\"%s\",\"relays\":\"%s\",\"admin\":\"%s\",\"other\":\"%s\",\"args\":[%s\"end\"]},\"error\":null,\"warnings\":[],\"meta\":{}}\\n' \\\n\
           \"$MOSTRO_PUBKEY\" \"$RELAYS\" \"${ADMIN_NSEC:-none}\" \"${SOME_OTHER_SECRET:-none}\" \"$args\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Make the fake CLI answer every other command with the fixture `name`.
fn answer_with(dir: &tempfile::TempDir, name: &str) {
    let outcomes: Value = serde_json::from_str(OUTCOMES).unwrap();
    std::fs::write(
        dir.path().join("reply.json"),
        outcomes[name]["stdout"].to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("reply.code"),
        outcomes[name]["exit_code"].to_string(),
    )
    .unwrap();
}

async fn connect(cli: &Path, allow_writes: bool) -> RunningService<RoleClient, ()> {
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_mostro-mcp")).configure(|cmd| {
            cmd.env_clear()
                .env("MOSTRO_CLI_PATH", cli)
                .env("MOSTRO_PUBKEY", NODE)
                .env("RELAYS", "ws://localhost:7000")
                .env("MOSTRO_MCP_ALLOW_WRITES", allow_writes.to_string())
                // Must never reach the CLI, whatever the parent exports.
                .env("ADMIN_NSEC", "nsec1shouldneverbeforwarded")
                .env("SOME_OTHER_SECRET", "hunter2");
        }),
    )
    .expect("spawn mostro-mcp");
    ().serve(transport).await.expect("MCP handshake")
}

async fn call(
    client: &RunningService<RoleClient, ()>,
    tool: &'static str,
    args: Value,
) -> CallToolResult {
    let Value::Object(args) = args else {
        panic!("tool arguments must be an object")
    };
    client
        .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
        .await
        .expect("tools/call")
}

fn text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect()
}

#[tokio::test]
async fn a_client_discovers_four_typed_tools() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&fake_cli(&dir), false).await;

    let info = client.peer_info().expect("server info");
    assert_eq!(
        info.server_info.as_ref().map(|i| i.name.as_str()),
        Some("mostro-mcp")
    );
    assert!(
        info.instructions
            .as_deref()
            .unwrap_or("")
            .contains("BUY sats")
    );

    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        ["create_order", "get_order", "list_orders", "take_order"]
    );

    let take = tools.iter().find(|t| t.name == "take_order").unwrap();
    let schema = serde_json::to_value(&take.input_schema).unwrap();
    assert_eq!(schema["required"], json!(["order_id", "order_kind"]));

    client.cancel().await.unwrap();
}

/// The `data` of the document a tool returned.
fn data_of(result: &CallToolResult) -> Value {
    result
        .structured_content
        .clone()
        .expect("structured content")["data"]
        .clone()
}

#[tokio::test]
async fn list_orders_reaches_the_cli_with_the_node_config_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&fake_cli(&dir), false).await;

    let result = call(
        &client,
        "list_orders",
        json!({"kind": "sell", "currency": "usd"}),
    )
    .await;

    assert_ne!(result.is_error, Some(true), "{}", text(&result));
    let data = data_of(&result);
    assert_eq!(data["node"], NODE);
    assert_eq!(data["relays"], "ws://localhost:7000");
    assert_eq!(data["admin"], "none");
    assert_eq!(
        data["other"], "none",
        "only the node config reaches the CLI"
    );
    assert_eq!(
        data["args"],
        json!([
            "--output=json",
            "listorders",
            "--kind=sell",
            "--currency=USD",
            "end"
        ])
    );
    // The text block is the same document.
    let from_text: Value = serde_json::from_str(&text(&result)).unwrap();
    assert_eq!(from_text, result.structured_content.clone().unwrap());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn writes_are_refused_unless_the_server_opted_in() {
    let dir = tempfile::tempdir().unwrap();
    let take = json!({
        "order_id": "ffdc768a-37d8-429e-bfcc-0968be56b6f9",
        "order_kind": "sell",
    });

    let read_only = connect(&fake_cli(&dir), false).await;
    let refused = call(&read_only, "take_order", take.clone()).await;
    assert_eq!(refused.is_error, Some(true));
    assert!(text(&refused).contains("MOSTRO_MCP_ALLOW_WRITES=true"));
    read_only.cancel().await.unwrap();

    let writable = connect(&fake_cli(&dir), true).await;
    let taken = call(&writable, "take_order", take).await;
    assert_ne!(taken.is_error, Some(true), "{}", text(&taken));
    assert_eq!(data_of(&taken)["args"][1], "takesell");
    writable.cancel().await.unwrap();
}

#[tokio::test]
async fn the_three_statuses_reach_the_agent_as_a_result_an_error_and_a_warned_error() {
    for (fixture, error, starts_with, contains) in [
        ("list_empty", false, "{", "order.list"),
        ("release_node_refused", true, "{", "invalid_peer"),
        ("release_uncertain", true, "UNCERTAIN", "reply_timeout"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let cli = fake_cli(&dir);
        answer_with(&dir, fixture);
        let client = connect(&cli, false).await;

        let result = call(&client, "list_orders", json!({})).await;

        assert_eq!(result.is_error == Some(true), error, "{fixture}");
        let body = text(&result);
        assert!(body.starts_with(starts_with), "{fixture}: {body}");
        assert!(body.contains(contains), "{fixture}: {body}");
        assert!(result.structured_content.is_some(), "{fixture}");
        client.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn output_that_is_not_a_document_is_a_tool_error_and_the_server_keeps_serving() {
    let dir = tempfile::tempdir().unwrap();
    let cli = fake_cli(&dir);
    std::fs::write(dir.path().join("reply.json"), "📋 List Orders\n(table)\n").unwrap();
    std::fs::write(dir.path().join("reply.code"), "0").unwrap();
    let client = connect(&cli, false).await;

    let result = call(&client, "list_orders", json!({})).await;

    assert_eq!(result.is_error, Some(true));
    assert!(
        text(&result).contains("not one JSON outcome document"),
        "{}",
        text(&result)
    );
    assert_eq!(client.list_all_tools().await.unwrap().len(), 4);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_bad_argument_is_a_tool_error_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&fake_cli(&dir), true).await;

    let result = call(&client, "get_order", json!({"order_id": "$(reboot)"})).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text(&result).contains("is not an order id"));

    // The server is still serving afterwards.
    assert_eq!(client.list_all_tools().await.unwrap().len(), 4);
    client.cancel().await.unwrap();
}
