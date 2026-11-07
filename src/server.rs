//! The MCP server: the Mostro tools over a [`CliRunner`].

use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};

use crate::cli_backend::CliRunner;
use crate::error::{Error, Result as MostroResult};
use crate::outcome::{Outcome, UNCERTAIN_WARNING, excerpt};
use crate::tools::{
    add_invoice::{self, AddInvoiceParams},
    create_order::{self, CreateOrderParams},
    get_order::{self, GetOrderParams},
    list_messages::{self, ListMessagesParams},
    list_my_orders,
    list_orders::{self, ListOrdersParams},
    sync_orders::{self, SyncOrdersParams},
    take_order::{self, TakeOrderParams},
    trade_step::{self, OrderStepParams, ReleaseParams},
};

const INSTRUCTIONS: &str = "\
Mostro is a peer-to-peer Bitcoin/Lightning exchange over Nostr. These tools act \
on one Mostro node with this server's identity (managed by mostro-cli).

Sides: an order is named by its maker. A `sell` order sells sats: taking it \
means you BUY sats and pay fiat. A `buy` order buys sats: taking it means you \
SELL sats, and you will be asked to pay a Lightning hold invoice.

Reading changes nothing: list_orders (the public book), get_order (the node's \
record of an order of yours), list_my_orders (the orders this identity holds \
locally, no network) and list_messages (what the node asked you: the next \
step, with any invoice to pay as data).

Every other tool is STATE-CHANGING and commits the user: confirm the exact \
order, side and amounts with the user before calling it. This server refuses \
them unless it was started with MOSTRO_MCP_ALLOW_WRITES=true. A trade runs: \
create_order or take_order; then, as list_messages asks, add_invoice (the \
buyer's invoice for the sats) or a payment by the user; fiat_sent (the buyer, \
once the fiat went out); release (the seller, only once the user confirms the \
fiat arrived; irreversible). cancel withdraws an order or a trade; \
add_bond_invoice claims a share of a slashed bond; sync_orders adopts orders \
the node created for this identity (the next order of a range).

These tools never pay an invoice. When the node asks for a payment (a hold \
invoice, a bond), give the invoice to the user to pay from their own wallet, \
then follow with list_messages.

Every result is the JSON outcome document of mostro-cli: `status` (`ok`, `error` or \
`uncertain`), `data` (with `next_step` where the node asks the user for something), \
`error` and `warnings`. An `uncertain` result means the request may have reached the \
node: never retry it, check the state first (get_order, list_my_orders, list_messages).";

pub struct MostroServer<R: CliRunner> {
    runner: Arc<R>,
    allow_writes: bool,
    tool_router: ToolRouter<Self>,
}

impl<R: CliRunner> Clone for MostroServer<R> {
    fn clone(&self) -> Self {
        Self {
            runner: Arc::clone(&self.runner),
            allow_writes: self.allow_writes,
            tool_router: self.tool_router.clone(),
        }
    }
}

#[tool_router]
impl<R: CliRunner> MostroServer<R> {
    pub fn new(runner: R, allow_writes: bool) -> Self {
        Self {
            runner: Arc::new(runner),
            allow_writes,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "List orders in the Mostro node's public order book. Filters: \
            kind (`sell`: maker sells sats, you would buy; `buy`: maker buys sats, \
            you would sell), currency (fiat code), status (default `pending`, the \
            takeable orders).",
        annotations(title = "List orders", read_only_hint = true, open_world_hint = true)
    )]
    async fn list_orders(
        &self,
        Parameters(params): Parameters<ListOrdersParams>,
    ) -> CallToolResult {
        self.execute(list_orders::args(&params)).await
    }

    #[tool(
        description = "Get the node's record of an order this identity made or took, by \
            its UUID. The node does not describe other people's orders: use \
            list_orders for those.",
        annotations(title = "Get order", read_only_hint = true, open_world_hint = true)
    )]
    async fn get_order(&self, Parameters(params): Parameters<GetOrderParams>) -> CallToolResult {
        self.execute(get_order::args(&params)).await
    }

    #[tool(
        description = "Publish a new order. STATE-CHANGING: confirm with the user first. \
            kind `sell` = you sell sats and receive fiat; `buy` = you buy sats and pay \
            fiat. Give fiat_amount, or fiat_amount_min and fiat_amount_max for a range. \
            amount_sats omitted = market price (premium applies). Refused unless the \
            server runs with MOSTRO_MCP_ALLOW_WRITES=true.",
        annotations(
            title = "Create order",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn create_order(
        &self,
        Parameters(params): Parameters<CreateOrderParams>,
    ) -> CallToolResult {
        self.execute_write(|| create_order::args(&params)).await
    }

    #[tool(
        description = "Take an order from the book. STATE-CHANGING: confirm with the user \
            first. order_kind must be the order's side as listed: `sell` = you BUY \
            sats (you may pass your invoice); `buy` = you SELL sats (you will pay a \
            hold invoice). fiat_amount only for range orders. Refused unless the \
            server runs with MOSTRO_MCP_ALLOW_WRITES=true. If it reports an error, \
            do NOT retry blindly: the node may have accepted the take (mostro-cli \
            can fail after the node answered). Check list_my_orders, get_order or \
            list_messages first. Next: list_messages says what the node asks.",
        annotations(
            title = "Take order",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn take_order(&self, Parameters(params): Parameters<TakeOrderParams>) -> CallToolResult {
        self.execute_write(|| take_order::args(&params)).await
    }

    #[tool(
        description = "List the orders this identity holds in mostro-cli's local store \
            (made, taken or adopted), newest first, with their local status. Offline: \
            reads no relay. Use it to find your order ids, and to check what happened \
            after an `uncertain` create_order or take_order.",
        annotations(
            title = "List my orders",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_my_orders(&self) -> CallToolResult {
        self.execute(Ok(list_my_orders::args())).await
    }

    #[tool(
        description = "Read what the node said about this identity's trades, newest \
            first: each message's action and `next_step` (pay an invoice, add an \
            invoice, send the fiat, wait), with any invoice to pay as data. Filter by \
            order_id; since_minutes reads further back (default 30, at most 10080). \
            Call it after every state-changing tool to learn the next step.",
        annotations(title = "List messages", read_only_hint = true, open_world_hint = true)
    )]
    async fn list_messages(
        &self,
        Parameters(params): Parameters<ListMessagesParams>,
    ) -> CallToolResult {
        self.execute(list_messages::args(&params)).await
    }

    #[tool(
        description = "Send the invoice the node asked for (`next_step` `add_invoice`): \
            the buyer's BOLT11 invoice or Lightning address for the sats. \
            STATE-CHANGING: confirm with the user first. Refused unless the server runs \
            with MOSTRO_MCP_ALLOW_WRITES=true. Then call list_messages.",
        annotations(
            title = "Add invoice",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn add_invoice(
        &self,
        Parameters(params): Parameters<AddInvoiceParams>,
    ) -> CallToolResult {
        self.execute_write(|| add_invoice::args(&params)).await
    }

    #[tool(
        description = "Claim your share of a slashed bond (`next_step` \
            `add_bond_invoice`): a BOLT11 invoice for exactly the share list_messages \
            reports. STATE-CHANGING: confirm with the user first. Refused unless the \
            server runs with MOSTRO_MCP_ALLOW_WRITES=true.",
        annotations(
            title = "Add bond invoice",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn add_bond_invoice(
        &self,
        Parameters(params): Parameters<AddInvoiceParams>,
    ) -> CallToolResult {
        self.execute_write(|| add_invoice::bond_args(&params)).await
    }

    #[tool(
        description = "Tell the seller that the fiat was sent (the buyer's step). \
            STATE-CHANGING: only once the user confirms they sent the fiat. Refused \
            unless the server runs with MOSTRO_MCP_ALLOW_WRITES=true. Then call \
            list_messages.",
        annotations(
            title = "Fiat sent",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn fiat_sent(&self, Parameters(params): Parameters<OrderStepParams>) -> CallToolResult {
        self.execute_write(|| trade_step::fiat_sent_args(&params))
            .await
    }

    #[tool(
        description = "Release the sats to the buyer (the seller's step). IRREVERSIBLE \
            and STATE-CHANGING: only once the user confirms the fiat arrived in their \
            account, which fiat_received=true states (set it only after the user said so \
            in this conversation); anything else is refused. \
            Refused unless the server runs with MOSTRO_MCP_ALLOW_WRITES=true. On \
            `uncertain`, never retry: check get_order and list_messages.",
        annotations(
            title = "Release",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn release(&self, Parameters(params): Parameters<ReleaseParams>) -> CallToolResult {
        self.execute_write(|| trade_step::release_args(&params))
            .await
    }

    #[tool(
        description = "Cancel an order: a pending order of yours is withdrawn; in a \
            running trade it asks the counterparty for a cooperative cancel. \
            STATE-CHANGING: confirm with the user first. Refused unless the server runs \
            with MOSTRO_MCP_ALLOW_WRITES=true. Then call list_messages.",
        annotations(
            title = "Cancel",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn cancel(&self, Parameters(params): Parameters<OrderStepParams>) -> CallToolResult {
        self.execute_write(|| trade_step::cancel_args(&params))
            .await
    }

    #[tool(
        description = "Adopt the orders the node created for this identity that the \
            local store lacks, such as the next order of a range after a release. \
            Saves them in mostro-cli's store (STATE-CHANGING locally; nothing is sent \
            to the node). since_minutes: how far back, default and maximum 10080. \
            Refused unless the server runs with MOSTRO_MCP_ALLOW_WRITES=true.",
        annotations(
            title = "Sync orders",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn sync_orders(
        &self,
        Parameters(params): Parameters<SyncOrdersParams>,
    ) -> CallToolResult {
        self.execute_local_write(|| sync_orders::args(&params))
            .await
    }
}

impl<R: CliRunner> MostroServer<R> {
    /// Runs a read-only tool's command.
    async fn execute(&self, args: MostroResult<Vec<String>>) -> CallToolResult {
        self.run_tool(args, false).await
    }

    /// Runs the CLI with `args` when they were built; every failure becomes a
    /// tool error the agent can read. A command that sends to the node and
    /// timed out, or whose output cannot be read, did run, so it may have
    /// reached the node: that error starts with the same no-retry warning as an
    /// `uncertain` outcome.
    async fn run_tool(
        &self,
        args: MostroResult<Vec<String>>,
        may_reach_node: bool,
    ) -> CallToolResult {
        let args = match args {
            Ok(args) => args,
            Err(error) => return tool_error(&error),
        };
        let command = args.first().cloned().unwrap_or_default();
        match self.runner.run(args).await {
            Ok(run) => match Outcome::parse(&run.stdout, run.code) {
                Ok(outcome) => {
                    tracing::info!(%command, status = ?outcome.status(), "mostro-cli finished");
                    outcome.into_tool_result()
                }
                Err(reason) => {
                    tracing::warn!(%command, %reason, "mostro-cli output not usable");
                    let status = run
                        .code
                        .map_or("it was stopped by a signal".to_string(), |code| {
                            format!("exit status: {code}")
                        });
                    let text = error_text(&Error::CliFailed {
                        status: format!("{status}; {reason}"),
                        stderr: run.stderr,
                        stdout: excerpt(&run.stdout),
                    });
                    if may_reach_node {
                        warned_error(&text)
                    } else {
                        CallToolResult::error(vec![ContentBlock::text(text)])
                    }
                }
            },
            Err(error) => {
                tracing::warn!(%command, %error, "mostro-cli failed");
                if may_reach_node && matches!(error, Error::Timeout(_)) {
                    return warned_error(&error_text(&error));
                }
                tool_error(&error)
            }
        }
    }

    /// The write gate: checked before the arguments are even built, so a
    /// disabled server never spawns the CLI for a state-changing tool.
    async fn execute_write(
        &self,
        build: impl FnOnce() -> MostroResult<Vec<String>>,
    ) -> CallToolResult {
        self.gated(build, true).await
    }

    /// The write gate for a tool that only writes the CLI's local store
    /// (`sync_orders`): nothing reaches the node, so a failure is not warned.
    async fn execute_local_write(
        &self,
        build: impl FnOnce() -> MostroResult<Vec<String>>,
    ) -> CallToolResult {
        self.gated(build, false).await
    }

    async fn gated(
        &self,
        build: impl FnOnce() -> MostroResult<Vec<String>>,
        may_reach_node: bool,
    ) -> CallToolResult {
        if !self.allow_writes {
            return tool_error(&Error::WritesDisabled);
        }
        self.run_tool(build(), may_reach_node).await
    }
}

/// A tool error that starts with the no-retry warning.
fn warned_error(text: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format!(
        "{UNCERTAIN_WARNING}\n\n{text}"
    ))])
}

fn tool_error(error: &Error) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(error_text(error))])
}

fn error_text(error: &Error) -> String {
    match error {
        Error::CliFailed { stdout, .. } if !stdout.trim().is_empty() => {
            format!("{error}\n\nmostro-cli output:\n{stdout}")
        }
        _ => error.to_string(),
    }
}

#[tool_handler]
impl<R: CliRunner> ServerHandler for MostroServer<R> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("mostro-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::cli_backend::CliRun;
    use crate::tools::{
        OrderKind,
        add_invoice::AddInvoiceParams,
        list_messages::ListMessagesParams,
        sync_orders::SyncOrdersParams,
        trade_step::{OrderStepParams, ReleaseParams},
    };

    /// Records every argument vector and answers with a canned result.
    struct FakeCli {
        calls: Mutex<Vec<Vec<String>>>,
        reply: fn() -> MostroResult<CliRun>,
    }

    impl FakeCli {
        fn answering(reply: fn() -> MostroResult<CliRun>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                reply,
            }
        }
    }

    impl CliRunner for Arc<FakeCli> {
        async fn run(&self, args: Vec<String>) -> MostroResult<CliRun> {
            self.calls.lock().unwrap().push(args);
            (self.reply)()
        }
    }

    const OUTCOMES: &str = include_str!("../tests/fixtures/outcomes.json");

    /// A fixture of the CLI's own documents as a finished run.
    fn run_of(name: &str) -> MostroResult<CliRun> {
        let all: serde_json::Value = serde_json::from_str(OUTCOMES).unwrap();
        Ok(CliRun {
            stdout: all[name]["stdout"].to_string(),
            stderr: String::new(),
            code: Some(i32::try_from(all[name]["exit_code"].as_i64().unwrap()).unwrap()),
        })
    }

    fn ok_output() -> MostroResult<CliRun> {
        run_of("list_empty")
    }

    fn server(allow_writes: bool) -> (MostroServer<Arc<FakeCli>>, Arc<FakeCli>) {
        let cli = Arc::new(FakeCli::answering(ok_output));
        (MostroServer::new(Arc::clone(&cli), allow_writes), cli)
    }

    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect()
    }

    fn sell_order() -> CreateOrderParams {
        CreateOrderParams {
            kind: OrderKind::Sell,
            fiat_code: "USD".into(),
            fiat_amount: Some(10),
            fiat_amount_min: None,
            fiat_amount_max: None,
            payment_method: "bank transfer".into(),
            amount_sats: Some(10_000),
            premium: None,
            invoice: None,
            expiration_days: None,
        }
    }

    #[test]
    fn twelve_tools_with_honest_read_only_hints() {
        let (server, _) = server(false);
        let mut tools: Vec<_> = server
            .tool_router
            .list_all()
            .into_iter()
            .map(|t| {
                let read_only = t.annotations.and_then(|a| a.read_only_hint);
                (t.name.to_string(), read_only)
            })
            .collect();
        tools.sort();
        assert_eq!(
            tools,
            [
                ("add_bond_invoice".to_string(), Some(false)),
                ("add_invoice".to_string(), Some(false)),
                ("cancel".to_string(), Some(false)),
                ("create_order".to_string(), Some(false)),
                ("fiat_sent".to_string(), Some(false)),
                ("get_order".to_string(), Some(true)),
                ("list_messages".to_string(), Some(true)),
                ("list_my_orders".to_string(), Some(true)),
                ("list_orders".to_string(), Some(true)),
                ("release".to_string(), Some(false)),
                ("sync_orders".to_string(), Some(false)),
                ("take_order".to_string(), Some(false)),
            ]
        );
    }

    const ID: &str = "ffdc768a-37d8-429e-bfcc-0968be56b6f9";

    fn step() -> OrderStepParams {
        OrderStepParams {
            order_id: ID.into(),
        }
    }

    fn invoice() -> AddInvoiceParams {
        AddInvoiceParams {
            order_id: ID.into(),
            invoice: "lnbcrt1pexample".into(),
        }
    }

    #[tokio::test]
    async fn every_trade_step_is_refused_by_default_without_spawning_the_cli() {
        let (server, cli) = server(false);
        let release = ReleaseParams {
            order_id: ID.into(),
            fiat_received: true,
        };
        for result in [
            server.add_invoice(Parameters(invoice())).await,
            server.add_bond_invoice(Parameters(invoice())).await,
            server.fiat_sent(Parameters(step())).await,
            server.release(Parameters(release)).await,
            server.cancel(Parameters(step())).await,
            server
                .sync_orders(Parameters(SyncOrdersParams::default()))
                .await,
        ] {
            assert_eq!(result.is_error, Some(true));
            assert!(text_of(&result).contains("MOSTRO_MCP_ALLOW_WRITES=true"));
        }
        assert!(cli.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn release_without_fiat_received_never_spawns_the_cli() {
        let (server, cli) = server(true);
        let result = server
            .release(Parameters(ReleaseParams {
                order_id: ID.into(),
                fiat_received: false,
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("fiat_received"));
        assert!(cli.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_new_tools_run_their_commands() {
        let (server, cli) = server(true);
        server.list_my_orders().await;
        server
            .list_messages(Parameters(ListMessagesParams::default()))
            .await;
        server.add_invoice(Parameters(invoice())).await;
        server.add_bond_invoice(Parameters(invoice())).await;
        server.fiat_sent(Parameters(step())).await;
        server
            .release(Parameters(ReleaseParams {
                order_id: ID.into(),
                fiat_received: true,
            }))
            .await;
        server.cancel(Parameters(step())).await;
        server
            .sync_orders(Parameters(SyncOrdersParams::default()))
            .await;
        let commands: Vec<String> = cli
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|args| args[..2].join(" "))
            .collect();
        assert_eq!(
            commands,
            [
                "order mine",
                "message list",
                "order add-invoice",
                "bond add-invoice",
                "order fiat-sent",
                "order release",
                "order cancel",
                "order sync"
            ]
        );
    }

    #[tokio::test]
    async fn list_orders_returns_the_cli_output() {
        let (server, cli) = server(false);
        let result = server
            .list_orders(Parameters(ListOrdersParams {
                kind: Some(OrderKind::Sell),
                ..Default::default()
            }))
            .await;
        assert_ne!(result.is_error, Some(true));
        let document = result
            .structured_content
            .clone()
            .expect("structured content");
        assert_eq!(document["command"], "order.list");
        assert_eq!(document["status"], "ok");
        assert!(text_of(&result).contains("\"order.list\""));
        assert_eq!(
            *cli.calls.lock().unwrap(),
            [["order", "list", "--side=sell"]]
        );
    }

    #[tokio::test]
    async fn writes_are_refused_by_default_without_spawning_the_cli() {
        let (server, cli) = server(false);
        let created = server.create_order(Parameters(sell_order())).await;
        let taken = server
            .take_order(Parameters(TakeOrderParams {
                order_id: "ffdc768a-37d8-429e-bfcc-0968be56b6f9".into(),
                order_kind: OrderKind::Sell,
                fiat_amount: None,
                invoice: None,
            }))
            .await;
        for result in [created, taken] {
            assert_eq!(result.is_error, Some(true));
            assert!(text_of(&result).contains("MOSTRO_MCP_ALLOW_WRITES=true"));
        }
        assert!(cli.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_enabled_server_runs_create_order() {
        let (server, cli) = server(true);
        let result = server.create_order(Parameters(sell_order())).await;
        assert_ne!(result.is_error, Some(true));
        assert_eq!(cli.calls.lock().unwrap()[0][..2], ["order", "new"]);
    }

    #[tokio::test]
    async fn invalid_arguments_never_reach_the_cli() {
        let (server, cli) = server(true);
        let result = server
            .get_order(Parameters(GetOrderParams {
                order_id: "nope".into(),
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("order_id"));
        assert!(cli.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_error_outcome_is_a_tool_error_with_the_nodes_reason() {
        let cli = Arc::new(FakeCli::answering(|| run_of("release_node_refused")));
        let server = MostroServer::new(Arc::clone(&cli), false);
        let result = server
            .list_orders(Parameters(ListOrdersParams::default()))
            .await;
        assert_eq!(result.is_error, Some(true));
        let body = text_of(&result);
        assert!(
            body.contains("node_refused") && body.contains("invalid_peer"),
            "{body}"
        );
    }

    #[tokio::test]
    async fn an_uncertain_outcome_tells_the_agent_not_to_retry() {
        let cli = Arc::new(FakeCli::answering(|| run_of("take_uncertain")));
        let server = MostroServer::new(Arc::clone(&cli), true);
        let result = server.create_order(Parameters(sell_order())).await;
        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).starts_with(crate::outcome::UNCERTAIN_WARNING));
    }

    #[tokio::test]
    async fn output_that_is_not_an_outcome_is_a_readable_tool_error_with_the_clis_stderr() {
        let cli = Arc::new(FakeCli::answering(|| {
            Ok(CliRun {
                stdout: "not a document".into(),
                stderr: "RELAYS not set".into(),
                code: Some(1),
            })
        }));
        let server = MostroServer::new(Arc::clone(&cli), false);
        let result = server
            .list_orders(Parameters(ListOrdersParams::default()))
            .await;
        assert_eq!(result.is_error, Some(true));
        let body = text_of(&result);
        assert!(
            body.contains("RELAYS not set") && body.contains("not one JSON"),
            "{body}"
        );
        assert!(body.contains("not a document"), "the excerpt: {body}");
    }

    #[tokio::test]
    async fn unreadable_output_of_a_write_is_warned_like_an_uncertain_one() {
        let cli = Arc::new(FakeCli::answering(|| {
            Ok(CliRun {
                stdout: "not a document".into(),
                stderr: String::new(),
                code: Some(1),
            })
        }));
        let server = MostroServer::new(Arc::clone(&cli), true);
        let created = server.create_order(Parameters(sell_order())).await;
        assert_eq!(created.is_error, Some(true));
        let body = text_of(&created);
        assert!(
            body.starts_with(crate::outcome::UNCERTAIN_WARNING),
            "{body}"
        );
        assert!(body.contains("not one JSON"), "{body}");
        // A read changes nothing on the node: no warning.
        let listed = server
            .list_orders(Parameters(ListOrdersParams::default()))
            .await;
        assert!(!text_of(&listed).starts_with(crate::outcome::UNCERTAIN_WARNING));
    }

    #[tokio::test]
    async fn a_timed_out_write_is_warned_and_a_timed_out_read_is_not() {
        let cli = Arc::new(FakeCli::answering(|| {
            Err(Error::Timeout(std::time::Duration::from_secs(60)))
        }));
        let server = MostroServer::new(Arc::clone(&cli), true);
        let sent = server.fiat_sent(Parameters(step())).await;
        assert_eq!(sent.is_error, Some(true));
        let body = text_of(&sent);
        assert!(
            body.starts_with(crate::outcome::UNCERTAIN_WARNING),
            "{body}"
        );
        assert!(body.contains("list_my_orders"), "{body}");
        let listed = server
            .list_orders(Parameters(ListOrdersParams::default()))
            .await;
        assert!(!text_of(&listed).starts_with(crate::outcome::UNCERTAIN_WARNING));
    }

    #[tokio::test]
    async fn sync_orders_sends_nothing_to_the_node_so_it_is_not_warned() {
        let cli = Arc::new(FakeCli::answering(|| {
            Ok(CliRun {
                stdout: "not a document".into(),
                stderr: String::new(),
                code: Some(1),
            })
        }));
        let server = MostroServer::new(Arc::clone(&cli), true);
        let synced = server
            .sync_orders(Parameters(SyncOrdersParams::default()))
            .await;
        assert_eq!(synced.is_error, Some(true));
        assert!(!text_of(&synced).starts_with(crate::outcome::UNCERTAIN_WARNING));
    }

    #[tokio::test]
    async fn a_document_that_disagrees_with_the_exit_status_is_not_believed() {
        let cli = Arc::new(FakeCli::answering(|| {
            let mut run = run_of("list_empty")?;
            run.code = Some(1);
            Ok(run)
        }));
        let server = MostroServer::new(Arc::clone(&cli), false);
        let result = server
            .list_orders(Parameters(ListOrdersParams::default()))
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("exit status"));
    }

    #[tokio::test]
    async fn a_runner_failure_is_a_readable_tool_error() {
        let cli = Arc::new(FakeCli::answering(|| {
            Err(Error::CliNotFound("/x/mostro-cli".into()))
        }));
        let server = MostroServer::new(Arc::clone(&cli), false);
        let result = server
            .list_orders(Parameters(ListOrdersParams::default()))
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("MOSTRO_CLI_PATH"));
    }
}
