# Test Summary

Evidence per slice of [specs/001-json-migration](../specs/001-json-migration/tasks.md), one entry each.

## Slice 1a: Backend And Outcome Mapping: 2026-11-04

Native Linux, `cargo` of the sandbox toolchain, on `main` after `e29c4be` (local). Lean mode: the only evidence entry for the slice.

- **Fixtures** (`tests/fixtures/outcomes.json`, `README.md`): eleven documents copied from the `contracts/examples.json` files of the CLI repository at `develop` `a364055` (a listing, a node refusal, an order created, a bond to pay, an uncertain take, a range release, a refused release, an uncertain release, a message list, an order added by sync, a missing `--yes`) with their exit codes.
- **Backend** (`cli_backend.rs`): every run adds `--output=json`; it returns `CliRun` (raw stdout, cleaned and cut stderr, exit code) and no longer judges the exit status, because the CLI prints its document for failures too. Cleared environment, timeout, kill on drop and the single-flight lock are unchanged. Unit tests: the argument vector (`--output=json` first), arguments reach the CLI verbatim without a shell, a failing CLI returns its status, stdout and cleaned stderr, stdout is never cut while stderr is.
- **Outcome mapping** (`outcome.rs`): `Outcome::parse` accepts exactly one document of schema version 1 with a valid `status`, checks it against the exit code in both directions (an `ok` document with a failing exit, or a failure with exit 0, is not believed), refuses a document over 256 KiB instead of cutting it, and redacts `nsec1...` and strips ANSI before parsing, so the text and the structured content are the same checked document. `into_tool_result`: `ok` is a result, `error` a tool error, `uncertain` a tool error whose text starts with `UNCERTAIN: ... Do NOT retry ...`; all carry structured content (rmcp 3.5.1 supports it). 9 unit tests, including every fixture parsing with the status its exit code implies.
- **Server** (`server.rs`): the tools' results are the mapped outcomes; output that is not a document (or disagrees with the exit status) is a tool error with the reason, the CLI's stderr and a cleaned excerpt; the instructions describe the JSON results and the `uncertain` rule. The tools themselves are unchanged in this slice: they still build the old arguments until slices 2 and 3.
- **Stdio tests** (`tests/mcp_stdio.rs`, 8, real binary and real MCP client): the fake CLI echoes its arguments and environment as an `ok` document, or prints a chosen fixture with its exit code. Covered: discovery of four tools, the node configuration and nothing else reaching the CLI, the write gate, the three statuses (`ok`, `error`, warned `uncertain`), non-document output keeping the server alive, and a bad argument.
- **Mutation spot checks**: skipping the exit-code check failed the both-directions test; restored.
- **A stray file**: an earlier red run of the overlap test wrote a file named `--output=json` in the working directory (the test script used `$1`, now `--output=json`); the test uses `$2` and the file was deleted.
- **Review fixes (CodeRabbit, 2026-11-06)**: the runner reads stdout to at most one byte past the 256 KiB limit and stderr to 256 KiB, and drains the rest, so a flooding CLI neither fills memory nor is cut off by a closed pipe; `Outcome::parse` also redacts every decoded string and key, since a JSON escape (`\u006esec1...`) slips past the raw-text pass. 3 unit tests; mutation: removing the drain failed both flood tests (the CLI died on SIGPIPE), restored.
- **Gates**: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 72 passed and 0 failed (the real-node e2e is ignored), `cargo build`.

## Slice 1b: Startup Contract Check: 2026-11-04

Native Linux, on top of slice 1a. Lean mode.

- **Contract check** (`contract.rs`, `main.rs`): at startup the server runs `schema` once and stops, naming the missing commands, unless each command the enabled tools need is listed with `json` (`order.list`, `order.info`; plus `order.new`, `order.take-sell`, `order.take-buy` when writes are on). A CLI that predates `schema` stops the server with "predates the JSON interface". The fixtures gain a `schema` outcome reduced to the three fields the check reads (`name`, `cli_path`, `output_modes`); the fake CLI answers `schema` with it.
- **Tests**: 4 unit tests and 2 stdio tests (a missing command named alone; an old CLI). Run against the real CLI of the CLI repository (no network, closed stdin): the check passed and logged "mostro-cli offers the JSON commands this server needs".
- **Mutation spot check**: making the check ignore names failed the missing-command test; restored.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` (no FAILED), build.

## Slice 2: Read Tools: 2026-11-04

Native Linux, on `main` after `47eef04` (local). Lean mode.

- **`list_orders`** now runs `order list [--side] [--currency] [--status]` (the tool keeps its `kind` parameter; the CLI flag is `--side`, `--kind` no longer exists). **`get_order`** runs `order info --order <uuid>` (`--orderids` is gone). Ids and currency are still validated before anything is spawned.
- **Tests**: the argument-vector unit tests of both tools were rewritten first and failed on the old commands; the server and stdio tests assert the new vectors; the three-statuses stdio test (ok, error, warned uncertain) already runs through `list_orders`. Real CLI: `order list` and `order info` accept the exact flags the tools build (checked with `--help`, no network).
- **Mutation spot check**: sending `--kind` instead of `--side` failed the unit and server tests (the run stops at the first failing test binary); restored.
- **Docs**: README tool table and the `get_order` note name the new commands.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` (no FAILED), build.

## Slice 3: Write Tools: 2026-11-04

Native Linux, on `main` after `c0114db` (local). Lean mode.

