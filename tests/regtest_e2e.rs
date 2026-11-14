//! End to end against the local regtest network: MCP client → mostro-mcp →
//! mostro-cli → relay → mostrod and back. Ignored by default; run with
//!
//! ```text
//! regtest/devenv.sh up
//! cargo test --test regtest_e2e -- --ignored --test-threads=1 --skip loses_the_bond
//! MOSTRO_BONDS=1 regtest/devenv.sh up
//! cargo test --test regtest_e2e -- --ignored --test-threads=1
//! ```
//!
//! The slash test needs the node with bonds: on one without, it fails rather
//! than pass without exercising anything.
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

/// How many watch_order calls a test makes before giving up.
const WATCH_CALLS: u32 = 12;

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

    assert_eq!(client.list_all_tools().await.unwrap().len(), 17);
    let document = ok(&client, "list_orders", json!({})).await;
    assert_eq!(document["command"], "order.list");
    assert_eq!(document["data"]["node_pubkey"], node);

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn a_whole_trade_runs_through_mcp() {
    trade(Maker::Seller, true).await;
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn a_take_without_an_invoice_is_completed_with_add_invoice() {
    trade(Maker::Seller, false).await;
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn taking_a_buy_order_the_taker_pays_the_hold_invoice() {
    trade(Maker::Buyer, false).await;
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn a_buy_order_can_carry_the_buyers_invoice() {
    trade(Maker::Buyer, true).await;
}

/// Who publishes the order. Alice always sells the sats and Bob buys them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Maker {
    Seller,
    Buyer,
}

/// Whether the node was started with bonds (`MOSTRO_BONDS=1 devenv.sh up`).
fn bonds_enabled() -> bool {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/regtest/.state/mostrod.toml"
    ))
    .is_ok_and(|settings| settings.contains("[anti_abuse_bond]"))
}

/// The newest message a user received about `id` that asks for `step`
/// (the list is oldest first).
async fn asked(client: &RunningService<RoleClient, ()>, id: &str, step: &str) -> Option<Value> {
    let (document, _) = call(client, "list_messages", json!({"order_id": id})).await;
    document["data"]["messages"]
        .as_array()?
        .iter()
        .rev()
        .find(|m| m["next_step"] == step)
        .cloned()
}

/// The invoice a user is asked to pay for `id`. The node can send the step
/// twice: first with the counterparty (`peer`), then with the invoice
/// (`payment_request`); only the second carries it.
async fn invoice_asked(client: &RunningService<RoleClient, ()>, id: &str) -> Option<String> {
    let (document, _) = call(client, "list_messages", json!({"order_id": id})).await;
    document["data"]["messages"]
        .as_array()?
        .iter()
        .rev()
        .find(|m| m["next_step"] == "pay_hold_invoice" && m["payload"]["kind"] == "payment_request")
        .and_then(|m| m["payload"]["invoice"].as_str())
        .map(String::from)
}

