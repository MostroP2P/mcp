//! The MCP server: four tools over a [`CliRunner`].

use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};

use crate::cli_backend::CliRunner;
use crate::error::{Error, Result as MostroResult};
use crate::tools::{
    create_order::{self, CreateOrderParams},
    get_order::{self, GetOrderParams},
    list_orders::{self, ListOrdersParams},
    take_order::{self, TakeOrderParams},
};

const INSTRUCTIONS: &str = "\
Mostro is a peer-to-peer Bitcoin/Lightning exchange over Nostr. These tools act \
on one Mostro node with this server's identity (managed by mostro-cli).

Sides: an order is named by its maker. A `sell` order sells sats: taking it \
means you BUY sats and pay fiat. A `buy` order buys sats: taking it means you \
SELL sats, and you will be asked to pay a Lightning hold invoice.

list_orders and get_order only read. create_order and take_order commit the \
user to a trade: confirm the exact order, side and amounts with the user \
before calling them. This server refuses them unless it was started with \
MOSTRO_MCP_ALLOW_WRITES=true.

Results are mostro-cli's text output.";

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
            can fail after the node answered). Check get_order or list_orders first.",
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
}

impl<R: CliRunner> MostroServer<R> {
    /// Runs the CLI with `args` when they were built; every failure becomes a
    /// tool error the agent can read.
    async fn execute(&self, args: MostroResult<Vec<String>>) -> CallToolResult {
        let args = match args {
            Ok(args) => args,
            Err(error) => return tool_error(&error),
        };
        let command = args.first().cloned().unwrap_or_default();
        match self.runner.run(args).await {
            Ok(output) => {
                tracing::info!(%command, "mostro-cli finished");
                let text = if output.stdout.trim().is_empty() {
                    "mostro-cli finished without output.".to_string()
                } else {
                    output.stdout
                };
                CallToolResult::success(vec![ContentBlock::text(text)])
            }
            Err(error) => {
                tracing::warn!(%command, %error, "mostro-cli failed");
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
        if !self.allow_writes {
            return tool_error(&Error::WritesDisabled);
        }
        self.execute(build()).await
    }
}

fn tool_error(error: &Error) -> CallToolResult {
    let text = match error {
        Error::CliFailed { stdout, .. } if !stdout.trim().is_empty() => {
            format!("{error}\n\nmostro-cli output:\n{stdout}")
        }
        _ => error.to_string(),
    };
    CallToolResult::error(vec![ContentBlock::text(text)])
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
    use crate::cli_backend::CliOutput;
    use crate::tools::OrderKind;

    /// Records every argument vector and answers with a canned result.
    struct FakeCli {
        calls: Mutex<Vec<Vec<String>>>,
        reply: fn() -> MostroResult<CliOutput>,
    }

    impl FakeCli {
        fn answering(reply: fn() -> MostroResult<CliOutput>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                reply,
            }
        }
    }

    impl CliRunner for Arc<FakeCli> {
        async fn run(&self, args: Vec<String>) -> MostroResult<CliOutput> {
            self.calls.lock().unwrap().push(args);
            (self.reply)()
        }
    }

    fn ok_output() -> MostroResult<CliOutput> {
        Ok(CliOutput {
            stdout: "📋 List Orders\n(table)".into(),
            stderr: String::new(),
        })
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
    fn exactly_four_tools_with_honest_read_only_hints() {
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
                ("create_order".to_string(), Some(false)),
                ("get_order".to_string(), Some(true)),
                ("list_orders".to_string(), Some(true)),
                ("take_order".to_string(), Some(false)),
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
        assert!(text_of(&result).contains("List Orders"));
        assert_eq!(*cli.calls.lock().unwrap(), [["listorders", "--kind=sell"]]);
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
        assert_eq!(cli.calls.lock().unwrap()[0][0], "neworder");
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
    async fn a_cli_failure_is_a_readable_tool_error() {
        let cli = Arc::new(FakeCli::answering(|| {
            Err(Error::CliFailed {
                status: "exit status: 1".into(),
                stderr: "RELAYS not set".into(),
                stdout: String::new(),
            })
        }));
        let server = MostroServer::new(Arc::clone(&cli), false);
        let result = server
            .list_orders(Parameters(ListOrdersParams::default()))
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("RELAYS not set"));
    }
}
