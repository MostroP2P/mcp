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

/// A stand-in `mostro-cli`. It answers `schema` with the contract fixture; any
/// other command with `reply.json` (printed as is, ending with the exit code in
/// `reply.code`) when the test wrote one, else with an `ok` document that
/// echoes the arguments and the node configuration it was handed.
fn fake_cli(dir: &tempfile::TempDir) -> PathBuf {
    let outcomes: Value = serde_json::from_str(OUTCOMES).unwrap();
    std::fs::write(
        dir.path().join("schema.json"),
        outcomes["schema"]["stdout"].to_string(),
    )
    .unwrap();
    let path = dir.path().join("mostro-cli");
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         here=$(dirname \"$0\")\n\
         if [ \"$2\" = schema ]; then cat \"$here/schema.json\"; exit 0; fi\n\
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
    connect_with(cli, allow_writes, &[]).await
}

async fn connect_with(
    cli: &Path,
    allow_writes: bool,
    extra_env: &[(&str, &str)],
) -> RunningService<RoleClient, ()> {
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_mostro-mcp")).configure(|cmd| {
            cmd.env_clear()
                .env("MOSTRO_CLI_PATH", cli)
                .env("MOSTRO_PUBKEY", NODE)
                .env("RELAYS", "ws://localhost:7000")
                .env("MOSTRO_MCP_ALLOW_WRITES", allow_writes.to_string())
                // Must never reach the CLI, whatever the parent exports.
                .env("ADMIN_NSEC", "nsec1shouldneverbeforwarded")
                .env("SOME_OTHER_SECRET", "hunter2")
                .envs(extra_env.iter().copied());
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
async fn a_client_discovers_seventeen_typed_tools() {
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
    let instructions = info.instructions.as_deref().unwrap_or("");
    for reference in [
        "https://mostro.network/mostro-cli-book/",
        "errors.html",
        "reference/actions.html",
        "flows/uncertain.html",
        "watch_order",
        "read_chat",
        "open_dispute",
        "rate_counterparty",
    ] {
        assert!(
            instructions.contains(reference),
            "the agent is told where the outcome documents are described: {reference}"
        );
    }

    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "add_bond_invoice",
            "add_invoice",
            "cancel",
            "create_order",
            "fiat_sent",
            "get_order",
            "list_messages",
            "list_my_orders",
            "list_orders",
            "open_dispute",
            "rate_counterparty",
            "read_chat",
            "release",
            "send_chat",
            "sync_orders",
            "take_order",
            "watch_order"
        ]
    );

    let take = tools.iter().find(|t| t.name == "take_order").unwrap();
    let schema = serde_json::to_value(&take.input_schema).unwrap();
    assert_eq!(schema["required"], json!(["order_id", "order_kind"]));
    let release = tools.iter().find(|t| t.name == "release").unwrap();
    let schema = serde_json::to_value(&release.input_schema).unwrap();
    assert_eq!(schema["required"], json!(["order_id", "fiat_received"]));
    let watch = tools.iter().find(|t| t.name == "watch_order").unwrap();
    let schema = serde_json::to_value(&watch.input_schema).unwrap();
    assert_eq!(schema["required"], json!(["order_id"]));
    let send = tools.iter().find(|t| t.name == "send_chat").unwrap();
    let schema = serde_json::to_value(&send.input_schema).unwrap();
    assert_eq!(schema["required"], json!(["order_id", "message"]));
    let dispute = tools.iter().find(|t| t.name == "open_dispute").unwrap();
    let schema = serde_json::to_value(&dispute.input_schema).unwrap();
    assert_eq!(schema["required"], json!(["order_id"]));
    let rate = tools
        .iter()
        .find(|t| t.name == "rate_counterparty")
        .unwrap();
    let schema = serde_json::to_value(&rate.input_schema).unwrap();
    assert_eq!(schema["required"], json!(["order_id", "rating"]));
    let rating = &schema["properties"]["rating"];
    assert_eq!(rating["type"], "integer", "{rating}");
    assert_eq!(rating["minimum"], json!(1), "{rating}");
    assert_eq!(rating["maximum"], json!(5), "{rating}");

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
            "order",
            "list",
            "--side=sell",
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
    assert_eq!(data_of(&taken)["args"][2], "take-sell");
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
async fn a_write_confirms_for_the_cli_and_an_uncertain_take_warns_without_retrying() {
    let dir = tempfile::tempdir().unwrap();
    let cli = fake_cli(&dir);
    let client = connect(&cli, true).await;
    let created = call(
        &client,
        "create_order",
        json!({"kind": "sell", "fiat_code": "usd", "fiat_amount_min": 50,
               "fiat_amount_max": 150, "payment_method": "bank transfer"}),
    )
    .await;
    assert_ne!(created.is_error, Some(true), "{}", text(&created));
    let args = data_of(&created)["args"].clone();
    assert_eq!(args[1], "order");
    assert_eq!(args[2], "new");
    assert_eq!(
        args.as_array()
            .unwrap()
            .iter()
            .filter(|a| *a == "--yes")
            .count(),
        1
    );
    assert!(
        args.as_array()
            .unwrap()
            .contains(&json!("--fiat-amount-max=150"))
    );
    client.cancel().await.unwrap();

    let dir = tempfile::tempdir().unwrap();
    let cli = fake_cli(&dir);
    answer_with(&dir, "take_uncertain");
    let client = connect(&cli, true).await;
    let taken = call(
        &client,
        "take_order",
        json!({"order_id": "ffdc768a-37d8-429e-bfcc-0968be56b6f9", "order_kind": "sell"}),
    )
    .await;
    assert_eq!(taken.is_error, Some(true));
    assert!(text(&taken).starts_with("UNCERTAIN"), "{}", text(&taken));
    client.cancel().await.unwrap();
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
    assert_eq!(client.list_all_tools().await.unwrap().len(), 17);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_cli_that_lacks_the_json_commands_stops_the_server_with_the_missing_names() {
    let dir = tempfile::tempdir().unwrap();
    let cli = fake_cli(&dir);
    // The fixture without order.take-buy.
    let outcomes: Value = serde_json::from_str(OUTCOMES).unwrap();
    let mut schema = outcomes["schema"]["stdout"].clone();
    schema["data"]["commands"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["name"] != "order.take-buy");
    std::fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mostro-mcp"))
        .env_clear()
        .env("MOSTRO_CLI_PATH", &cli)
        .env("MOSTRO_PUBKEY", NODE)
        .env("RELAYS", "ws://localhost:7000")
        .env("MOSTRO_MCP_ALLOW_WRITES", "true")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "a normal error exit, not a panic"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mostro-cli does not offer JSON output for:"),
        "{stderr}"
    );
    assert!(stderr.contains("order.take-buy"), "{stderr}");
    assert!(
        !stderr.contains("order.take-sell"),
        "only what is missing: {stderr}"
    );
}

#[tokio::test]
async fn a_cli_that_does_not_know_schema_stops_the_server_clearly() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("mostro-cli");
    std::fs::write(
        &old,
        "#!/bin/sh\necho 'error: unrecognized subcommand' >&2\nexit 2\n",
    )
    .unwrap();
    std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mostro-mcp"))
        .env_clear()
        .env("MOSTRO_CLI_PATH", &old)
        .env("MOSTRO_PUBKEY", NODE)
        .env("RELAYS", "ws://localhost:7000")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("predates the JSON interface"), "{stderr}");
}

