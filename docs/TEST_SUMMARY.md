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

## Slice 5: Regtest, E2E And Verification: 2026-11-07

Native Linux, Docker 29.9, on `pr/5-trade-tools` after `4355e88`, mostro-cli `v0.17.0` (debug build, `MOSTRO_CLI_PATH`). Authorized by the
maintainer for this session.

- **Regtest**: mostrod pinned to `v0.20.2`; sha256 `2d11a694…773d` from the release's `manifest.txt`, whose signature (`manifest.txt.sig.negrunch`)
  verified. The v0.20 settings keys are additive (`[health]`, pegged prices, bond CLTV) with defaults; bonds stay off, so `settings.toml`
  only changes its header. `devenv.sh up` now creates Alice's and Bob's identities (`identity init`), which the JSON commands never do.
- **E2E** (`tests/regtest_e2e.rs`, ignored by default, one MCP server per user from `devenv.sh env`): the book read names the node; a whole
  sell trade with the invoice given on the take; the same trade with `add_invoice` when the node asks; a pending order canceled and a sync.
  The hold invoice is paid from Alice's wallet (`devenv.sh pay`), never by a tool. The old e2e expected the text output (`List Orders`); it
  now checks the JSON document.
- **Run**: `cargo test --test regtest_e2e -- --ignored --test-threads=1`: 4 passed in about 20 s. Checked outside the tests: order
  `6cdced42…` is `success` on the node (`order info`) and Bob's 15 000-sat invoice is `SETTLED` in his LND. Seen on the way: `order mine`
  keeps Alice's last local status (`settled-hold-invoice`) after the node reports `success`; that is the CLI's local store, not this server.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` (no FAILED), build.

## Follow-up: Buy-Side Trades And Bonds On Regtest: 2026-11-07

Native Linux, Docker 29.9, on `main` after `99d74d6`, mostrod `v0.20.2`, mostro-cli `v0.17.0` (debug build). Authorized by the maintainer.

- **Regtest**: `MOSTRO_BONDS=1 devenv.sh up` writes the node's settings into `.state/mostrod.toml` (base settings, plus
  `mostrod/bonds.toml`: taker bond of 1 % with a 1 000-sat floor, slashed on a waiting timeout, half to the counterparty; waiting states cut
  to 60 s) and restarts mostrod when they change; without it the node goes back to no bonds.
- **E2E**: the trade is now run four ways (Alice's sell order or Bob's buy order; Bob's invoice first or through `add_invoice`); a bond
  the node asks of the taker is paid from the taker's wallet, and each trade asserts that a bond is asked exactly when bonds are on. New: a
  taker who pays the bond and never sends the invoice is slashed after the timeout, and the seller claims her share with
  `add_bond_invoice`; it skips itself without bonds.
- **Found on the way**: the node sends the seller's payment step as two `pay-invoice` messages, first with the counterparty (`peer`), then
  with the invoice (`payment_request`). The first run took the first one and failed (empty invoice), and its stuck order then made the next
  take fail with `pending_order_exists`. The test now reads the `payment_request`. The `list_messages` description said "newest first";
  the CLI lists oldest first, so the description is corrected and now names the two messages.
- **Runs**: without bonds 7 passed (the slash test skipped) in 39 s; with bonds 7 passed in 256 s. Checked outside the tests: Alice's
  500-sat share invoice is `SETTLED` and Bob's slashed 1 000-sat bond payment `SUCCEEDED`.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` 89 + 11 passed, build.
- **Review fix (CodeRabbit)**: without bonds the slash test passed without exercising anything, its skip note hidden by libtest. It now
  fails on a node without bonds, and the no-bonds run leaves it out with `--skip loses_the_bond` (README and the test's header).

## Follow-up: Pointing Agents At The mostro-cli Book: 2026-11-07

- **Instructions** (`server.rs`): one paragraph sends agents to https://mostro.network/mostro-cli-book/ for the outcome documents (error
  codes, `next_step` and node actions, recovery from `uncertain`). **README**: the "Tools" section links the envelope, error codes,
  statuses, actions and uncertain-recovery pages. All six URLs answered 200.
