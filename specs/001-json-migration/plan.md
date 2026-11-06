# Implementation Plan: Drive The JSON CLI From MCP

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-04 | **Status**: Draft

## Summary

Keep the server shape (typed params, a pure `args` function per tool, one runner, a write gate) and change the contract underneath: the runner
returns a parsed outcome, each tool builds the new command line, the server maps `status` to a tool result, and a startup check reads the CLI's own
`schema`. The new trade tools are the same pattern.

## Design

1. **Backend** (`cli_backend.rs`): `CliRunner::run(args)` adds `--output json` and returns the run (stdout, cleaned stderr, exit code); it keeps the
   cleared environment, timeout, single-flight lock and kill-on-drop. `CliOutput`'s text-only path is removed. Parsing the outcome refuses stdout over
   a fixed size limit as a whole (a tool error naming the size and the limit, never a cut document). It redacts `nsec1...` twice: in the raw text
   after stripping ANSI, and in every string of the parsed document, since a JSON escape can hide the prefix from the raw text. The text and the
   structured content are serialized from that one redacted document; only the excerpt of unreadable output is truncated.
   Tested with a document over the limit.
2. **Mapping** (`server.rs` or a new `outcome.rs`): `ok` -> success; `error` -> tool error with `error.code`/`reason`; `uncertain` -> tool error whose
   text is the fixed warning, a blank line and the document, and whose structured content is the document; unreadable output -> tool error with a
   redacted, truncated excerpt, starting with the same fixed warning when the tool is state-changing. One function, unit-tested with the CLI's examples.
3. **Contract check** (`contract.rs`): run `schema` at startup, compare the canonical names each enabled tool needs with the entries that list `json`;
   fail with the missing names and `--version`. A pure function over the parsed document, tested with good and bad documents.
4. **Tools** (`tools/*.rs`): the four existing tools get the new flags (`--side`, `--currency`, `--fiat-amount[-max]`, `--payment-method`, `--sats`, `--premium`,
   `--invoice`, `--expiration-days`, `--order`, `--amount`, `--yes`); new modules for `list_messages`, `add_invoice`, `add_bond_invoice`, `fiat_sent`,
   `release`, `cancel`, `sync_orders`.
5. **Tests**: unit tests for every `args`; stdio integration tests (`tests/mcp_stdio.rs`) against a scripted CLI script that prints example documents
   selected by its first arguments; the e2e test updated for the new flow.
6. **Docs**: README tools table, CLI requirement, a whole-trade walk-through, security section.

## Risks

- **Version drift**: the CLI contract can change; the startup check and a `schema` version check turn that into a clear error instead of odd results.
- **Agent mistakes on money-moving tools**: the write gate, the host's approval prompt and `fiat_received` are the guards; descriptions are explicit.
- **Duplicate sends after `uncertain`**: the fixed warning and the descriptions point at `get_order` and `list_messages`.
- **Large outputs**: `order list` and `message list` documents are bounded by the CLI; the server refuses to return a half document.

## Complexity Tracking

| Addition | Why it is needed | Simpler option rejected |
| --- | --- | --- |
| Parsed outcome in the runner | status drives the result; exit codes alone cannot say `uncertain` | return text and let the agent read it |
| Startup contract check | the CLI has no stable version number for this surface; its `schema` is the contract | pin a commit in the README |
| `fiat_received` on `release` | the one irreversible tool deserves an explicit statement beyond the host prompt | rely on the host prompt |