#[tokio::test]
async fn a_bad_argument_is_a_tool_error_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&fake_cli(&dir), true).await;

    let result = call(&client, "get_order", json!({"order_id": "$(reboot)"})).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text(&result).contains("is not an order id"));

    // The server is still serving afterwards.
    assert_eq!(client.list_all_tools().await.unwrap().len(), 17);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_whole_trade_reaches_the_cli_as_its_json_commands() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&fake_cli(&dir), true).await;
    let id = "ffdc768a-37d8-429e-bfcc-0968be56b6f9";

    for (tool, args, expected) in [
        ("list_my_orders", json!({}), vec!["order", "mine"]),
        (
            "list_messages",
            json!({"order_id": id, "since_minutes": 60}),
            vec![
                "message",
                "list",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--since=60",
            ],
        ),
        (
            "add_invoice",
            json!({"order_id": id, "invoice": "lnbcrt1pexample"}),
            vec![
                "order",
                "add-invoice",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--invoice=lnbcrt1pexample",
                "--yes",
            ],
        ),
        (
            "fiat_sent",
            json!({"order_id": id}),
            vec![
                "order",
                "fiat-sent",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--yes",
            ],
        ),
        (
            "release",
            json!({"order_id": id, "fiat_received": true}),
            vec![
                "order",
                "release",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--yes",
            ],
        ),
        (
            "add_bond_invoice",
            json!({"order_id": id, "invoice": "lnbcrt1pbond"}),
            vec![
                "bond",
                "add-invoice",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--invoice=lnbcrt1pbond",
                "--yes",
            ],
        ),
        (
            "cancel",
            json!({"order_id": id}),
            vec![
                "order",
                "cancel",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--yes",
            ],
        ),
        ("sync_orders", json!({}), vec!["order", "sync"]),
        (
            "watch_order",
            json!({"order_id": id, "wait_seconds": 5}),
            vec![
                "order",
                "watch",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--timeout=5",
            ],
        ),
        (
            "read_chat",
            json!({"order_id": id}),
            vec![
                "order",
                "chat",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
            ],
        ),
        (
            "send_chat",
            json!({"order_id": id, "message": "Alias: mi.alias.belo"}),
            vec![
                "order",
                "chat",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--message=Alias: mi.alias.belo",
                "--yes",
            ],
        ),
        (
            "open_dispute",
            json!({"order_id": id}),
            vec![
                "dispute",
                "open",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--yes",
            ],
        ),
        (
            "rate_counterparty",
            json!({"order_id": id, "rating": 5}),
            vec![
                "order",
                "rate",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--rating=5",
                "--yes",
            ],
        ),
    ] {
        let result = call(&client, tool, args).await;
        assert_ne!(result.is_error, Some(true), "{tool}: {}", text(&result));
        let mut want = vec![json!("--output=json")];
        want.extend(expected.into_iter().map(|a| json!(a)));
        want.push(json!("end"));
        assert_eq!(data_of(&result)["args"], json!(want), "{tool}");
    }

    let refused = call(
        &client,
        "release",
        json!({"order_id": id, "fiat_received": false}),
    )
    .await;
    assert_eq!(refused.is_error, Some(true));
    assert!(
        text(&refused).contains("fiat_received"),
        "{}",
        text(&refused)
    );
    // A whole number written as a float is the same rating.
    let rated = call(
        &client,
        "rate_counterparty",
        json!({"order_id": id, "rating": 5.0}),
    )
    .await;
    assert_ne!(rated.is_error, Some(true), "{}", text(&rated));
    assert_eq!(data_of(&rated)["args"][4], "--rating=5");
    for bad in [
        json!(0),
        json!(6),
        json!(-1),
        json!(4.5),
        json!(5.5),
        json!(6.0),
        json!("5"),
    ] {
        let refused = call(
            &client,
            "rate_counterparty",
            json!({"order_id": id, "rating": bad}),
        )
        .await;
        assert_eq!(refused.is_error, Some(true), "{bad}: {}", text(&refused));
        assert!(
            refused.structured_content.is_none(),
            "{bad}: refused before the CLI ran: {}",
            text(&refused)
        );
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn the_cli_s_own_errors_for_the_new_tools_reach_the_agent() {
    for (fixture, tool, args, contains) in [
        (
            "messages_identity_missing",
            "list_messages",
            json!({}),
            "identity_missing",
        ),
        (
            "mine_newest_first",
            "list_my_orders",
            json!({}),
            "order.mine",
        ),
        (
            "bond_invoice_accepted",
            "add_bond_invoice",
            json!({"order_id": "00000000-0000-0000-0000-000000000001", "invoice": "lnbcrt1p"}),
            "invoice_accepted",
        ),
        (
            "watch_new_records",
            "watch_order",
            json!({"order_id": "00000000-0000-0000-0000-000000000001"}),
            "\"cursor\"",
        ),
        (
            "chat_read",
            "read_chat",
            json!({"order_id": "00000000-0000-0000-0000-000000000001"}),
            "Received, releasing now.",
        ),
        (
            "chat_counterparty_unknown",
            "read_chat",
            json!({"order_id": "00000000-0000-0000-0000-000000000001"}),
            "counterparty_unknown",
        ),
        (
            "chat_send_uncertain",
            "send_chat",
            json!({"order_id": "00000000-0000-0000-0000-000000000001", "message": "hola"}),
            "UNCERTAIN",
        ),
        (
            "dispute_opened",
            "open_dispute",
            json!({"order_id": "00000000-0000-0000-0000-000000000001"}),
            "00000000-0000-0000-0000-000000000d16",
        ),
        (
            "dispute_node_refused",
            "open_dispute",
            json!({"order_id": "00000000-0000-0000-0000-000000000001"}),
            "not_allowed_by_status",
        ),
        (
            "dispute_order_unknown",
            "open_dispute",
            json!({"order_id": "00000000-0000-0000-0000-000000000001"}),
            "order_unknown",
        ),
        (
            "dispute_uncertain",
            "open_dispute",
            json!({"order_id": "00000000-0000-0000-0000-000000000001"}),
            "UNCERTAIN",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let cli = fake_cli(&dir);
        answer_with(&dir, fixture);
        let client = connect(&cli, true).await;
        let result = call(&client, tool, args).await;
        assert!(
            text(&result).contains(contains),
            "{fixture}: {}",
            text(&result)
        );
        client.cancel().await.unwrap();
    }
}

/// Each outcome of a rating reaches the agent as the CLI's own document: the
/// result's error flag and structured content, not a substring of its text.
#[tokio::test]
async fn the_cli_s_rating_outcomes_reach_the_agent_intact() {
    let dir = tempfile::tempdir().unwrap();
    let cli = fake_cli(&dir);
    let client = connect(&cli, true).await;
    let order = "00000000-0000-0000-0000-000000000001";
    // The fake CLI reads its reply on every run: one server serves all three.
    let outcome = async |fixture: &str| {
        answer_with(&dir, fixture);
        call(
            &client,
            "rate_counterparty",
            json!({"order_id": order, "rating": 5}),
        )
        .await
    };

    let rated = outcome("rate_received").await;
    assert_ne!(rated.is_error, Some(true), "{}", text(&rated));
    let document = rated.structured_content.clone().expect("a document");
    assert_eq!(document["status"], "ok");
    assert_eq!(document["command"], "order.rate");
    assert_eq!(document["data"]["order_id"], order);
    assert_eq!(document["data"]["rating"], 5);

    let refused = outcome("rate_node_refused").await;
    assert_eq!(refused.is_error, Some(true));
    let document = refused.structured_content.clone().expect("a document");
    assert_eq!(document["status"], "error");
    assert_eq!(document["error"]["code"], "node_refused");
    assert_eq!(document["error"]["reason"], "invalid_order_status");
    assert!(
        !text(&refused).starts_with("UNCERTAIN"),
        "{}",
        text(&refused)
    );

    let uncertain = outcome("rate_uncertain").await;
    assert_eq!(uncertain.is_error, Some(true));
    let document = uncertain.structured_content.clone().expect("a document");
    assert_eq!(document["status"], "uncertain");
    assert_eq!(document["error"]["code"], "reply_timeout");
    let body = text(&uncertain);
    assert!(body.starts_with("UNCERTAIN RATING"), "{body}");
    assert!(
        body.contains("Never call rate_counterparty again for this order and side")
            && body.contains("not even if the user asks")
            && body.contains("may have been sent, but whether it was recorded is unknown"),
        "{body}"
    );
    // No text tells the agent another attempt is allowed or harmless.
    for permission in [
        "harmless",
        "usually pointless",
        "another attempt",
        "one more attempt",
        "go-ahead",
        "never count twice",
        "again on your own",
    ] {
        assert!(!body.contains(permission), "{permission}: {body}");
    }

    // `uncertain` / `pow_required` (no CLI example exists; this is the
    // uncertain example with the error the CLI builds for it).
    let mut short: Value = serde_json::from_str(OUTCOMES).unwrap();
    let short = short["rate_uncertain"]["stdout"].take();
    let mut short = short;
    short["error"] = json!({"code": "pow_required",
        "message": "The node requires more proof of work than is configured.",
        "required": 22, "configured": 0});
    std::fs::write(dir.path().join("reply.json"), short.to_string()).unwrap();
    let pow = call(
        &client,
        "rate_counterparty",
        json!({"order_id": order, "rating": 5}),
    )
    .await;
    assert_eq!(pow.is_error, Some(true));
    let document = pow.structured_content.clone().expect("a document");
    assert_eq!(document["error"]["code"], "pow_required");
    let body = text(&pow);
    assert!(
        body.starts_with("UNCERTAIN RATING, TOO LITTLE PROOF OF WORK")
            && body.contains("the node requires 22 bits")
            && body.contains("future requests")
            && body.contains("Never call rate_counterparty again for this order and side")
            && body.contains("not even if the user asks"),
        "{body}"
    );
    for permission in [
        "one more attempt",
        "another attempt",
        "go-ahead",
        "harmless",
    ] {
        assert!(!body.contains(permission), "{permission}: {body}");
    }
    client.cancel().await.unwrap();
}

/// The server with piped stdio, and a thread collecting its stderr.
fn spawn_raw(
    cli: &Path,
    extra_env: &[(&str, &str)],
) -> (std::process::Child, std::sync::mpsc::Receiver<String>) {
    use std::io::BufRead;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_mostro-mcp"))
        .env_clear()
        .envs(extra_env.iter().copied())
        .env("MOSTRO_CLI_PATH", cli)
        .env("MOSTRO_PUBKEY", NODE)
        .env("RELAYS", "ws://localhost:7000")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (lines, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
        {
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    (child, rx)
}

/// Waits for a stderr line containing `needle`; returns every line read.
fn stderr_until(rx: &std::sync::mpsc::Receiver<String>, needle: &str) -> String {
    let mut seen = String::new();
    while let Ok(line) = rx.recv_timeout(std::time::Duration::from_secs(10)) {
        seen.push_str(&line);
        seen.push('\n');
        if line.contains(needle) {
            return seen;
        }
    }
    panic!("no stderr line with `{needle}`:\n{seen}");
}

#[tokio::test]
async fn a_termination_signal_is_logged_before_the_server_exits() {
    let dir = tempfile::tempdir().unwrap();
    let (mut child, stderr) = spawn_raw(&fake_cli(&dir), &[]);
    stderr_until(&stderr, "offers the JSON commands");

    let killed = std::process::Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());

    let log = stderr_until(&stderr, "SIGTERM");
    assert!(log.contains("stopped by a signal"), "{log}");
    assert_eq!(child.wait().unwrap().code(), Some(128 + 15));
}

#[tokio::test]
async fn a_client_that_closes_the_connection_is_logged() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let (mut child, stderr) = spawn_raw(&fake_cli(&dir), &[]);
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"t","version":"0"}}}}}}"#
    )
    .unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();
    stderr_until(&stderr, "MCP session started");

    drop(stdin);

    let log = stderr_until(&stderr, "MCP session ended");
    assert!(log.contains("Closed"), "{log}");
    assert_eq!(child.wait().unwrap().code(), Some(0));
}