- **Test**: the stdio discovery test asserts that the instructions carry the book's URL; it failed before the change. Gates: fmt, clippy
  `-D warnings`, `cargo test` 89 + 11 passed.

## Follow-up: Opening A Dispute (Issue #14): 2026-11-11

- **Tool** (`src/tools/dispute.rs`, `server.rs`): `open_dispute` runs `dispute open --order=<uuid> --yes` behind the write gate. A timeout
  or unreadable output carries the no-retry warning. Annotations: not read-only, not destructive, not idempotent, open world. The
  description and the instructions leave the decision to the user, name the `active` / `fiat-sent` rule, and say never to resend on
  `uncertain` (the node answers a second dispute with silence). `contract::WRITE` requires `dispute.open`. The dispute chat and
  `listdisputes` are not wrapped: mostro-cli 0.17.1 prints them only for humans.
- **Fixtures**: the five `dispute.open` examples of spec `013-dispute-open` at `v0.17.1` (opened, opened with `local_status_stale`,
  `node_refused` / `not_allowed_by_status`, `uncertain` / `reply_timeout`, `order_unknown`). Before they were added, `refresh.py` reproduced
  the current `outcomes.json` byte for byte.
- **Tests**: written first and run red, first the unit tests (`todo!()` panics), then the server tests (no `open_dispute` method). Unit:
  the exact argv, a lowercased id, a bad id refused. Server: refused by default without spawning, runs `dispute open`, the annotations
  and description, an opened dispute's id and stale-status warning, a refusal that keeps the node's reason without the warning, a warned
  `uncertain` and timeout, the instructions. Contract: `dispute.open` required with writes on. Stdio: discovery of sixteen tools and the
  `open_dispute` schema, the whole-trade argv, and four of the CLI's documents reaching the agent.
- **Regtest** (mostrod v0.20.2, local network): a pending order is refused with `not_allowed_by_status`; Bob disputes an active trade,
  both sides see `dispute`, and a second dispute is `uncertain` / `reply_timeout`. `trade()` is split at `active` (`until_active`) so the
  test reuses it. Without bonds, 7 passed in 59 s (`--skip loses_the_bond`); with bonds (`MOSTRO_BONDS=1`), 8 passed in 286 s,
  the dispute taken to `active` through a paid taker bond. The review caught the listing test still expecting 15
  tools. First run: every request was refused with `maintenance_mode`, because mostrod turns maintenance on when its LND has seen no
  block for 600 s and regtest only mines on demand. One `devenv.sh mine 1` cleared it after the node's 300 s recovery window.
