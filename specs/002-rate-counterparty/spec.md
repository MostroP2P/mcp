# Feature Specification: Rate The Counterparty

**Feature**: `002-rate-counterparty`

**Created**: 2026-11-12

**Status**: Implemented; unit, stdio and regtest tests pass (regtest first run 2026-11-13, mostrod v0.20.2, mostro-cli v0.17.1). Evidence in
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
  `uncertain` / `reply_timeout`. No command reads a rating back. `order rate` accepts only `rate-received` for the same order with
  `Payload::RatingUser(r)` where `r` is the rating sent (`outcome/rate.rs`), so its `ok` alone proves the exact rating. `message list`
  lists every message the node sent to the order's trade key, but shows a `RatingUser` payload as `other` / `rating_user` without the
  score (`outcome/messages/payload.rs`): a `rate-received` there shows a rating was recorded, not which one. A missing
  acknowledgement, a missing rating payload or a mismatched rating confirms nothing, and the server checks none of it itself.
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
  a timeout, unreadable output or a CLI that started and then failed (`Error::IoAfterStart`), say the rating may have been sent but
  whether it was recorded is unknown, under a warning of its own (the generic one says to check the state, which no tool can for a
  rating), and never call `rate_counterparty` again for that order and side, not even if the user asks (FR-007); `uncertain` with
  `pow_required` gets its own warning (FR-007); a CLI that never started sent nothing; `get_order`
  does not show ratings; only the tool's `ok` proves the exact rating, and a `rate-received` record in `list_messages` shows a
  rating was recorded but not which one (a missing acknowledgement, a missing rating payload or a mismatched rating confirms
  nothing).
- **FR-004**: `contract::WRITE` requires `order.rate`.
- **FR-005**: `list_messages` and `watch_order` give the same message rules: the `next_step` values of the findings, each step's
  valid statuses in the instructions, act on each node message (`event_id`) once and read each chat message (`message_id`; chat
  records have no `event_id`) once (the first watch replays 30 minutes), and a null or unknown
  `next_step` read from `action`. `rating` also accepts a whole number written as a float (`5.0`), as JSON Schema's `integer` does.
- **FR-006**: The four `order.rate` examples of spec `014-order-rate` are fixtures; README and test summary updated.
- **FR-007**: An uncertain rating is never resent (the maintainer's ruling on issue #10, 2026-11-13, by rule 5 of `AGENTS.md`). After
  an `uncertain` rating, a timeout, unreadable output or a CLI that started and then failed, no text the server gives an agent says
  that another attempt is allowed, harmless, or the user's to approve: the warning, the instructions and the description say to never
  call `rate_counterparty` again for that order and side, not even if the user asks. Why: mostrod versions differ in how they guard
  against a duplicate rating. v0.18.0 (`rate_user.rs`) checks the order's side flag and then writes the score, with no conditional
  claim; v0.20.2 claims the flag and writes the score in one transaction (`db.rs` `claim_order_rating_flag`). This server pins no node
  version and adds no minimum, so it cannot promise that a second attempt would count once. For `uncertain` with `pow_required`
  (published, then a late read of the node's advertisement asks for more work than was mined) the warning says the node most likely
  dropped the rating unread (not proven) and names `error.required` and `error.configured` when they are bit counts (0 to 255;
  anything else is not echoed), so the user can raise `POW` for future requests (`POW` is read at startup); it still does not let
  this rating be sent again. This server never retries anything itself. Regression tests fail on any retry-permission phrase
  (`RETRY_PERMISSIONS` in `src/server/tests.rs`) in the warnings, the instructions or the description.

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
- [x] **5. Exact acknowledgement.** (2026-11-13) Only the tool's `ok` proves the exact rating (same order, same rating, checked by
  the CLI); a `rate-received` record in `list_messages` has no score, so it shows a rating was recorded, not which one.
- [x] **6. Another attempt after an uncertain rating.** (2026-11-13) **Superseded by slice 7: the retry it allowed is withdrawn.** The
  blanket "never call it again for this order and side" was replaced after checking mostrod v0.20.2's per-side claim;
  `uncertain` / `pow_required` named the bits and allowed one more attempt, with the user's go-ahead, once `POW` was raised. Regtest
  run for the first time: all ten tests pass, and the node's database shows a second rating from the same side was not counted
  (on v0.20.2 only).
- [x] **7. Never resend (PR #16).** (2026-11-14) The maintainer ruled on issue #10 that an uncertain rating is never resent and that
  no minimum mostrod version is added: rule 5 of `AGENTS.md` has no exception that holds only on some node versions, and a rating
  counted twice would corrupt the counterparty's reputation. The user-approved retry, the
  `pow_required` retry and the "harmless" and "can never count twice" claims are removed from the warnings, the instructions, the
  description, the README and this spec (FR-003, FR-007). `uncertain` / `pow_required` still names the bits, for future requests.
  Tests first: ten failed, then passed.
