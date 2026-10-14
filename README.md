# mostro-mcp

An [MCP](https://modelcontextprotocol.io) server that lets AI agents use
[Mostro](https://mostro.network), the peer-to-peer Bitcoin/Lightning exchange
over Nostr. It is a prototype: it drives `mostro-cli` as a subprocess, to prove
the architecture end to end before any refactor.

```text
MCP client / agent ──MCP (stdio)──▶ mostro-mcp ──subprocess──▶ mostro-cli ──Nostr──▶ mostrod
```

## Requirements

- Rust 1.88+ (built and tested with 1.99).
- [`mostro-cli`](https://github.com/MostroP2P/mostro-cli) at commit `7944e82` or
  later (0.16.3 plus the take fix, mostro-cli#205). `cargo build --release` in
  its checkout. An older CLI exits 1 after taking a sell order with an invoice
  and strands that trade on the client (mostro-cli#204).
- For the regtest network: Docker with Compose v2, `jq`.

## Build and test

```bash
cargo build --release          # target/release/mostro-mcp
cargo test                     # unit + stdio integration tests, no node needed
cargo clippy --all-targets -- -D warnings
```

## Configuration

All configuration comes from the environment. Keys and the mnemonic are not
configuration: they stay in mostro-cli's own database (`MCLI_DIR`), which this
server never reads.

| Variable | Required | Meaning |
|---|---|---|
| `MOSTRO_PUBKEY` | yes | The Mostro node, hex or npub |
| `RELAYS` | yes | Comma-separated `ws://` / `wss://` relays |
| `MOSTRO_CLI_PATH` | no | The CLI binary; default `mostro-cli` from `$PATH` |
| `MCLI_DIR` | no | The CLI's data dir = the identity used; default `~/.mcli` |
| `POW`, `TRANSPORT` | no | Passed to the CLI as-is |
| `MOSTRO_MCP_ALLOW_WRITES` | no | `true` / `1` / `yes` enables `create_order` / `take_order`. **Default off** |
| `MOSTRO_MCP_COMMAND_TIMEOUT_SECS` | no | Per-command timeout, default 60 |
| `MOSTRO_MCP_LOG` | no | Log filter (stderr), default `info` |

An invalid value stops the server at startup with a message naming it. A
switch that is neither true nor false is an error, so a typo can never enable
writes.

## Connecting an MCP client

The server speaks MCP over stdio. Claude Code:

```bash
claude mcp add mostro \
  -e MOSTRO_PUBKEY=<node pubkey> -e RELAYS=wss://relay.mostro.network \
  -e MOSTRO_CLI_PATH=/path/to/mostro-cli \
  -- /path/to/mostro-mcp
```

Any host that takes the usual JSON configuration:

```json
{
  "mcpServers": {
    "mostro": {
      "command": "/path/to/mostro-mcp",
      "env": {
        "MOSTRO_PUBKEY": "<node pubkey>",
        "RELAYS": "wss://relay.mostro.network",
        "MOSTRO_CLI_PATH": "/path/to/mostro-cli"
      }
    }
  }
}
```

`examples/mcp_call.rs` is a minimal client for trying it by hand:

```bash
cargo run --example mcp_call -- --list
cargo run --example mcp_call -- list_orders '{"kind":"sell","currency":"USD"}'
```

## Tools

An order is named by its **maker**. Taking a `sell` order means you *buy* sats
and pay fiat; taking a `buy` order means you *sell* sats and pay a hold invoice.

| Tool | Read-only | mostro-cli | Arguments |
|---|---|---|---|
| `list_orders` | yes | `listorders` | `kind?` (`buy`/`sell`), `currency?`, `status?` (`pending` default, `in-progress`, `success`, `canceled`) |
| `get_order` | yes | `ordersinfo` | `order_id` (UUID) |
| `create_order` | no | `neworder` | `kind`, `fiat_code`, `fiat_amount` **or** `fiat_amount_min`+`fiat_amount_max`, `payment_method`, `amount_sats?` (omit = market price), `premium?` (above -100 %), `invoice?` (buy only), `expiration_days?` |
| `take_order` | no | `takesell` / `takebuy` | `order_id`, `order_kind` (the order's side, never guessed), `fiat_amount?` (range orders), `invoice?` (taking a sell only) |

Each tool advertises a JSON schema and MCP annotations (`readOnlyHint`). Every
failure is a tool error (`isError: true`) whose text says what to fix; a CLI
failure includes the CLI's own output. Results are the CLI's text output, with
ANSI codes stripped.

## Security

- **Writes are off by default.** `create_order` and `take_order` are refused
  server-side unless the server was started with `MOSTRO_MCP_ALLOW_WRITES=true`,
  independently of any confirmation the MCP host shows. Refused calls never
  start the CLI.
- **No shell, no arbitrary commands.** The CLI is spawned directly with one
  argument per value, from a fixed allow-list of four commands. Every value is
  passed as `--flag=value`, so a value starting with `-` can never become a
  flag. Order ids, fiat codes, invoices and free text are validated before the
  process starts.
- **No keys as arguments, none in output.** No tool takes a key or mnemonic.
  The CLI starts with a cleared environment: the node configuration plus
  `PATH`, `HOME`, `USER`, `TMPDIR`, locale and TLS-certificate variables, so
  `ADMIN_NSEC`, `MOSTRO_RPC_TOKEN` or any other exported secret never reaches
  it. Output is scanned and anything shaped like an `nsec1…` is redacted before
  it reaches the client.
- **The side of a take is checked twice.** `order_kind` is required (never
  guessed), and the node itself refuses a take of the wrong side
  (`InvalidOrderKind`, verified on regtest: the order stays `pending`).
- **One CLI run at a time.** Calls are serialized: every run shares the CLI's
  SQLite database and its next trade-key index.
- **Bounded output.** stdout and stderr are cut at 64 KiB each.
- **No blind retries.** A timeout or a failed take says the command may have
  reached the node and asks the agent to check before retrying.
- **stdout is the protocol.** Logs go to stderr only.
- `MCLI_DIR` holds the identity's mnemonic: keep it `0700`, out of backups you
  share and out of anything an LLM can read.

## Regtest network

`regtest/` is a complete local network: bitcoind, three LND nodes (the node's,
Alice's and Bob's), a nostr relay and **mostrod v0.19.3** from the official
release binary, pinned and checksum-verified. The upstream `mostro-regtest`
repository is out of date: its image expects a `.tar.gz` release asset, which
v0.19.3 no longer ships, and its settings predate the current daemon.

```bash
regtest/devenv.sh up              # start, fund, open 4 channels (idempotent)
regtest/devenv.sh status
eval "$(regtest/devenv.sh env alice)"   # point the CLI and the server at Alice
cargo test --test regtest_e2e -- --ignored
regtest/devenv.sh clean           # wipe everything, test identities included
```

The node's key is the public `mostro-regtest` test key
(`724497f3…218ac8d5`); the relay is `ws://localhost:7000`. `devenv.sh cli
<alice|bob> …` runs mostro-cli as either user, and `invoice` / `pay` / `lncli`
drive their wallets.

### A full trade through the MCP server

```bash
export MOSTRO_MCP_ALLOW_WRITES=true
# Alice publishes a sell order (MCP)
(eval "$(regtest/devenv.sh env alice)"; cargo run -q --example mcp_call -- create_order \
  '{"kind":"sell","fiat_code":"USD","fiat_amount":15,"payment_method":"bank transfer","amount_sats":15000}')
# Bob takes it (MCP), handing over the invoice his sats go to
(eval "$(regtest/devenv.sh env bob)"; cargo run -q --example mcp_call -- take_order \
  "{\"order_id\":\"<id>\",\"order_kind\":\"sell\",\"invoice\":\"$(regtest/devenv.sh invoice bob 15000)\"}")
# The rest of the lifecycle is not exposed as tools yet: mostro-cli
regtest/devenv.sh cli alice getdm           # Alice's hold invoice
regtest/devenv.sh pay alice <hold invoice>
regtest/devenv.sh cli bob fiatsent -o <id>
regtest/devenv.sh cli alice release -o <id>
```

Taking without `invoice` works too: the node then asks for one, and Bob sends
it with `regtest/devenv.sh cli bob addinvoice -o <id> -i <bolt11>`.

Both ways were run on 2026-10-14 and ended in `success`, read back through
`get_order` and `list_orders`: order `52f8d0fe…` (take, then `addinvoice`) and
order `aa1b2c87…` (take with the invoice, mostro-cli `7944e82`), with Bob's
invoices settled for 15 000 and 11 000 sats.

## Limitations

- **Text output.** `mostro-cli` has no machine-readable output: results are
  its tables. The server does not parse them; that would break on any change
  to emoji, spacing or columns. Structured results need a JSON mode in the CLI,
  or the library route below.
- **`get_order` only describes your own orders.** The daemon answers
  `ordersinfo` per identity; another user's order returns *Resource not found*.
  Use `list_orders` for the book.
- **A failed write may still have reached the node.** A timeout, or a CLI that
  fails after the node answered, does not mean nothing happened. The error says
  so, and the `take_order` description tells agents to check with `get_order` or
  `list_orders` before retrying.
- **Invoices must outlive the node's window.** mostrod rejects a BOLT11 invoice
  that expires before `now + invoice_expiration_window` (one hour by default).
- **Four tools.** `add_invoice`, `fiat_sent`, `release`, `cancel`, `dispute`,
  messages and ratings are still CLI-only.
- **One identity per server process**, the one in `MCLI_DIR`.

## Next step: the library route

`mostro-cli` is already split into a library, `mostro_client` (`[lib]` in its
`Cargo.toml`), exposing `db` (identity, keys, orders), `util::net::connect_nostr`,
`util::messaging::{send_dm, wait_for_dm}`, `util::events::fetch_events_list`,
`parser` and `cli`. The obstacle is output: the `cli::execute_*` functions
print their results (about 290 `println!` across `cli/` and `parser/`) instead of
returning them. The extraction worth doing:

1. Make the `execute_*` functions return typed results; move printing to the
   binary. The CLI keeps its behaviour, and the `WaitingSellerToPay` bug above
   gets fixed on the way.
2. Have mostro-mcp depend on `mostro_client` instead of spawning it: structured
   JSON results (`fetch_events_list` already returns events), one Nostr
   connection per server instead of per call, real errors instead of exit codes.
3. Add the remaining lifecycle tools on top.

Until then the subprocess backend stays: it works today against an unmodified
CLI and node.