- **Mutation spot checks**: removing `dispute.open` from `WRITE` failed the contract test; bypassing the write gate failed the refusal
  and the warned-uncertain tests. Both restored.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` 123 + 16 passed (8 regtest ignored), build. Also seen: one run of
  `a_run_given_extra_time_outlasts_the_base_timeout_and_no_more` failed under load and passed three times on its own. It is a timing
  test this change does not touch.

## Follow-up: Rating The Counterparty (Issue #10): 2026-11-12

[Spec 002](../specs/002-rate-counterparty/spec.md). Native Linux, cargo 1.97, on top of `main` `0e01da4`.

- **Tool** (`src/tools/rate_counterparty.rs`, `server.rs`): `rate_counterparty` runs `order rate --order=<uuid> --rating=<1-5> --yes`
  behind the write gate; a timeout or unreadable output carries the no-retry warning. Annotations: not read-only, not destructive, not
  idempotent, open world. `contract::WRITE` requires `order.rate`. The descriptions of `rate_counterparty` and `list_messages` and the
  instructions are updated (the user's own rating; the `next_step` values of mostro-cli 0.17.1, read from its source at the tag).
- **Fixtures**: the four `order.rate` examples of spec `014-order-rate` at `v0.17.1`. Before they were added, `refresh.py` reproduced
  the current `outcomes.json` byte for byte (the `schema` input taken from the current fixture).
- **Tests**: written first and run red: the unit tests on a `todo!()` (4 panicked; the typed-deserialization test passed, as `u8`
  already refuses those values), the server tests (no `rate_counterparty` method), the stdio tests (6 failed: unknown tool, sixteen
  tools). Unit: the exact argv, a lowercased id, 1 and 5 accepted, 0, 6 and 255 refused, a bad id, -1 / 256 / 4.5 / "5" refused by
  the type. Server: refused by default without spawning, runs `order rate`, the annotations and description, the CLI's document passed
  through unchanged, out-of-range and bad-id refusals that spawn nothing, a refusal keeping `invalid_order_status` without the warning,
  a warned `uncertain` and timeout while a timed-out read is not warned, the instructions, the `list_messages` description. Contract:
  `order.rate` required with writes on only. Stdio: discovery of seventeen tools and the `rate_counterparty` schema, the whole-trade
  argv, bad ratings refused before the CLI runs, and the rated, refused and uncertain documents reaching the agent.
- **Mutation spot checks**: accepting 0 (`0..=5`) failed two unit tests and the stdio whole-trade test; removing `order.rate` from
  `WRITE` failed the contract test; running the tool as a local write failed the warned-timeout test. All restored, tests green.
- **Regtest**: `both_sides_rate_a_finished_trade_once_and_a_running_one_is_refused` is written and **not run** (no Docker daemon in
  this environment). The real CLI was not run either.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` 135 + 17 passed (9 regtest ignored), build, `git diff --check`.
- **Review fixes (2026-11-12)**: the `rating` schema advertised `minimum` 0 and `maximum` 255 (a plain `u8`), against the 1-to-5 rule;
  it now carries `#[schemars(range(min = 1, max = 5))]`, so it advertises 1 and 5 (seen in `tools/list` of the built binary). The
  stdio discovery test asserts both bounds; it failed first on `minimum` 0, and with `max = 6` or without `min` it failed again
  (restored). The description now gives `reply_timeout` as one example of `uncertain`, not the only one. Gates: fmt, clippy
  `-D warnings`, `cargo test` 135 + 17 passed (9 regtest ignored), build, `git diff --check`. The regtest and the real CLI were still
  not run.

## Follow-up: Review Fixes For Rating (PR #16): 2026-11-12

Static review by the maintainer, checked here against mostrod `v0.20.2` and mostro-cli `v0.17.1` source.

- **Guidance** (`server.rs`): the instructions now hold one set of message rules for list_messages and watch_order, and both
  descriptions repeat them. A watch's first call replays 30 minutes, so each `event_id` is acted on once. Each step has its own valid
  statuses: `add_invoice` in `waiting-buyer-invoice`, and in `settled-hold-invoice` after a failed payout (`add_invoice.rs:83`);
  `fiat_sent` only while `active`; `release` only while `fiat-sent` and on the user's word. A null or unknown `next_step` is read
  from `action` (`cooperative-cancel-initiated-by-peer` has none). Rating goes by the order's status, not a `rate` message: a
  solver's settlement sends the seller only `admin-settled`, and a watch can end before the buyer's `rate` (10 s grace, cut at the
  deadline). The rate_counterparty outcomes say what each one proves. An untaken order is silent, not `invalid_order_status`
  (`prepare_variables_for_vote`, internal errors only logged). A full-privacy counterparty is silent with nothing recorded, and a
  failed rating event after the commit is silent with the rating stored. A missed PoW probe ends `uncertain` / `pow_required`.
  README, spec 002 and the fixtures README are corrected to match: PoW is `max(POW, advertised)`, and the `proposed` caveat is
  dropped because all 16 v0.17.1 example files are `proposed`.
- **`rating` of `5.0`**: reproduced first: rmcp refused it ("invalid type: floating point"). `rating` now reads a whole number written
  as an integer or a float with no fraction. Its schema stays integer 1 to 5. `5.5`, `"5"`, `256.0` and negatives are refused, and
  `0.0` / `6.0` are refused by the 1-to-5 check. Seen on the built binary with a logging stand-in CLI: `5.0` ran `--rating=5`, and the
  refused calls started no CLI. Other integer parameters of the server are unchanged.
