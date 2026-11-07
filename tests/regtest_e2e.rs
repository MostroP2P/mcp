//! End to end against the local regtest network: MCP client → mostro-mcp →
//! mostro-cli → relay → mostrod and back. Ignored by default; run with
//!
//! ```text
//! regtest/devenv.sh up
//! cargo test --test regtest_e2e -- --ignored --test-threads=1
//! ```
//!
//! Each test starts one server per test user, with the environment that
//! `regtest/devenv.sh env <user>` prints (set `MOSTRO_CLI_PATH` when the CLI is
//! not at `~/dev/mostro-cli/target/release/mostro-cli`). Payments are made the
//! way a user makes them, from the user's own wallet (`devenv.sh pay`): no tool
//! pays.

use std::time::Duration;

use rmcp::{
    RoleClient, ServiceExt,
    model::CallToolRequestParams,
    service::RunningService,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::{Value, json};

const DEVENV: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/regtest/devenv.sh");

/// Seconds between two reads while waiting for the node, and how many reads.
const POLL_EVERY: Duration = Duration::from_secs(3);
const POLL_TRIES: u32 = 40;

/// Runs `devenv.sh` with `args` and returns its trimmed stdout.
fn devenv(args: &[&str]) -> String {
    let output = std::process::Command::new(DEVENV)
        .args(args)
        .output()
        .expect("run regtest/devenv.sh");
    assert!(
        output.status.success(),
        "devenv.sh {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// The `export NAME="value"` lines of `devenv.sh env <who>`.
fn user_env(who: &str) -> Vec<(String, String)> {
    devenv(&["env", who])
        .lines()
        .filter_map(|line| line.strip_prefix("export "))
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_string(), value.trim_matches('"').to_string()))
        .collect()
}

async fn connect(who: &str, allow_writes: bool) -> RunningService<RoleClient, ()> {
    let env = user_env(who);
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_mostro-mcp")).configure(|cmd| {
            cmd.envs(env)
                .env("MOSTRO_MCP_ALLOW_WRITES", allow_writes.to_string());
        }),
    )
    .expect("spawn mostro-mcp");
    ().serve(transport).await.expect("MCP handshake")
}

/// The outcome document of a call that must succeed.
async fn ok(client: &RunningService<RoleClient, ()>, tool: &'static str, args: Value) -> Value {
    let (document, error) = call(client, tool, args).await;
    assert!(!error, "{tool}: {document}");
    assert_eq!(document["status"], "ok", "{tool}: {document}");
    document
}

/// The outcome document of a call and whether it was a tool error.
async fn call(
    client: &RunningService<RoleClient, ()>,
    tool: &'static str,
    args: Value,
) -> (Value, bool) {
    let Value::Object(args) = args else {
        panic!("tool arguments must be an object")
    };
    let result = client
        .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
        .await
        .expect("tools/call");
    let text: String = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect();
    let document = result.structured_content.clone().unwrap_or(json!(text));
    (document, result.is_error == Some(true))
}

/// Reads until `found` returns something; the node answers in its own time.
async fn poll<T, F, Fut>(what: &str, mut found: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    for _ in 0..POLL_TRIES {
        if let Some(value) = found().await {
            return value;
        }
        tokio::time::sleep(POLL_EVERY).await;
    }
    panic!("timed out waiting for {what}");
}

