# Feature Specification: Drive The JSON CLI From MCP

**Feature**: `001-json-migration`

**Working Branch**: `main` (feature directory naming does not change Git branches)

**Created**: 2026-11-04

**Status**: Draft, awaiting maintainer review. Progress is tracked only in [tasks.md](tasks.md)

**Input**: `mostro-cli` was refactored (specs 001-008 in its repository) into commands that answer with one versioned JSON outcome
(`--output json`): `order list`, `order info`, `order new`, `order take-sell|take-buy`, `order add-invoice`, `bond add-invoice`,
`order fiat-sent`, `order release`, `order cancel`, `order sync`, `message list` and `schema`. The old spellings (`listorders`, `ordersinfo`,
`neworder`, `takesell`, `takebuy`, `--kind`, `--fiatcode`, ...) were removed with no alias, so all four tools of this server fail today. This
feature moves the server to the new contract, then lets an agent finish a whole trade.

## Findings

- The server spawns the CLI per call and returns its stdout text, ANSI-stripped and secret-redacted, truncated at 64 KiB; it runs one CLI at a
  time (the CLI database and trade-key index are shared) and refuses state-changing tools unless `MOSTRO_MCP_ALLOW_WRITES` is on.
- In JSON mode the CLI prints exactly one document on stdout for success **and** failure (`status` `ok`, `error` or `uncertain`, `command`, `data`,
  `error.code`/`reason`, `warnings`, `meta`); exit 0 ok, 2 invalid arguments, 1 anything else. The write commands need `--yes` in JSON mode and never
  read stdin.
- `uncertain` means the request may have reached the node: it must never be retried blindly. The text-output server could not tell this apart.
- The CLI publishes its own contract: `mostro-cli --output json schema` lists every command with `output_modes`, an input schema and an output schema.
- The CLI never pays an invoice. The user pays in their own wallet; `message list` returns what the node asks, with any invoice to pay as data.

## Clarifications

### Session 2026-11-04 (defaults, open to veto)

- Q: Scope and order? -> A: First migrate the four existing tools so the server works again (slices 1-3), then add the tools that complete a trade
  (slices 4-5): `list_messages`, `add_invoice`, `fiat_sent`, `release`, `cancel`, `sync_orders` (and `add_bond_invoice`). Each group is reviewed on its own.
- Q: What does a tool return? -> A: The CLI's JSON outcome, redacted (FR-006), as the text block (compact, serialized from the parsed
  document), plus the same document as MCP structured content when the SDK supports it. `status: ok` is a successful tool result; `error` is a tool
  error (`isError`); `uncertain` is a tool error whose text block starts with a fixed warning not to retry and to check the state first (as FR-001
  describes), followed by a blank line and the document; its structured content is the document alone, unchanged.
- Q: How does the server know the CLI fits? -> A: At startup it runs `schema` once and refuses to start, naming the missing commands and the CLI's
  `--version`, unless every command the enabled tools need is listed with `json` among its `output_modes`. The README stops pinning a commit.
- Q: Confirmation? -> A: The server always passes `--yes` (the confirmation is the host's tool-approval prompt plus `MOSTRO_MCP_ALLOW_WRITES`, refused
  before spawning as today). `release` also requires the parameter `fiat_received: true`: the server refuses without it, so an agent must state it.
- Q: Which tools write? -> A: `create_order`, `take_order`, `add_invoice`, `add_bond_invoice`, `fiat_sent`, `release`, `cancel` and `sync_orders`
  (it saves rows locally). `list_orders`, `get_order` and `list_messages` are read-only.
- Q: Identity? -> A: One identity per server, as today (`MCLI_DIR`). The CLI never creates one: a missing identity is the CLI's `identity_missing`,
  returned as an error that tells the user to create it with the CLI first.

## Scope

Tools, backend, contract check, output mapping, tests, README. Out of scope: paying invoices, creating identities, disputes, `rate` and the admin
commands (not migrated in the CLI), a multi-user or per-call identity, changing the CLI. Later, outside this spec: `open_dispute`
over `dispute open` (issue #14; the dispute chat and list stay out while the CLI prints them only for humans), and `rate_counterparty`
over `order rate` ([spec 002](../002-rate-counterparty/spec.md), issue #10).

## Requirements

- **FR-001**: The backend runs the CLI with `--output json` for every tool, parses stdout as one outcome document, and never trusts the exit code alone.
  Output that is not exactly one document (including stdout over the size limit, which is refused whole, never cut) is a tool error with a redacted,
  truncated excerpt. For a state-changing tool that error starts with the same no-retry warning as `uncertain`: the CLI ran, so the request may have
  reached the node. The agent checks the state first: `get_order` when it has the order id, `list_orders` when it does not (an uncertain or unreadable
  `create_order`), `list_messages` once slice 4 adds it. An order missing from the book does not prove the write failed; when the state stays unclear,
  the user reconciles it by hand before anything is retried.
- **FR-002**: Every tool maps to its canonical command (`order list`, `order info`, `order new`, `order take-sell|take-buy`, `order add-invoice`,
  `bond add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order sync`, `message list`), with arguments validated before the CLI starts and
  passed as `--flag=value` so a value can never become a flag.
- **FR-003**: Outcome mapping as clarified above: `ok` success, `error` tool error, `uncertain` tool error with the no-retry warning first in the text;
  warnings are kept in the document; the structured content is the document, and the text is that same document (after the warning, for `uncertain`).
- **FR-004**: State-changing tools are refused before any process starts unless writes are enabled; `release` also needs `fiat_received: true`.
- **FR-005**: Startup contract check through `schema`, as clarified.
- **FR-006**: The environment of the CLI stays cleared except the node configuration and the fixed allow-list; output is redacted of `nsec1...` twice:
  the raw text before it is parsed (so an excerpt of unreadable output is clean) and every string of the parsed document (a JSON escape such as
  `\u006e` hides the prefix from the raw text), so the text and the structured content carry the same redacted document; one CLI run at a time; a
  per-call timeout that reports possible delivery.
- **FR-007**: Tool descriptions state what is state-changing, that sats are never paid by the tools, what `uncertain` means, and name the next tool to
  call (for example `list_messages` after a take).
- **FR-008**: The README documents the tools, the CLI requirement (a `schema` that lists the commands), the flow of a whole trade and the security
  properties.

## Success Criteria

- **SC-001**: With a scripted CLI printing the CLI's own example documents, every tool returns the right result for `ok`, `error` and `uncertain`, and
  the arguments reaching the CLI are exactly the expected vector.
- **SC-002**: A disabled server never spawns the CLI for a writing tool; `release` without `fiat_received` never spawns it either.
- **SC-003**: A CLI without a needed command makes the server exit at startup with a message naming it.
- **SC-004**: No secret appears in any tool result or log line; truncation never produces a half document.
- **SC-005**: Nothing about a real node is claimed until a separately authorized regtest run of the e2e test is recorded.

## Assumptions And Open Items For The Plan

- The SDK (`rmcp 3.5.x`) supports structured content in tool results; if not, text only and the plan says so.
- The e2e test (`tests/regtest_e2e.rs`) and the regtest Docker setup need the new CLI; they run only with authorization.
- Example documents for the scripted CLI are copied from the CLI repository's `specs/*/contracts/examples.json`, with their provenance noted.