- **Tests**: written red first (five unit tests and the stdio whole-trade test). The `!required(false).contains("order.rate")`
  assertion was vacuous next to the `assert_eq` on `required(false)` and is removed, so the earlier "required with writes on only"
  entry rested on that `assert_eq`. The duplicated stdio rating test became two table rows. The regtest's copy of the end of
  `trade()` is now a shared `until_success`; it compiles, and was not run.
- **Mutation checks**: accepting fractions, dropping the whole-number reader, dropping `order.rate` from `WRITE`, moving it to `READ`,
  and dropping the `event_id` rule from watch_order each failed a test. All restored.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` 137 + 16 passed (9 regtest ignored), build, `git diff --check`. Not run: the
  regtest (no Docker daemon here) and a real mostro-cli binary. No test covers a seller rating after a solver's settlement on a node.

## Follow-up: Second Review Pass On Rating (PR #16): 2026-11-12

On top of `upstream/main` `8f8ad46`, merged with no conflicts (release workflow and install docs). Checked against mostro-cli
`v0.17.1` and mostrod `v0.20.2` source.

- **Message ids**: chat records carry `message_id` and no `event_id` (CLI `outcome/chat.rs`), yet the instructions said to act on
  each watch record by `event_id`. Now node messages go by `event_id` and chat messages by `message_id`, in the instructions, the
  watch_order description, the README and spec 002, and skipping a seen record never skips a newer one. Tests: a watch result and a
  chat read keep the CLI's document whole (every record, with its own id kind), and the descriptions name both ids.
- **Rating uncertainty**: the generic warning ("check the current state, then decide") and the generic timeout text ("do not retry
  before checking get_order") invite a retry that a rating cannot justify. `rate_counterparty` now uses its own warning
  (`outcome::RATING_UNCERTAIN_WARNING`): never call it again for that order, get_order does not show ratings, a missing
  `rate-received` proves nothing, report only that the outcome is unknown. It applies to the CLI's `uncertain` outcomes (the
  document unchanged), a timeout and unreadable output. Every other write keeps the generic warning. `reply_malformed` has no CLI
  example for `order.rate`, so its test changes the code of the `uncertain` example and is labelled so.
- **Tests that could pass falsely**: the rating rows of the stdio substring table are replaced by a test of the result's error flag
  and structured content (status, `order_id`, rating, `node_refused` / `invalid_order_status`, `reply_timeout`, the rating's
  warning), on one server. The timed-out-rating test now also checks that no document came and that the text is the rating's
  timeout, not a parse failure.
- **Regtest, written and not run** (no Docker daemon here): an untaken order is `uncertain` / `reply_timeout` and a taken one in
  another status `node_refused`; Bob rates after his watch has ended; a seller rates after a solver settles the dispute
  (`admtakedispute`, `admsettle` with the node's public test key from `regtest/mostrod/settings.toml` as `ADMIN_NSEC`), with
  `settled-hold-invoice` or `success`, `admin-settled` and no `rate` asserted. Not reproducible here: a full-privacy counterparty
  (no switch for it in the commands this server wraps) and a missed PoW probe (timing).
- **Mutation checks**: routing the rating through the generic write path failed four unit tests and the stdio rating test; ignoring
  the warning argument failed two; the generic timeout text for a rating failed one; dropping the `message_id` rule from
  watch_order failed one. All restored.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` 143 + 17 passed (10 regtest ignored), build, `git diff --check`. Not run: the
  regtest and a real mostro-cli binary.

## Follow-up: Third Review Pass On Rating (PR #16): 2026-11-13

On top of `upstream/main` `3966bae` (AGENTS.md), merged with no conflicts. From the CodeRabbit review of `a1fc799`.

- **Wording**: an `uncertain` rating does not prove publication (`request_rejected` has publication `unknown`). The rating's
  warning, the tool description, the README and spec 002 now say the rating may have been sent, but whether it was recorded is
  unknown, and never to call `rate_counterparty` again for that order and side. Tests also check that "it was sent and" is gone.