/// Alice sells 15 000 sats to Bob for 15 USD. `maker` publishes the order and
/// the other takes it. Bob's invoice comes with the order or the take when
/// `invoice_first`, else when the node asks for it (`add_invoice`). Every
/// payment, the taker's bond included, is made from the payer's own wallet.
async fn trade(maker: Maker, invoice_first: bool) {
    let ActiveTrade {
        alice,
        bob,
        id,
        invoice,
    } = until_active(maker, invoice_first).await;

    // Alice gives Bob her payment details in the order's chat; Bob's watch
    // returns with them, and the chat shows them as the counterparty's.
    let details = format!("Pay 15 USD to alias alice.bank, order {id}");
    let sent = ok(
        &alice,
        "send_chat",
        json!({"order_id": id, "message": details}),
    )
    .await;
    assert_eq!(sent["data"]["sent"]["text"], details.as_str(), "{sent}");
    let seen = watch_until(&bob, &id, |found| {
        found["data"]["chat"]
            .as_array()
            .is_some_and(|chat| chat.iter().any(|m| m["text"] == details.as_str()))
    })
    .await;
    assert_eq!(
        seen["data"]["counterparty_pubkey"].as_str().map(str::len),
        Some(64)
    );
    let read = ok(&bob, "read_chat", json!({"order_id": id})).await;
    assert!(
        read["data"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["text"] == details.as_str() && m["from"] == "counterparty"),
        "{read}"
    );

    until_success(&alice, &bob, &id).await;
    assert!(settled("bob", &invoice), "Bob's sats arrived");
    // Bob's watch sees the trade end.
    watch_until(&bob, &id, |found| found["data"]["ended"] == true).await;

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

/// A trade that reached `active`: both servers, the order and Bob's invoice.
struct ActiveTrade {
    alice: RunningService<RoleClient, ()>,
    bob: RunningService<RoleClient, ()>,
    id: String,
    invoice: String,
}

/// Takes a trade of `maker`'s order up to `active`, as [`trade`] describes.
async fn until_active(maker: Maker, invoice_first: bool) -> ActiveTrade {
    let alice = connect("alice", true).await;
    let bob = connect("bob", true).await;
    let invoice = devenv(&["invoice", "bob", "15000"]);
    let (side, maker_client, taker_client, taker) = match maker {
        Maker::Seller => ("sell", &alice, &bob, "bob"),
        Maker::Buyer => ("buy", &bob, &alice, "alice"),
    };

    let mut order = json!({"kind": side, "fiat_code": "USD", "fiat_amount": 15,
                           "payment_method": "bank transfer", "amount_sats": 15000});
    if maker == Maker::Buyer && invoice_first {
        order["invoice"] = json!(invoice);
    }
    let created = ok(maker_client, "create_order", order).await;
    let id = created["data"]["order"]["id"]
        .as_str()
        .expect("the new order's id")
        .to_string();

    let mut take = json!({"order_id": id, "order_kind": side});
    if maker == Maker::Seller && invoice_first {
        take["invoice"] = json!(invoice);
    }
    let taken = ok(taker_client, "take_order", take).await;
    let bonded = taken["data"]["next_step"] == "pay_bond_invoice";
    assert_eq!(
        bonded,
        bonds_enabled(),
        "a bond exactly when the node takes them: {taken}"
    );
    if bonded {
        let bond = taken["data"]["payment_request"].as_str().unwrap();
        devenv(&["pay", taker, bond]);
    }

    // The node asks Alice for the hold invoice, which she pays from her
    // wallet, and Bob for his invoice if he gave none, in its own order.
    let (mut paid, mut added) = (false, invoice_first);
    let mut active = false;
    for _ in 0..POLL_TRIES {
        if !paid && let Some(hold) = invoice_asked(&alice, &id).await {
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
    ActiveTrade {
        alice,
        bob,
        id,
        invoice,
    }
}

/// Finishes an active trade: Bob sends the fiat and says so, Alice sees it
/// arrive and releases, and the order reaches `success`.
async fn until_success(
    alice: &RunningService<RoleClient, ()>,
    bob: &RunningService<RoleClient, ()>,
    id: &str,
) {
    ok(bob, "fiat_sent", json!({"order_id": id})).await;
    poll("the fiat-sent status", || async {
        (status(alice, id).await.as_deref() == Some("fiat-sent")).then_some(())
    })
    .await;
    ok(
        alice,
        "release",
        json!({"order_id": id, "fiat_received": true}),
    )
    .await;
    poll("the trade to succeed", || async {
        (status(alice, id).await.as_deref() == Some("success")).then_some(())
    })
    .await;
}

/// Calls watch_order with the cursor of each previous call until a result
/// satisfies `done`, and returns that result.
async fn watch_until(
    client: &RunningService<RoleClient, ()>,
    id: &str,
    done: impl Fn(&Value) -> bool,
) -> Value {
    let mut after: Option<String> = None;
    for _ in 0..WATCH_CALLS {
        let mut args = json!({"order_id": id, "wait_seconds": 10});
        if let Some(cursor) = &after {
            args["after"] = json!(cursor);
        }
        let found = ok(client, "watch_order", args).await;
        if done(&found) {
            return found;
        }
        after = found["data"]["cursor"].as_str().map(String::from);
        assert!(
            after.is_some(),
            "every watch result carries a cursor: {found}"
        );
    }
    panic!("watch_order never returned what was expected for {id}");
}

/// Whether `who`'s wallet holds `bolt11` as a settled invoice.
fn settled(who: &str, bolt11: &str) -> bool {
    let invoices: Value = serde_json::from_str(&devenv(&["lncli", who, "listinvoices"])).unwrap();
    invoices["invoices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["payment_request"] == bolt11 && i["state"] == "SETTLED")
}

/// Reads up to four minutes: a slash waits for the waiting state to time out
/// (60 s with bonds on) and for the node's next check (every minute).
const SLASH_POLL_TRIES: u32 = 80;

#[tokio::test]
#[ignore = "needs the regtest network with bonds (MOSTRO_BONDS=1 regtest/devenv.sh up)"]
async fn a_taker_who_never_sends_an_invoice_loses_the_bond_and_the_seller_claims_a_share() {
    assert!(
        bonds_enabled(),
        "the node takes no bonds: start it with MOSTRO_BONDS=1 regtest/devenv.sh up, \
         or leave this test out with --skip loses_the_bond"
    );
    let alice = connect("alice", true).await;
    let bob = connect("bob", true).await;
    let created = ok(
        &alice,
        "create_order",
        json!({"kind": "sell", "fiat_code": "USD", "fiat_amount": 15,
               "payment_method": "bank transfer", "amount_sats": 15000}),
    )
    .await;
    let id = created["data"]["order"]["id"].as_str().unwrap().to_string();

    // Bob takes it without an invoice, pays the bond, and never sends one.
    let taken = ok(
        &bob,
        "take_order",
        json!({"order_id": id, "order_kind": "sell"}),
    )
    .await;
    assert_eq!(taken["data"]["next_step"], "pay_bond_invoice", "{taken}");
    devenv(&[
        "pay",
        "bob",
        taken["data"]["payment_request"].as_str().unwrap(),
    ]);
    poll("the node to ask Bob for his invoice", || async {
        asked(&bob, &id, "add_invoice").await.map(|_| ())
    })
    .await;

    // The wait times out: Bob's bond is slashed and Alice, the other party, is
    // asked for an invoice for her share of it.
    let mut share = None;
    for _ in 0..SLASH_POLL_TRIES {
        if let Some(message) = asked(&alice, &id, "add_bond_invoice").await {
            share = message["payload"]["order"]["sats_amount"]
                .as_str()
                .map(String::from);
            break;
        }
        tokio::time::sleep(POLL_EVERY).await;
    }
    let share = share.expect("Alice is asked for her share of the slashed bond");
    let invoice = devenv(&["invoice", "alice", &share]);
    let claimed = ok(
        &alice,
        "add_bond_invoice",
        json!({"order_id": id, "invoice": invoice}),
    )
    .await;
    assert_eq!(claimed["data"]["result"], "invoice_accepted", "{claimed}");

    // The node pays the share into Alice's wallet.
    poll("Alice's share to be paid", || async {
        settled("alice", &invoice).then_some(())
    })
    .await;

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

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn the_buyer_disputes_an_active_trade_and_a_pending_order_is_refused() {
    // A pending order cannot be disputed: the node refuses it.
    let alice = connect("alice", true).await;
    let created = ok(
        &alice,
        "create_order",
        json!({"kind": "sell", "fiat_code": "USD", "fiat_amount": 5,
               "payment_method": "bank transfer", "amount_sats": 5000}),
    )
    .await;
    let pending = created["data"]["order"]["id"].as_str().unwrap().to_string();
    let (refused, error) = call(&alice, "open_dispute", json!({"order_id": pending})).await;
    assert!(error, "{refused}");
    assert_eq!(refused["error"]["code"], "node_refused", "{refused}");
    assert_eq!(
        refused["error"]["reason"], "not_allowed_by_status",
        "{refused}"
    );
    ok(&alice, "cancel", json!({"order_id": pending})).await;
    alice.cancel().await.unwrap();

    // Bob disputes the active trade; both sides see the order in dispute.
    let ActiveTrade { alice, bob, id, .. } = until_active(Maker::Seller, true).await;
    let opened = ok(&bob, "open_dispute", json!({"order_id": id})).await;
    assert_eq!(opened["data"]["order_id"], id.as_str(), "{opened}");
    assert!(opened["data"]["dispute_id"].is_string(), "{opened}");
    for client in [&bob, &alice] {
        poll("the dispute status", || async {
            (status(client, &id).await.as_deref() == Some("dispute")).then_some(())
        })
        .await;
    }

    // A second dispute is answered with silence: uncertain, never an ok.
    let (again, error) = call(&bob, "open_dispute", json!({"order_id": id})).await;
    assert!(error, "{again}");
    assert_eq!(again["status"], "uncertain", "{again}");
    assert_eq!(again["error"]["code"], "reply_timeout", "{again}");

    alice.cancel().await.unwrap();
    bob.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn both_sides_rate_a_finished_trade_once_and_a_running_one_is_refused() {
    // An order never taken has no counterparty: the node cannot place the
    // rating and does not answer, so it is uncertain, not a refusal.
    let alice = connect("alice", true).await;
    let created = ok(
        &alice,
        "create_order",
        json!({"kind": "sell", "fiat_code": "USD", "fiat_amount": 5,
               "payment_method": "bank transfer", "amount_sats": 5000}),
    )
    .await;
    let untaken = created["data"]["order"]["id"].as_str().unwrap().to_string();
    let (silent, error) = call(
        &alice,
        "rate_counterparty",
        json!({"order_id": untaken, "rating": 5}),
    )
    .await;
    assert!(error, "{silent}");
    assert_eq!(silent["status"], "uncertain", "{silent}");
    assert_eq!(silent["error"]["code"], "reply_timeout", "{silent}");
    ok(&alice, "cancel", json!({"order_id": untaken})).await;
    alice.cancel().await.unwrap();

    let ActiveTrade { alice, bob, id, .. } = until_active(Maker::Seller, true).await;

    // A taken order in another status is refused: an error, nothing recorded.
    let (refused, error) = call(
        &bob,
        "rate_counterparty",
        json!({"order_id": id, "rating": 5}),
    )
    .await;
    assert!(error, "{refused}");
    assert_eq!(refused["error"]["code"], "node_refused", "{refused}");
    assert_eq!(
        refused["error"]["reason"], "invalid_order_status",
        "{refused}"
    );

    // The trade finishes; each side rates from the order's status alone,
    // without waiting for a `rate` message. Bob only rates once his watch has
    // ended, whether or not a `rate` came in it.
    until_success(&alice, &bob, &id).await;
    watch_until(&bob, &id, |found| found["data"]["ended"] == true).await;

    // Alice, the seller, and Bob, the buyer, each rate once; the node echoes
    // the rating each sent.
    for (client, rating) in [(&alice, 5), (&bob, 4)] {
        let rated = ok(
            client,
            "rate_counterparty",
            json!({"order_id": id, "rating": rating}),
        )
        .await;
        assert_eq!(rated["command"], "order.rate", "{rated}");
        assert_eq!(rated["data"]["order_id"], id.as_str(), "{rated}");
        assert_eq!(rated["data"]["rating"], rating, "{rated}");
    }

    // Test only, against the pinned mostrod v0.20.2: a second rating from the
    // same side gets no answer, so uncertain and never ok. An agent never sends
    // one: an uncertain rating is never resent, and older nodes may not guard
    // against the duplicate.
    let (again, error) = call(
        &alice,
        "rate_counterparty",
        json!({"order_id": id, "rating": 1}),
    )
    .await;
    assert!(error, "{again}");
    assert_eq!(again["status"], "uncertain", "{again}");
    assert_eq!(again["error"]["code"], "reply_timeout", "{again}");

    // The node's acknowledgement is also among Alice's messages.
    poll("the rate-received message", || async {
        let (document, _) = call(&alice, "list_messages", json!({"order_id": id})).await;
        document["data"]["messages"]
            .as_array()?
            .iter()
            .any(|m| m["action"] == "rate-received")
            .then_some(())
    })
    .await;

    alice.cancel().await.unwrap();
    bob.cancel().await.unwrap();
}

/// The node's own key, which the regtest node also accepts as its admin: the
/// public `mostro-regtest` test key of `regtest/mostrod/settings.toml`.
fn node_admin_key() -> String {
    let settings = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/regtest/mostrod/settings.toml"
    ))
    .expect("read the regtest node settings");
    settings
        .lines()
        .find_map(|line| line.trim().strip_prefix("nsec_privkey = "))
        .map(|value| value.trim_matches('\'').to_string())
        .expect("the node's nsec_privkey")
}

/// Runs mostro-cli's admin command `args` (`devenv.sh cli alice …` with
/// `ADMIN_NSEC`), as a solver of the node would.
fn admin(args: &[&str]) {
    let output = std::process::Command::new(DEVENV)
        .arg("cli")
        .arg("alice")
        .args(args)
        .env("ADMIN_NSEC", node_admin_key())
        .output()
        .expect("run regtest/devenv.sh cli");
    assert!(
        output.status.success(),
        "admin {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[ignore = "needs the regtest network (regtest/devenv.sh up)"]
async fn the_seller_rates_after_a_solver_settles_the_dispute() {
    let ActiveTrade { alice, bob, id, .. } = until_active(Maker::Seller, true).await;
    let opened = ok(&bob, "open_dispute", json!({"order_id": id})).await;
    let dispute = opened["data"]["dispute_id"].as_str().unwrap().to_string();

    // A solver takes the dispute and settles it for the buyer.
    admin(&["admtakedispute", "-d", &dispute]);
    admin(&["admsettle", "-o", &id]);

    // The seller's order reaches a status the node accepts her rating in.
    poll("the settled status", || async {
        matches!(
            status(&alice, &id).await.as_deref(),
            Some("settled-hold-invoice" | "success")
        )
        .then_some(())
    })
    .await;
    // She is told `admin-settled`, and no `rate` asks her to rate.
    let actions: Vec<String> = poll("the admin-settled message", || async {
        let (document, _) = call(&alice, "list_messages", json!({"order_id": id})).await;
        let actions: Vec<String> = document["data"]["messages"]
            .as_array()?
            .iter()
            .filter_map(|m| m["action"].as_str().map(String::from))
            .collect();
        actions
            .iter()
            .any(|a| a == "admin-settled")
            .then_some(actions)
    })
    .await;
    assert!(!actions.iter().any(|a| a == "rate"), "{actions:?}");

    // Her rating, the score the user chose, is still accepted.
    let rated = ok(
        &alice,
        "rate_counterparty",
        json!({"order_id": id, "rating": 4}),
    )
    .await;
    assert_eq!(rated["status"], "ok", "{rated}");
    assert_eq!(rated["data"]["order_id"], id.as_str(), "{rated}");
    assert_eq!(rated["data"]["rating"], 4, "{rated}");

    alice.cancel().await.unwrap();
    bob.cancel().await.unwrap();
}
