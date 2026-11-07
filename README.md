# mostro-mcp

An [MCP](https://modelcontextprotocol.io) server that lets AI agents use
[Mostro](https://mostro.network), the peer-to-peer Bitcoin/Lightning exchange
over Nostr. It drives `mostro-cli` as a subprocess through the CLI's versioned
JSON interface (`--output json`).

```text
MCP client / agent ──MCP (stdio)──▶ mostro-mcp ──subprocess──▶ mostro-cli ──Nostr──▶ mostrod
```

## Requirements

- Rust 1.88+ (built and tested with 1.99).
- [`mostro-cli`](https://github.com/MostroP2P/mostro-cli) **0.17.0 or later**,
  the first release whose commands answer in JSON. At startup the server runs
  `mostro-cli --output json schema` once and refuses to start, naming what is
  missing, unless the CLI lists every command the enabled tools need with JSON
  output. An older CLI stops it with "predates the JSON interface".
- An identity in the CLI's data directory (`MCLI_DIR`): the JSON commands never
  create one. Create it once with `mostro-cli identity init`; `mostro-cli
  identity show` prints its public key. Without one every tool answers
  `identity_missing`.
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
| `MOSTRO_MCP_ALLOW_WRITES` | no | `true` / `1` / `yes` enables the state-changing tools. **Default off** |
| `MOSTRO_MCP_COMMAND_TIMEOUT_SECS` | no | Per-command timeout, default 60, at least 30 (the CLI waits up to 15 s for the node's reply) |
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
| `list_orders` | yes | `order list` | `kind?` (`buy`/`sell`), `currency?`, `status?` (`pending` default, `in-progress`, `success`, `canceled`) |
| `get_order` | yes | `order info` | `order_id` (UUID) |
| `create_order` | no | `order new` | `kind`, `fiat_code`, `fiat_amount` **or** `fiat_amount_min`+`fiat_amount_max`, `payment_method`, `amount_sats?` (omit = market price), `premium?` (above -100 %), `invoice?` (buy only), `expiration_days?` |
| `take_order` | no | `order take-sell` / `order take-buy` | `order_id`, `order_kind` (the order's side, never guessed), `fiat_amount?` (range orders), `invoice?` (taking a sell only) |
| `list_my_orders` | yes | `order mine` | none. Local store only, no network |
| `list_messages` | yes | `message list` | `order_id?`, `since_minutes?` (1 to 10080, default 30) |
| `add_invoice` | no | `order add-invoice` | `order_id`, `invoice` (BOLT11 or Lightning address, for the sats you buy) |
| `add_bond_invoice` | no | `bond add-invoice` | `order_id`, `invoice` (BOLT11 for exactly your share of a slashed bond) |
| `fiat_sent` | no | `order fiat-sent` | `order_id` |
| `release` | no | `order release` | `order_id`, `fiat_received` (must be `true`, else refused before the CLI runs) |
| `cancel` | no | `order cancel` | `order_id` |
| `sync_orders` | no | `order sync` | `since_minutes?` (1 to 10080, the default). Saves in the local store only |

Every state-changing tool passes `--yes`: the confirmation is the MCP host's
approval prompt plus `MOSTRO_MCP_ALLOW_WRITES`. **No tool pays an invoice.**
When the node asks for a payment (a seller's hold invoice, a bond),
`list_messages` returns the invoice as data and the user pays it from their own
wallet.

Each tool advertises a JSON schema and MCP annotations (`readOnlyHint`). Every
failure is a tool error (`isError: true`) whose text says what to fix.

A result is the CLI's JSON outcome document, as text and as structured content:
`status` `ok` is a result; `error` is a tool error carrying `error.code`;
`uncertain` (the request may have reached the node) is a tool error whose text
starts with a warning not to retry before checking the state. Output that is
not exactly one outcome document, or that disagrees with the CLI's exit status,
is a tool error with a cleaned excerpt, under the same warning when the tool is
state-changing.

## Security

- **Writes are off by default.** Every tool but `list_orders`, `get_order`,
  `list_my_orders` and `list_messages` is refused server-side unless the server was started with `MOSTRO_MCP_ALLOW_WRITES=true`,
  independently of any confirmation the MCP host shows. Refused calls never
  start the CLI.
- **No shell, no arbitrary commands.** The CLI is spawned directly with one
  argument per value, from a fixed allow-list of commands. Every value is
  passed as `--flag=value`, so a value starting with `-` can never become a
  flag. Order ids, fiat codes, invoices and free text are validated before the
  process starts.
- **No keys as arguments, none in output.** No tool takes a key or mnemonic.
  The CLI starts with a cleared environment: the node configuration plus
  `PATH`, `HOME`, `USER`, `TMPDIR`, locale and TLS-certificate variables, so
  `ADMIN_NSEC`, `MOSTRO_RPC_TOKEN` or any other exported secret never reaches
  it. Anything shaped like an `nsec1…` is redacted before it reaches the
  client: in the raw output, and again in every string of the parsed document
  (a JSON escape can hide the prefix from the first pass).
- **The side of a take is checked twice.** `order_kind` is required (never
  guessed), and the node itself refuses a take of the wrong side
  (`InvalidOrderKind`, verified on regtest: the order stays `pending`).
- **One CLI run at a time.** Calls are serialized: every run shares the CLI's
  SQLite database and its next trade-key index.
- **Bounded output.** A document over 256 KiB is refused whole, never cut;
  stderr is shown cut at 64 KiB.
- **No blind retries.** A timeout, an `uncertain` outcome, or unreadable
  output of a state-changing tool says the command may have reached the node
  and asks the agent to check before retrying.
- **stdout is the protocol.** Logs go to stderr only.
- `MCLI_DIR` holds the identity's mnemonic: keep it `0700`, out of backups you
  share and out of anything an LLM can read.

## Regtest network

`regtest/` is a complete local network: bitcoind, three LND nodes (the node's,
Alice's and Bob's), a nostr relay and **mostrod v0.20.2** from the official
release binary, pinned and checksum-verified. The upstream `mostro-regtest`
repository is out of date: its image expects a `.tar.gz` release asset, which
current releases no longer ship, and its settings predate the current daemon.

```bash
regtest/devenv.sh up              # start, fund, open 4 channels, create Alice's and Bob's identities (idempotent)
regtest/devenv.sh status
eval "$(regtest/devenv.sh env alice)"   # point the CLI and the server at Alice
cargo test --test regtest_e2e -- --ignored --test-threads=1 --skip loses_the_bond   # the trades below, through MCP
regtest/devenv.sh clean           # wipe everything, test identities included
```

`MOSTRO_BONDS=1 regtest/devenv.sh up` starts (or restarts) the node with the
taker bond of `regtest/mostrod/bonds.toml`: a taker pays a hold-invoice bond
(1 % of the order, at least 1 000 sats) before the trade goes on, and loses it
if they let a waiting state time out (60 s in this mode), the other party
claiming half with `add_bond_invoice`. `regtest/devenv.sh up` without it goes
back to no bonds. The e2e tests run in both modes, the slash test only with
bonds: run without `--skip loses_the_bond` then. On a node without bonds it
fails rather than pass without exercising anything.

The node's key is the public `mostro-regtest` test key
(`724497f3…218ac8d5`); the relay is `ws://localhost:7000`. `devenv.sh cli
<alice|bob> …` runs mostro-cli as either user, and `invoice` / `pay` / `lncli`
drive their wallets.

### A full trade through the MCP server

```bash
export MOSTRO_MCP_ALLOW_WRITES=true
alice() { (eval "$(regtest/devenv.sh env alice)"; cargo run -q --example mcp_call -- "$@"); }
bob() { (eval "$(regtest/devenv.sh env bob)"; cargo run -q --example mcp_call -- "$@"); }

# Alice publishes a sell order
alice create_order '{"kind":"sell","fiat_code":"USD","fiat_amount":15,"payment_method":"bank transfer","amount_sats":15000}'
# Bob takes it, handing over the invoice his sats go to
bob take_order "{\"order_id\":\"<id>\",\"order_kind\":\"sell\",\"invoice\":\"$(regtest/devenv.sh invoice bob 15000)\"}"
# Alice reads the hold invoice the node asks her to pay, and pays it from her wallet
alice list_messages '{"order_id":"<id>"}'
regtest/devenv.sh pay alice <hold invoice>
# Bob sends the fiat, then says so; Alice sees it arrive and releases
bob fiat_sent '{"order_id":"<id>"}'
alice release '{"order_id":"<id>","fiat_received":true}'
alice get_order '{"order_id":"<id>"}'       # status success
```

Taking without `invoice` works too: `list_messages` then shows Bob the node's
request, and he answers with `add_invoice`.

`tests/regtest_e2e.rs` runs this trade four ways (Bob or Alice publishes the
order; Bob's invoice comes first or through `add_invoice` when the node asks),
a cancel and a sync, and with bonds on, a taker who lets the wait time out and
loses the bond while the seller claims her share with `add_bond_invoice`. On
2026-11-07, with mostrod v0.20.2 and mostro-cli v0.17.0, all passed in both
modes: the trades in `success` with Bob's 15 000 sats settled, the taker's bond
paid on every take with bonds on, and Alice's 500-sat share of a slashed
1 000-sat bond settled.

## Limitations

- **`get_order` only describes your own orders.** The daemon answers
  `order info` per identity; another user's order returns *Resource not found*.
  Use `list_orders` for the book.
- **A failed write may still have reached the node.** A timeout, or a CLI that
  fails after the node answered, does not mean nothing happened. The error says
  so, and the `take_order` description tells agents to check with `get_order` or
  `list_orders` before retrying.
- **Invoices must outlive the node's window.** mostrod rejects a BOLT11 invoice
  that expires before `now + invoice_expiration_window` (one hour by default).
- **Disputes and ratings are CLI-only.** mostro-cli 0.17.0 offers `dispute
  open` and `order rate` in JSON; they are not tools yet.
- **One identity per server process**, the one in `MCLI_DIR`.
