# Feature Specification: Rate The Counterparty

**Feature**: `002-rate-counterparty`

**Created**: 2026-11-12

**Status**: Implemented; unit and stdio tests pass. The regtest test is written and has not been run. Evidence in
[docs/TEST_SUMMARY.md](../../docs/TEST_SUMMARY.md).

**Input**: issue #10. [Spec 001](../001-json-migration/spec.md) left `rate` out of scope. mostro-cli 0.17.1, the version this server
already requires, offers `order rate --order UUID --rating 1-5 --yes` in JSON (its spec `014-order-rate`). The maintainer agreed to a
standalone module, conservative annotations and no extra confirmation parameter.

## Findings

- mostrod v0.20.2 (`src/app/rate_user.rs`) accepts a rating once the order is `success`, or `settled-hold-invoice` when the seller rates.
  Both a release and a solver's settlement (`src/app/admin_settle.rs`) set `settled-hold-invoice`. A taken order in any other status
  is `cant-do` `invalid_order_status`. No answer at all (internal errors are only logged, `src/app.rs` `manage_errors`): an order
  never taken (`prepare_variables_for_vote` lacks a party key, before the status check), a side that already rated, a full-privacy
  counterparty, a stranger, a failed database step before the commit, and a failed rating event after it (the rating is stored).
- The node queues `rate` only after a release (to the seller) and a successful buyer payout (`release.rs`); a solver's settlement
  sends the seller only `admin-settled`. The CLI's watch waits for the buyer's `rate` up to 10 s, never past the deadline, so it can
  report `ended` without it. Eligibility therefore comes from the order's status, not from a `rate` message.
- The CLI signs with the order's trade key, waits for `rate-received` with the rating it sent, and writes nothing locally. Silence is
  `uncertain` / `reply_timeout`. No command reads a rating back. `message list` lists every message the node sent to the order's trade
  key, so a `rate-received` there confirms a rating; its absence proves nothing.
- At 0.17.1 every request mines the larger of `POW` and what the node advertises, on its own up to 20 bits (spec `017-advertised-pow`,
  `node_info.rs` `work_to_mine`; a rating uses the first-contact lane); above that it is `pow_required`, nothing sent. When the early
  probe misses, it mines `POW` alone, the node drops the event silently and a late probe makes the timeout `uncertain` /
  `pow_required` with publication `published` (`exchange.rs`). The 0.17.0 caveat (a rating failing on a node
  that asks for proof of work unless `POW` is set) does not apply to 0.17.1.
- At 0.17.1 `message list` reports `next_step` `pay_hold_invoice`, `pay_bond_invoice`, `add_invoice`, `add_bond_invoice`,
  `fiat_sent`, `release` and `rate` (`src/outcome/messages/next_step.rs` at tag `v0.17.1`), and null otherwise. 0.17.0 had the first
  four only. The CLI book still calls the last three unreleased. Null means this CLI maps no step to the action (`_ => None`), not that
  nothing is asked: `cooperative-cancel-initiated-by-peer` has no step but waits for the user's answer. Each step is valid in its own
  statuses: `add_invoice` in `waiting-buyer-invoice`, and in `settled-hold-invoice` after a failed payout (`add_invoice.rs`).

## Requirements

- **FR-001**: `rate_counterparty(order_id, rating)` runs `order rate --order=<canonical uuid> --rating=<n> --yes` behind the write
  gate. `order_id` goes through the shared validator; `rating` is a `u8` refused outside 1 to 5 before the CLI starts.
- **FR-002**: Annotations: title `Rate counterparty`, not read-only, not destructive, not idempotent, open world. The outcome is the
  CLI's document, unchanged; a timeout or unreadable output carries the no-retry warning.
- **FR-003**: The description and the instructions say the rating is the user's choice, never the agent's, never sent unasked; one
  rating per side per order, final; who may rate when, from the order's status and without waiting for a `rate` message; what each
  outcome proves (`ok` recorded, `error` nothing recorded, `uncertain` unknown, with the silent cases); after an `uncertain` rating,
  a timeout, unreadable output or a CLI that started and then failed (`Error::IoAfterStart`), never call `rate_counterparty` again
  for that order and side, and say the rating may have been sent but whether it was recorded is unknown, under a warning of its own
  (the generic one says to check the state, which no tool can for a rating); a CLI that never started sent nothing; `get_order` does not show ratings and `rate-received` in
  `list_messages` confirms one, its absence proving nothing.
- **FR-004**: `contract::WRITE` requires `order.rate`.
- **FR-005**: `list_messages` and `watch_order` give the same message rules: the `next_step` values of the findings, each step's
  valid statuses in the instructions, act on each node message (`event_id`) once and read each chat message (`message_id`; chat
  records have no `event_id`) once (the first watch replays 30 minutes), and a null or unknown
  `next_step` read from `action`. `rating` also accepts a whole number written as a float (`5.0`), as JSON Schema's `integer` does.
- **FR-006**: The four `order.rate` examples of spec `014-order-rate` are fixtures; README and test summary updated.

## Slice

- [x] **1. Tool, contract, descriptions, tests, docs.** (2026-11-12) Regtest written, not run: no Docker daemon in the environment
  where it was implemented.
- [x] **2. Review fixes (PR #16).** (2026-11-12) The guidance above, the outcome table, a `rating` of `5.0`, a vacuous contract
  assertion removed, duplicated test code shared. Regtest still not run.
- [x] **3. Second review pass.** (2026-11-12) `event_id` for node messages, `message_id` for chat; the rating's own uncertain
  warning (`outcome::RATING_UNCERTAIN_WARNING`) on uncertain outcomes, timeouts and unreadable output; rating outcomes checked by
  their structure; regtest tests for an untaken order and a solver's settlement, written, not run.
- [x] **4. Third review pass.** (2026-11-13) An `uncertain` rating "may have been sent", not "was sent"; a failure after the CLI
  started (`Error::IoAfterStart`) carries the rating's warning, a spawn failure does not; distinct chat `message_id`s kept.