- **`create_order`** runs `order new --side --currency --fiat-amount [--fiat-amount-max] --payment-method [--sats] [--premium] [--invoice] [--expiration-days] --yes`; a range is now `--fiat-amount` plus `--fiat-amount-max` (the `100-200` syntax is gone), and `amount_sats` with a range is refused before spawning because the CLI prices a range at market. **`take_order`** runs `order take-sell|take-buy --order [--invoice] [--amount] --yes`. `--yes` is always added: the JSON mode never prompts, and the write gate (`MOSTRO_MCP_ALLOW_WRITES`) is unchanged and still checked before anything is built or spawned.
- **Tests**: unit tests of both tools rewritten (vectors, range split, range without sats, flag-looking payment method stays one value, `--yes` last). Stdio: the existing write-gate test now expects `order take-sell`; a new test creates a range order (one `--yes`, `--fiat-amount-max=150`) and shows an `uncertain` take reaching the agent as an error starting `UNCERTAIN`.
- **Real CLI**: `order new`, `order take-sell` and `order take-buy` accept the exact flags the tools build (`--help`, no network, nothing sent).
- **Mutation spot check**: dropping `--yes` from `take_order` failed its unit tests; restored.
- **Not run**: no node or regtest; the real create/take path is covered by the CLI repository's own probes and by slice 5 when authorized.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` (no FAILED), build.
- **Follow-up (2026-11-07, mostro-cli 0.17.0)**: unreadable output of `create_order` or `take_order` now starts with the no-retry warning (FR-001);
  a read gets none. 1 unit test, red first; mutation: passing `false` from the write path failed it, restored. README: mostro-cli 0.17.0 or later
  instead of a commit pin, `identity init` before use, JSON results, redaction and bounds as implemented, the walk-through's remaining CLI steps in
  the new syntax; the library-route section, superseded by this spec, removed. Smoke run against mostro-cli `v0.17.0` (offline: empty `MCLI_DIR`,
  a closed local relay port): the contract check passed with writes on, and the four tools reached the CLI with flags it accepted
  (`identity_missing`, `relay_connection_failed`, never `invalid_arguments`). Gates: fmt, clippy `-D warnings`, `cargo test` 70 + 9 passed, build.

## Slice 4: The Tools That Finish A Trade: 2026-11-07

Native Linux, on `pr/4-migrate-tools` after `c785cd8`, against mostro-cli `v0.17.0`. Lean mode.

- **Tools** (`tools/`): `list_my_orders` → `order mine` (read-only, offline; added beyond the spec as the recovery read after an `uncertain`
  create or take), `list_messages` → `message list [--order] [--since]` (read-only), `add_invoice` → `order add-invoice`, `add_bond_invoice` →
  `bond add-invoice` (BOLT11 only), `fiat_sent` → `order fiat-sent`, `release` → `order release` (refused before spawning unless
  `fiat_received` is `true`), `cancel` → `order cancel`, `sync_orders` → `order sync [--since]`. Every write passes `--yes` and is behind the
  write gate; read windows are 1 to 10080 minutes, as the CLI accepts. Descriptions and instructions name the next tool, say that no tool pays
  an invoice, and what `uncertain` means.
- **Contract check**: reads need `order.mine` and `message.list`; writes add the six trade-step commands.
- **Config**: `MOSTRO_MCP_COMMAND_TIMEOUT_SECS` under 30 is refused: the CLI waits up to 15 s (`--reply-timeout` default) for the node's
  reply, and a shorter limit would stop it before it can report `uncertain`.
- **Fixtures**: rebuilt by `tests/fixtures/refresh.py` from the CLI at `v0.17.0`; the eleven earlier documents are byte-identical there; added
  `cancel_canceled`, `bond_invoice_accepted`, `messages_identity_missing`, `mine_newest_first`; the `schema` fixture now lists `dispute.open`,
  `order.rate`, `identity.*` and `order.mine` with JSON.
- **Tests**: unit tests for every new `args` and the shared validators, written first and failing on `todo!()` stubs (13 red); server tests
  for the twelve tools' hints, the write gate on every trade step, `release` without `fiat_received` never spawning, and each command name;
  stdio tests for a whole trade's argument vectors and the CLI's own documents reaching the agent.
- **Mutation spot check**: dropping the `fiat_received` check failed two tests; restored.
- **Real CLI** (`v0.17.0`, offline: empty `MCLI_DIR`, closed local relay port): the contract check passed with writes on, and all eight new
  tools reached the CLI with flags it accepted (`identity_missing` each, never `invalid_arguments`); the CLI created nothing in `MCLI_DIR`.
- **Not run**: no node or regtest (slice 5).
- **Gates**: fmt, clippy `-D warnings`, `cargo test` 86 + 11 passed, build.
- **Review fixes (2026-11-07)**: a timed-out write that sends to the node now starts with the no-retry warning, and the timeout text points
  at `get_order`, `list_my_orders` and `list_messages` instead of inviting a retry; `sync_orders`, which only writes the local store, is gated
  but never warned; invoices may be 4096 characters (route hints), with the limit named; the `release` description says to set
  `fiat_received` only once the user said so; the stdio trade table adds `add_bond_invoice` and `cancel`. 3 tests, red first; mutation:
  dropping the timeout warning failed its test, restored. Gates: fmt, clippy `-D warnings`, `cargo test` 89 + 11 passed.
