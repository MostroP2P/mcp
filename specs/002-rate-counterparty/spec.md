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
  Both a release and a solver's settlement (`src/app/admin_settle.rs`) set `settled-hold-invoice`. Anything else is `cant-do`
  `invalid_order_status`. A side that already rated, a full-privacy counterparty and a stranger get no answer.
- The CLI signs with the order's trade key, waits for `rate-received` with the rating it sent, and writes nothing locally. Silence is
  `uncertain` / `reply_timeout`. No command reads a rating back. `message list` lists every message the node sent to the order's trade
  key, so a `rate-received` there confirms a rating; its absence proves nothing.
- At 0.17.1 every request mines the proof of work the node advertises on top of `POW`, up to 20 bits (spec `017-advertised-pow`; a
  rating uses the first-contact lane); above that it is `pow_required`, nothing sent. The 0.17.0 caveat (a rating failing on a node
  that asks for proof of work unless `POW` is set) does not apply to 0.17.1.
- At 0.17.1 `message list` reports `next_step` `pay_hold_invoice`, `pay_bond_invoice`, `add_invoice`, `add_bond_invoice`,
  `fiat_sent`, `release` and `rate` (`src/outcome/messages/next_step.rs` at tag `v0.17.1`), and null otherwise. 0.17.0 had the first
  four only. The CLI book still calls the last three unreleased.

## Requirements

- **FR-001**: `rate_counterparty(order_id, rating)` runs `order rate --order=<canonical uuid> --rating=<n> --yes` behind the write
  gate. `order_id` goes through the shared validator; `rating` is a `u8` refused outside 1 to 5 before the CLI starts.
- **FR-002**: Annotations: title `Rate counterparty`, not read-only, not destructive, not idempotent, open world. The outcome is the
  CLI's document, unchanged; a timeout or unreadable output carries the no-retry warning.
- **FR-003**: The description and the instructions say the rating is the user's choice, never the agent's, and never a consequence of
  `success` alone; one rating per side per order, final; who may rate when; never resend on `uncertain`; `get_order` does not show
  ratings and `rate-received` in `list_messages` confirms one.
- **FR-004**: `contract::WRITE` requires `order.rate`.
- **FR-005**: `list_messages` names the `next_step` values of the findings and says an action `rate` asks for the user's rating.
- **FR-006**: The four `order.rate` examples of spec `014-order-rate` are fixtures; README and test summary updated.

## Slice

- [x] **1. Tool, contract, descriptions, tests, docs.** (2026-11-12) Regtest written, not run: no Docker daemon in the environment
  where it was implemented.