#[tokio::test]
async fn the_log_file_shows_who_launched_the_server_and_every_tool_call() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("mcp.log");
    let client = connect_with(
        &fake_cli(&dir),
        false,
        &[("MOSTRO_MCP_LOG_FILE", log.to_str().unwrap())],
    )
    .await;
    call(&client, "list_orders", json!({})).await;
    call(&client, "get_order", json!({"order_id": "nope"})).await;
    client.cancel().await.unwrap();

    let mut text = String::new();
    for _ in 0..50 {
        text = std::fs::read_to_string(&log).unwrap_or_default();
        if text.contains("MCP session ended") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    for needle in [
        "starting mostro-mcp",
        "ppid=",
        "client initialized",
        "client=\"rmcp\"",
        "tool call tool=\"list_orders\"",
        "tool call finished tool=\"list_orders\" outcome=\"ok\"",
        "tool call finished tool=\"get_order\" outcome=\"error\"",
        "elapsed_ms=",
        "MCP session ended",
    ] {
        assert!(text.contains(needle), "{needle} missing from:\n{text}");
    }
}

#[tokio::test]
async fn a_heartbeat_marks_that_the_server_is_alive() {
    let dir = tempfile::tempdir().unwrap();
    let (mut child, stderr) = spawn_raw(&fake_cli(&dir), &[("MOSTRO_MCP_HEARTBEAT_SECS", "1")]);
    let log = stderr_until(&stderr, "alive");
    assert!(log.contains("uptime_secs="), "{log}");
    child.kill().unwrap();
    child.wait().unwrap();
}

#[tokio::test]
async fn a_server_that_cannot_start_says_why_in_the_log_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("mcp.log");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mostro-mcp"))
        .env_clear()
        .env("MOSTRO_CLI_PATH", dir.path().join("no-such-cli"))
        .env("MOSTRO_PUBKEY", NODE)
        .env("RELAYS", "ws://localhost:7000")
        .env("MOSTRO_MCP_LOG_FILE", &log)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("mostro-mcp stopped"), "{text}");
    assert!(text.contains("could not run `mostro-cli schema`"), "{text}");
    assert!(
        text.contains("MOSTRO_CLI_PATH"),
        "the cause, not just the context: {text}"
    );
}