/// The order's status as the node reports it to `client`'s identity.
async fn status(client: &RunningService<RoleClient, ()>, id: &str) -> Option<String> {
    let (document, _) = call(client, "get_order", json!({"order_id": id})).await;
    document["data"]["orders"][0]["status"]
        .as_str()
        .map(String::from)
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn list_orders_reaches_the_regtest_node() {
    let node = user_env("alice")
        .into_iter()
        .find(|(key, _)| key == "MOSTRO_PUBKEY")
        .map(|(_, value)| value)
        .unwrap();
    let client = connect("alice", false).await;

    assert_eq!(client.list_all_tools().await.unwrap().len(), 12);
    let document = ok(&client, "list_orders", json!({})).await;
    assert_eq!(document["command"], "order.list");
    assert_eq!(document["data"]["node_pubkey"], node);

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn a_whole_trade_runs_through_mcp() {
    trade(true).await;
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn a_take_without_an_invoice_is_completed_with_add_invoice() {
    trade(false).await;
}

/// The next step a user's messages about `id` ask for, with its invoice.
async fn asked(client: &RunningService<RoleClient, ()>, id: &str, step: &str) -> Option<String> {
    let (document, _) = call(client, "list_messages", json!({"order_id": id})).await;
    document["data"]["messages"]
        .as_array()?
        .iter()
        .find(|m| m["next_step"] == step)
        .map(|m| {
            m["payload"]["invoice"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
}

/// Alice sells 15 000 sats to Bob for 15 USD. Bob gives his invoice with the
/// take, or later when the node asks for it (`add_invoice`).
async fn trade(invoice_on_take: bool) {
    let alice = connect("alice", true).await;
    let bob = connect("bob", true).await;

    // Alice sells 15 000 sats for 15 USD.
    let created = ok(
        &alice,
        "create_order",
        json!({"kind": "sell", "fiat_code": "USD", "fiat_amount": 15,
               "payment_method": "bank transfer", "amount_sats": 15000}),
    )
    .await;
    let id = created["data"]["order"]["id"]
        .as_str()
        .expect("the new order's id")
        .to_string();

    // Bob takes it, with or without the invoice his sats go to.
    let invoice = devenv(&["invoice", "bob", "15000"]);
    let mut take = json!({"order_id": id, "order_kind": "sell"});
    if invoice_on_take {
        take["invoice"] = json!(invoice);
    }
    ok(&bob, "take_order", take).await;

    // The node asks Alice for the hold invoice, which she pays from her
    // wallet, and Bob for his invoice if he gave none, in its own order.
    let (mut paid, mut added) = (false, invoice_on_take);
    let mut active = false;
    for _ in 0..POLL_TRIES {
        if !paid && let Some(hold) = asked(&alice, &id, "pay_hold_invoice").await {
            devenv(&["pay", "alice", &hold]);
            paid = true;
        }
        if !added && asked(&bob, &id, "add_invoice").await.is_some() {
            ok(
                &bob,
                "add_invoice",
                json!({"order_id": id, "invoice": invoice}),
            )
            .await;
            added = true;
        }
        if status(&bob, &id).await.as_deref() == Some("active") {
            active = true;
            break;
        }
        tokio::time::sleep(POLL_EVERY).await;
    }
    assert!(active, "timed out waiting for the trade to become active");
    assert!(paid && added);

    // Bob sends the fiat and says so.
    ok(&bob, "fiat_sent", json!({"order_id": id})).await;

    // Alice sees the fiat arrive and releases.
    poll("the fiat-sent status", || async {
        (status(&alice, &id).await.as_deref() == Some("fiat-sent")).then_some(())
    })
    .await;
    ok(
        &alice,
        "release",
        json!({"order_id": id, "fiat_received": true}),
    )
    .await;
    poll("the trade to succeed", || async {
        (status(&alice, &id).await.as_deref() == Some("success")).then_some(())
    })
    .await;

    // Both keep the order in their local store.
    for client in [&alice, &bob] {
        let mine = ok(client, "list_my_orders", json!({})).await;
        assert!(
            mine["data"]["orders"]
                .as_array()
                .unwrap()
                .iter()
                .any(|o| o["id"] == id.as_str()),
            "{mine}"
        );
    }

    alice.cancel().await.unwrap();
    bob.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn a_pending_order_is_canceled_and_sync_finds_nothing_missing() {
    let alice = connect("alice", true).await;
    let created = ok(
        &alice,
        "create_order",
        json!({"kind": "buy", "fiat_code": "EUR", "fiat_amount": 20,
               "payment_method": "SEPA", "amount_sats": 20000}),
    )
    .await;
    let id = created["data"]["order"]["id"].as_str().unwrap().to_string();

    ok(&alice, "cancel", json!({"order_id": id})).await;
    poll("the order to be canceled", || async {
        (status(&alice, &id).await.as_deref() == Some("canceled")).then_some(())
    })
    .await;

    let synced = ok(&alice, "sync_orders", json!({"since_minutes": 60})).await;
    assert_eq!(synced["command"], "order.sync", "{synced}");
    alice.cancel().await.unwrap();
}
