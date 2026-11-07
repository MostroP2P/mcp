# Tasks: Drive The JSON CLI From MCP

**Design**: [spec](spec.md), [plan](plan.md).

**Mode**: lean, as for the CLI refactor: slices tracked only here, one short evidence entry per slice in `docs/TEST_SUMMARY.md`, stop after each slice for
review. Tests before code, a real failure before a repair, mutation spot checks. Gates per slice: `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test` (check for FAILED lines), `cargo build`. No push, public-node use or regtest without
authorization.

## Slices

- [x] **1a. Backend and outcome mapping.** (done 2026-11-04; evidence in `docs/TEST_SUMMARY.md`) Fixtures with the CLI's example documents; the runner returns a parsed outcome (`--output json`, one
  document, redacted as raw text and again in its parsed strings, refused whole over the size limit); the outcome mapping (`ok`, `error`,
  `uncertain`, unreadable). The no-retry warning on unreadable output of a state-changing tool comes with the write tools (slice 3).
  Unit tests; the scripted CLI for the stdio tests.
- [ ] **1b. Startup contract check.** `schema` once at startup; stop naming the missing commands; old CLI refused.
- [ ] **2. Read tools.** `list_orders` -> `order list`, `get_order` -> `order info`; params, `args`, descriptions, stdio tests for the three statuses.
- [ ] **3. Write tools.** `create_order` -> `order new --yes`, `take_order` -> `order take-sell|take-buy --yes`; the write gate unchanged; stdio tests
  including the refusals before spawning and the `uncertain` warning.
- [ ] **4. The tools that finish a trade.** `list_messages`, `add_invoice`, `add_bond_invoice`, `fiat_sent`, `release` (with `fiat_received`), `cancel`,
  `sync_orders`; tests as above.
- [ ] **5. Docs, e2e and verification.** README, the whole-trade walk-through, the e2e test updated (run only with authorization), evidence table, status lines.

## Requirement ownership

| Requirement | Slice |
| --- | --- |
| FR-001, FR-003, FR-005, FR-006 | 1 |
| FR-002, FR-007 | 2, 3, 4 |
| FR-004 | 3, 4 |
| FR-008 | 5 |
| SC-001 to SC-004 | 1 to 4 |
| SC-005 | 5 |