- **Failure after start**: `run_waiting` returned `Error::Io` both for a failed `spawn` and for a failure after the CLI started
  (output not captured, a read, `wait`). The latter is now `Error::IoAfterStart`, with the same text. For a rating it carries the
  rating's warning; a spawn failure (`Io`, `CliNotFound`) stays a plain error, since nothing was sent. Every other tool shows the same
  text as before. Tests: the server for both kinds and for two other tools; the backend for a non-executable CLI (`Io`). Not tested:
  the backend's mapping of a real read or wait failure, which cannot be induced here.
- **Chat records**: the chat-read test also checks that distinct `message_id`s stay distinct records. The README says the server
  passes records through and leaves telling them apart to the agent.
- **Mutation checks**: dropping the rating's after-start arm failed one test; the old wording failed five; classifying a spawn
  failure as after-start failed the backend test. All restored.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` 147 + 17 passed (10 regtest ignored), `cargo build`, `cargo build --release`,
  `git diff --check`. Not run: the Windows check of AGENTS.md (target not installed; no Unix API added), the regtest (no Docker
  daemon), a real mostro-cli binary.

## Follow-up: Exact Rating Acknowledgement (PR #16): 2026-11-13

From CodeRabbit's review of `96046b3`. mostro-cli `v0.17.1` accepts as the acknowledgement only `rate-received` for the same
order carrying `Payload::RatingUser(r)` with `r` the rating sent (`outcome/rate.rs`). Its `message list` shows that payload as
`other` / `rating_user` without the score (`outcome/messages/payload.rs`; the CLI's own test maps `RatingUser(5)` that way).

- **Guidance**: the instructions, the rate_counterparty description, the README and spec 002 now say that only the tool's `ok`
  proves the exact rating, checked by the CLI, which this server passes on without checking anything itself. A `rate-received`
  record in list_messages shows a rating from this side was recorded, not which one. A missing acknowledgement, a missing rating
  payload or a mismatched rating confirms nothing. Code behaviour is unchanged.
- **Tests**: `the_instructions_say_only_an_exact_acknowledgement_confirms_a_rating` and
  `the_rating_description_says_only_an_exact_acknowledgement_confirms_it` check the rule and the absence of the old blanket claims.
  Both were written first and failed. Mutation: putting the old instructions sentence back failed the first; restored.
- **Gates**: fmt, clippy `-D warnings`, `cargo test` 149 + 17 passed (10 regtest ignored), `cargo build`, `cargo build --release`,
  `git diff --check`. Not run: the regtest (no Docker daemon), the Windows check (target not installed), a real mostro-cli binary.

## Follow-up: Another Attempt After An Uncertain Rating (PR #16): 2026-11-13

**Superseded by "Follow-up: Never Resend An Uncertain Rating" below.** The retry this entry allowed was withdrawn on the
maintainer's ruling; what it records about the regtest run and the node's database is still evidence, for mostrod v0.20.2 only.

From the maintainer's review of `eab9dbc` (the open thread on "never resend"). Base `upstream/main` `3966bae`, unchanged since the
last merge, so nothing was merged. Checked against mostrod `v0.20.2` and mostro-cli `v0.17.1` source.

- **The node's guarantee, read in source**: `rate_user.rs` returns early when the sender's flag is already set, then claims it with
  `db.rs` `claim_order_rating_flag` (`UPDATE orders SET buyer_sent_rate = 1 WHERE id = ? AND buyer_sent_rate = 0 AND status = ?`,
  the seller's alike) in the same transaction as the score, and commits the score only when the claim won. A second rating from the
  same side is never counted, never replaces the first, and gets no answer. So the blanket "never call rate_counterparty again for
  this order and side" had no safety basis.
- **Guidance** (`outcome.rs`, `server.rs`): the rating's warning now says not to call `rate_counterparty` again on the agent's own;
  another attempt can never count twice but is usually pointless (the node answers a repeat, a full-privacy counterparty and an
  untaken order with the same silence), so it is made only when the user asks. `uncertain` with `pow_required` gets its own warning
  (`outcome::rating_uncertain_warning`): the node most likely dropped the rating for too little work (not proven); it names
  `error.required` and `error.configured` when they are bit counts (0 to 255, anything else is not echoed), and allows one more
  attempt, with the user's explicit go-ahead, once the server runs with `POW` at least `error.required`; if that one is uncertain
  too, stop. Nothing is retried by the server. Timeouts, unreadable output and failures after start keep the general rating
  warning (no document, so no `pow_required` to read). `error.rs` is unchanged: the rating already overrides its timeout text.
- **Tests, red first** (10 failed before the change): the POW warning with numbers, without them or with non-bit values, only on
  `uncertain` (an `error` `pow_required` is a plain error), the other uncertain codes keep the general warning, the description and
  instructions drop the blanket ban, and the stdio rating test covers `uncertain` / `pow_required` end to end. No CLI example of
  `uncertain` / `pow_required` exists at v0.17.1; the tests build it from the `rate_uncertain` example with the error the CLI's
  `ErrorInfo::with_work` produces, and say so. The fake runner now takes a closure so those cases can be built.
- **Weak assertion found and fixed**: the `rate` needle of the list_messages / watch_order test matched any sentence naming a `rate`
  message; it now matches the step list itself, and the instructions' `rate` step is pinned. Rewriting the list to drop the step
  while keeping the word `rate` failed it.
- **Mutation checks**: the server ignoring the POW selector failed two tests; dropping the `uncertain` check failed one; dropping the
  0-255 bound failed one; the blanket ban restored failed six; dropping "one more attempt" failed one; dropping the `rate` step from
  the list or the instructions failed one each; dropping the whole-number reader failed one. All restored. Not pinned by any test:
  `cli_backend.rs` mapping a real read or `wait` failure to `Error::IoAfterStart` (it cannot be induced here; changing it to
  `Error::Io` left every test green).
- **Regtest, run for the first time on this PR**: Docker daemon started locally; mostrod `v0.20.2` image built from
  `regtest/mostrod/Dockerfile` with its pinned SHA-256 checked (the build container was given this environment's proxy CA through
  the local `debian:bookworm-slim` base; no repository file changed); mostro-cli `v0.17.1` built from its tag. `cargo test --test
  regtest_e2e -- --ignored --test-threads=1 --skip loses_the_bond`: 9 passed, including both rating tests. With `MOSTRO_BONDS=1
  regtest/devenv.sh up`, the bond test: 1 passed. Afterwards, mostrod's database: Bob's identity holds `total_reviews` 2, `min_rating`
  4, `max_rating` 5 (Alice's 5 and 4); her second rating of the first order (a 1) was not counted. Not reproduced: a full-privacy
  counterparty and a missed PoW probe.
- **Gates**: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 154 unit + 17 stdio passed
  (10 regtest ignored by default; run above), `cargo build`, `cargo build --release`, `git diff --check`. Not run: the Windows check
  of AGENTS.md (target not installed; no Unix API added).

## Follow-up: Never Resend An Uncertain Rating (PR #16): 2026-11-14

From the maintainer's ruling on issue #10 (2026-11-13, 20:24 UTC): an uncertain rating is never resent, and no minimum mostrod
version is added. Rule 5 of `AGENTS.md` (no blind retries) has no exception that holds only on some node versions, and a rating
counted twice would corrupt the counterparty's reputation. This supersedes the policy of the previous entry. Base `upstream/main`
`3966bae`, unchanged when fetched on 2026-11-14; the branch head before this change was `563dd8f`.

- **Why no node rule**, read in source: mostrod `v0.18.0` `rate_user.rs` checks the order's side flag as loaded, then calls
  `update_user_rating` and `update_user_rating_event`, with no conditional claim and no transaction; `v0.20.2` begins a transaction,
  runs `claim_order_rating_flag`, applies the score and commits. This server pins no mostrod version, so it cannot promise that a
  second attempt counts once. Not run against `v0.18.0`.
- **Guidance** (`outcome.rs`, `server.rs`, README, spec 002 FR-003 and FR-007): the rating's warning, the `uncertain` /
  `pow_required` warning, the server instructions and the `rate_counterparty` description say to never call `rate_counterparty`
  again for that order and side, not even if the user asks. The user-approved retry, the `pow_required` retry and the "harmless",
  "usually pointless" and "can never count twice" claims are gone. Kept: the rating may have been sent but whether it was recorded
  is unknown; `get_order` shows no ratings; a missing `rate-received` proves nothing; `uncertain` / `pow_required` names
  `error.required` and `error.configured` (bit counts only), now so `POW` can be raised for future requests. The server retries
  nothing. The regtest's second rating from the same side stays as a test-only probe of the pinned v0.20.2 node, with a comment
  saying so.
- **Tests, red first**: `RETRY_PERMISSIONS` (twelve phrases) and the rule `not even if the user asks` are checked in the rating's
  warning, the `pow_required` warning (with and without bit counts), the instructions, the tool description and the stdio rating
  test. Against the old code 10 of the 154 unit tests failed; the stdio rating test was edited in the same step and was not run red
  on its own. Four tests were renamed or rewritten, none deleted.
- **Mutation checks** (in a scratch copy, restored after each): "another attempt is harmless" added to the general warning failed 6
  unit tests and the stdio test; "one more attempt" back in the `pow_required` warning failed 1 and the stdio test; "unless the user
  asks" in the instructions failed 1; "go-ahead" in the description failed 1; the general warning's rule weakened failed 6 and the
  stdio test; the `pow_required` warning's rule weakened failed 2 and the stdio test. My first attempt at the fifth did not apply
  (its anchor matched twice, and the script refused it), so it showed a false survivor; it was redone with a unique anchor.
- **Environment** (new for this PR): WSL2 Ubuntu 24.04, rustc and cargo 1.100.0 with clippy and rustfmt, Docker Desktop 29.8.1 with
  Compose v5.5.1 and its WSL integration enabled for the distro, `mostro-cli` 0.17.1 built from tag `v0.17.1` (commit `0665895`),
  `jq` 1.8.2. The Windows tree has CRLF line endings, so the Linux gates and the regtest ran from a one-way LF copy of it.
- **Regtest, run on the final code**: mostrod `v0.20.2` from `regtest/mostrod/Dockerfile` (its pinned SHA-256 is checked in the
  build). `cargo test --test regtest_e2e -- --ignored --test-threads=1 --skip loses_the_bond`: 9 passed in 103 s, including
  `both_sides_rate_a_finished_trade_once_and_a_running_one_is_refused` and `the_seller_rates_after_a_solver_settles_the_dispute`.
  With `MOSTRO_BONDS=1`, `loses_the_bond`: 1 passed in 222 s. Afterwards, mostrod's database: both rated identities hold
  `min_rating` 4, so the second rating (a 1) of the same side was not counted on this node; one order has both rating flags set,
  and one only the seller's. One setup problem, not a test failure: under Docker Desktop's WSL integration, `MOSTRO_BONDS=1
  regtest/devenv.sh up` could not restart the node, because `write_settings` replaces `.state/mostrod.toml` with `mv` and the
  container's single-file bind mount then pointed at the old file. Removing only the node container and running `up` again fixed
  it; no repository file changed. Stopped afterwards with `devenv.sh down`.
- **Gates**: on Linux, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 154 unit + 17 stdio
  passed (10 regtest ignored by default; run above), `cargo build`, `cargo build --release`; `git diff --check`; and on Windows
  (rustc 1.95.0, host `x86_64-pc-windows-msvc`) `cargo check --bin mostro-mcp`, the Windows check of AGENTS.md. Not run: `cargo test`
  natively on Windows (the fake-CLI tests use `/bin/sh` and `std::os::unix`, as on `upstream/main`: at `563dd8f`, 145 of the 154
  unit tests passed there and the stdio tests did not compile), a full-privacy counterparty and a missed PoW probe on the regtest,
  and any node older than `v0.20.2`.
