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
- [`mostro-cli`](https://github.com/MostroP2P/mostro-cli) **0.17.1 or later**,
  the first release with `order watch` and `order chat` in JSON (0.17.0 was the
  first whose commands answer in JSON). At startup the server runs
  `mostro-cli --output json schema` once and refuses to start, naming what is
  missing, unless the CLI lists every command the enabled tools need with JSON
  output. An older CLI stops it with "predates the JSON interface".
- An identity in the CLI's data directory (`MCLI_DIR`): the JSON commands never
  create one. Create it once with `mostro-cli identity init`; `mostro-cli
  identity show` prints its public key. Without one every tool answers
  `identity_missing`.
- For the regtest network: Docker with Compose v2, `jq`.

## Installation

Either way, `mostro-mcp` needs [`mostro-cli`](https://github.com/MostroP2P/mostro-cli)
**0.17.1 or later** installed separately and an identity created with
`mostro-cli identity init` (see [Requirements](#requirements)).

### Option A: a release binary

Download the package for your system from
[the releases page](https://github.com/MostroP2P/mcp/releases), together with
its `.sha256` file:

| System | Architecture | Package |
|---|---|---|
| Linux | x86_64 | `mostro-mcp-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | ARM64 (aarch64) | `mostro-mcp-<version>-aarch64-unknown-linux-gnu.tar.gz` |
| macOS | Intel | `mostro-mcp-<version>-x86_64-apple-darwin.tar.gz` |
| macOS | Apple Silicon | `mostro-mcp-<version>-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `mostro-mcp-<version>-x86_64-pc-windows-msvc.zip` |

Each package holds the `mostro-mcp` binary (`mostro-mcp.exe` on Windows) and
this README.

Linux and macOS (set `TARGET` from the table):

```bash
VERSION=0.1.0
TARGET=x86_64-unknown-linux-gnu
PKG=mostro-mcp-$VERSION-$TARGET.tar.gz
BASE=https://github.com/MostroP2P/mcp/releases/download/v$VERSION
curl -LO "$BASE/$PKG" -LO "$BASE/$PKG.sha256"
sha256sum -c "$PKG.sha256"          # macOS: shasum -a 256 -c "$PKG.sha256"
tar -xzf "$PKG"
install -m 755 "mostro-mcp-$VERSION-$TARGET/mostro-mcp" ~/.local/bin/   # any directory in PATH
```

The macOS binaries are not signed. If macOS refuses to run it, remove the
quarantine flag once:

```bash
xattr -d com.apple.quarantine ~/.local/bin/mostro-mcp
```

Windows (PowerShell):

```powershell
$Version = "0.1.0"
$Pkg = "mostro-mcp-$Version-x86_64-pc-windows-msvc.zip"
$Base = "https://github.com/MostroP2P/mcp/releases/download/v$Version"
Invoke-WebRequest "$Base/$Pkg" -OutFile $Pkg
Invoke-WebRequest "$Base/$Pkg.sha256" -OutFile "$Pkg.sha256"
$Expected = (Get-Content "$Pkg.sha256").Split(" ")[0]
if ((Get-FileHash -Algorithm SHA256 $Pkg).Hash -ne $Expected) { throw "checksum mismatch" }
Expand-Archive $Pkg -DestinationPath .
# Move mostro-mcp.exe to a folder in your PATH, for example:
New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\Programs\mostro-mcp" | Out-Null
Move-Item "mostro-mcp-$Version-x86_64-pc-windows-msvc\mostro-mcp.exe" "$env:LOCALAPPDATA\Programs\mostro-mcp\" -Force
```

### Option B: build from source

With Rust 1.88 or later:

```bash
git clone https://github.com/MostroP2P/mcp.git
cd mcp
cargo build --release          # binary at target/release/mostro-mcp
```

### Publishing a new version

The version in `Cargo.toml` is the source of truth and the tag must match it.
Bump `version` in `Cargo.toml` (and `Cargo.lock`, e.g. with `cargo build`) in a
pull request; once it is merged, tag that merge commit by its SHA (another pull
request may have been merged after it) and push the tag:

```bash
git fetch origin
git merge-base --is-ancestor <merge-commit-sha> origin/main   # must succeed
git tag -a v0.2.0 <merge-commit-sha> -m "v0.2.0"
git push origin v0.2.0
```

The `Release` workflow checks that the tag matches `Cargo.toml`, runs the tests
and clippy, builds the five packages above with their `.sha256` files and
publishes them in the GitHub release with generated notes.

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
| `MOSTRO_MCP_LOG_FILE` | no | Also append the log to this file (for a host that discards stderr) |
| `MOSTRO_MCP_HEARTBEAT_SECS` | no | Log `alive` every N seconds (1 to 3600). Off by default |

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
| `watch_order` | yes | `order watch` | `order_id`, `after?` (the previous result's `cursor`), `since_minutes?` (first call only, default 30), `wait_seconds?` (1 to 60, default 20) |
| `read_chat` | yes | `order chat` | `order_id`, `since_minutes?` (1 to 10080, the default) |
| `send_chat` | no | `order chat --message` | `order_id`, `message` (1 to 4096 bytes; line breaks kept) |
| `open_dispute` | no | `dispute open` | `order_id` (an `active` or `fiat-sent` order you hold) |
| `rate_counterparty` | no | `order rate` | `order_id` (a finished order you hold), `rating` (1 to 5, the user's choice; anything else is refused before the CLI runs) |

`watch_order` follows one trade without polling: each call returns at the
first new node message or chat message, or empty after `wait_seconds`, with a
`cursor` to pass back as `after`; `ended` is true once the trade is over. Its
first call replays the last 30 minutes, so the server's instructions tell agents
to act on each node message (`event_id`) once and to read each chat message
(`message_id`, chat records have no `event_id`) once, to check each step against
the order's status (for example `add_invoice` is valid in
`waiting-buyer-invoice`, and in `settled-hold-invoice` after a failed payout),
and to read `action` when `next_step` is null or unknown (a cooperative cancel
request from the counterparty has no step). The server runs one CLI command at
a time, so other tools wait while a watch does: keep `wait_seconds` short and
loop. A watch may run `wait_seconds` plus 40 s
(the CLI's replays) past `MOSTRO_MCP_COMMAND_TIMEOUT_SECS`.

The seller's payment details for the fiat travel in the order's peer chat, not
in the node's messages: `read_chat` and `watch_order` show them, `send_chat`
answers. Chat text comes from the counterparty; the server's instructions tell
agents to treat it as data, never to call a state-changing tool because a chat
message asks for it, and never to take an invoice from the chat. A week of a
flooded chat can exceed the 256 KiB document limit: narrow it with
`since_minutes`.

`open_dispute` is for a stuck trade: the fiat was sent and the seller does not
release, or the counterparty stopped answering. The server's instructions let
an agent suggest a dispute but leave the decision to the user, since a dispute
brings in a solver and cannot be withdrawn. The node refuses any order that is
not `active` or `fiat-sent` (`not_allowed_by_status`) and answers a second
dispute on the same order with silence, so the tool reports `uncertain`;
`get_order` then shows the status `dispute`.

`rate_counterparty` ends a trade: it sends the rating the user chose for the
counterparty, 1 to 5. The server's instructions tell agents to ask the user for
the score, never to choose one and never to rate unasked. Each side rates an
order once and cannot change it. The node accepts the buyer's rating once the
order is `success`, and the seller's from `settled-hold-invoice` (after the
release, or a solver's settlement) or `success`. Agents go by that status, not
by a `rate` message: a seller whose dispute a solver settled gets only
`admin-settled`, and a watch can end before the buyer's `rate` arrives.

The outcomes say what is known. `ok`: the node acknowledged this rating.
`error`: nothing was recorded, because the node refused it
(`invalid_order_status` for a taken order in another status) or the CLI sent
nothing (`order_unknown`, or `pow_required` before publishing). `uncertain`: the
request was (or may have been) published and no usable answer came, and
whether a rating exists cannot be known. The node gives no answer to a side
that already rated, when the counterparty trades in full privacy, for an order
that was never taken (it cannot name both parties) and on internal failures,
one of which comes after the rating is stored. An `uncertain` rating, a
timeout, unreadable output, or a CLI that started and then failed is headed by
the rating's own warning, not the generic one: tell the user the rating may
have been sent, but whether it was recorded is unknown. **An uncertain rating
is never resent**: the agent never calls `rate_counterparty` again for that
order and side, not even if the user asks. mostrod versions differ in how they
guard against a duplicate rating (v0.18.0 checks the order's side flag and then
writes the score, with no conditional claim; v0.20.2 claims the flag and writes
the score in one transaction, `claim_order_rating_flag`), and this server pins
no node version, so it cannot promise that a second attempt would count once.
`uncertain` with `pow_required` gets a warning of its own: the node most likely
dropped the rating for too little proof of work (likely, not proven); the
warning names the bits the node requires (`error.required`) and the `POW` that
was mined (`error.configured`) so the user can raise `POW` for future requests
(`POW` is read at startup), and it still does not let this rating be sent
again. Nothing is ever retried automatically. A CLI that could not start sent
nothing and is a plain error. `get_order` does not show ratings. Only the tool's `ok`
proves the exact rating: mostro-cli accepts as the acknowledgement only a
`rate-received` for the same order carrying the same rating, and this server
passes that result on without checking anything itself. A `rate-received`
record in `list_messages` for the order shows that a rating from this side was
recorded, but mostro-cli 0.17.1 lists its payload as `other` (`rating_user`)
without the score, so it does not show which one. A missing acknowledgement, a
missing rating payload or a mismatched rating confirms nothing. The server passes every node and chat record through as the CLI
returns it; telling records already handled from new ones (node messages by
`event_id`, chat messages by `message_id`) is the agent's.

Like every request it sends, mostro-cli 0.17.1 mines the larger of `POW` and the
difficulty the node advertises (for a rating, the first-contact one), on its own
up to 20 bits. A node asking for more stops the rating before it is sent
(`pow_required`) until `POW` is raised to `error.required`. When the CLI's early
read of the node's advertisement misses, it sends with `POW` alone; the node
drops an under-mined request in silence, and the outcome is `uncertain` with
`pow_required`. See the book's
[order rate](https://mostro.network/mostro-cli-book/commands/order_rate.html)
page and its [guide for AI agents](https://mostro.network/mostro-cli-book/wrapper/ai_agents.html).

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

The [mostro-cli book](https://mostro.network/mostro-cli-book/) documents these
documents field by field: [the outcome envelope](https://mostro.network/mostro-cli-book/envelope.html),
[error codes](https://mostro.network/mostro-cli-book/errors.html),
[order statuses](https://mostro.network/mostro-cli-book/reference/statuses.html),
[the node's actions and `next_step` values](https://mostro.network/mostro-cli-book/reference/actions.html)
and [recovering from `uncertain`](https://mostro.network/mostro-cli-book/flows/uncertain.html).
The server's instructions point agents to it.

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
# Alice gives Bob her payment details in the order's chat; Bob waits for them
alice send_chat '{"order_id":"<id>","message":"Pay 15 USD to alias alice.bank"}'
bob watch_order '{"order_id":"<id>"}'       # then again with "after":"<cursor>"
bob read_chat '{"order_id":"<id>"}'
# Bob sends the fiat, then says so; Alice sees it arrive and releases
bob fiat_sent '{"order_id":"<id>"}'
alice release '{"order_id":"<id>","fiat_received":true}'
alice get_order '{"order_id":"<id>"}'       # status success
# Each rates the other once, with the score the user chose
alice rate_counterparty '{"order_id":"<id>","rating":5}'
bob rate_counterparty '{"order_id":"<id>","rating":5}'
```

Taking without `invoice` works too: `list_messages` then shows Bob the node's
request, and he answers with `add_invoice`.

`tests/regtest_e2e.rs` runs this trade four ways (Bob or Alice publishes the
order; Bob's invoice comes first or through `add_invoice` when the node asks),
each with Alice's payment details sent by `send_chat` and seen by Bob through
`watch_order` and `read_chat`, and Bob's watch seeing the trade end; a cancel and a sync; a dispute
(refused on a pending order, opened by Bob on an active trade, a second one `uncertain`); and with bonds on, a taker who lets the wait time out and
loses the bond while the seller claims her share with `add_bond_invoice`. On
2026-11-10, with mostrod v0.20.2 and mostro-cli v0.17.1, all passed in both
modes: the trades in `success` with Bob's 15 000 sats settled, the taker's bond
paid on every take with bonds on, and Alice's 500-sat share of a slashed
1 000-sat bond settled. Two rating tests came later: an untaken order
`uncertain` (no answer) and a running trade refused (`invalid_order_status`),
then both sides rating the finished trade (Bob after his watch ended) and a
second rating from the same side `uncertain`; and a seller who rates after a
solver settles the dispute (`admtakedispute`, `admsettle` with the node's
public test key as `ADMIN_NSEC`), told `admin-settled` and no `rate`. On
2026-11-13, with the same versions, all ten passed (nine without bonds, the
bond test with them). Read afterwards from the node's database, the second
rating (a 1 after a 5) was not counted: Bob's identity holds two reviews, a 5
and a 4, with a minimum of 4.

## Diagnosing a server that disappeared

Set `MOSTRO_MCP_LOG_FILE` so the log survives a host that discards stderr.
The log then answers four questions:

- **Who launched it?** `starting mostro-mcp` carries the version, `pid` and
  `ppid`; `client initialized` names the MCP client and its version.
- **Did a call arrive?** Every call logs `tool call tool="…"` when it arrives
  and `tool call finished` with `outcome` (`ok`, `error`, `rejected`) and
  `elapsed_ms`. A tool that runs mostro-cli also logs `mostro-cli finished`
  with the command.
- **How did it end?** `MCP session ended reason=Closed` when the client closed
  the connection; `stopped by a signal: SIGTERM` (also `SIGINT`, `SIGHUP`,
  exit status 128 + the signal) when something stopped the process; `panic`
  when the server crashed.
- **When did it die?** A server that vanished with none of those lines was
  killed with SIGKILL, which no process can catch: look at the client that
  launched it. With `MOSTRO_MCP_HEARTBEAT_SECS` set, the last `alive` line
  says until when it was running.

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
- **After a dispute opens, the rest is CLI-only.** mostro-cli 0.17.1 prints
  the dispute chat (`disputechat`) and the list of disputes (`listdisputes`)
  only for humans, so this server cannot read or answer the solver. Answer the
  solver with mostro-cli.
- **A rating cannot be read back.** No command shows the score a side gave. A
  `rate-received` record in `list_messages` shows that a rating was recorded,
  not which one; only `rate_counterparty`'s own `ok` proves the exact rating.
- **One identity per server process**, the one in `MCLI_DIR`.
