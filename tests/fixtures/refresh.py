"""Rebuild outcomes.json from a mostro-cli checkout and the output of its `--output json schema`.

usage: python3 -I tests/fixtures/refresh.py <cli checkout> <schema.json> tests/fixtures/outcomes.json
"""
import json, sys
cli, schema_path, out = sys.argv[1:4]
PICK = {
    "info_node_refused": ("002-order-info-json", "node_refused"),
    "list_empty": ("001-order-list-json", "empty_success"),
    "messages_of_a_trade": ("006-message-list-json", "messages_of_a_trade"),
    "messages_identity_missing": ("006-message-list-json", "identity_missing"),
    "new_created": ("003-order-new-json", "created"),
    "release_node_refused": ("007-trade-steps-json", "release_node_refused"),
    "release_range_continued": ("007-trade-steps-json", "release_range_continued"),
    "release_uncertain": ("007-trade-steps-json", "release_reply_timeout_uncertain"),
    "cancel_canceled": ("007-trade-steps-json", "cancel_canceled"),
    "sync_child_added": ("008-range-children", "child_order_added"),
    "take_pay_bond": ("004-order-take-json", "sell_pay_bond_invoice"),
    "take_uncertain": ("004-order-take-json", "buy_reply_timeout_uncertain"),
    "bond_invoice_accepted": ("005-invoice-json", "bond_invoice_accepted"),
    "yes_required": ("007-trade-steps-json", "fiat_sent_yes_required"),
    "mine_newest_first": ("011-order-mine", "orders_newest_first"),
    "chat_read": ("019-order-chat", "conversation_read"),
    "chat_sent": ("019-order-chat", "message_sent"),
    "chat_send_uncertain": ("019-order-chat", "send_uncertain"),
    "chat_counterparty_unknown": ("019-order-chat", "counterparty_unknown"),
    "watch_new_records": ("020-order-watch", "new_records"),
    "watch_timeout_empty": ("020-order-watch", "timeout_empty"),
    "dispute_opened": ("013-dispute-open", "dispute_opened"),
    "dispute_status_stale": ("013-dispute-open", "dispute_opened_status_stale"),
    "dispute_node_refused": ("013-dispute-open", "node_refused"),
    "dispute_uncertain": ("013-dispute-open", "no_answer_uncertain"),
    "dispute_order_unknown": ("013-dispute-open", "order_unknown"),
    "rate_received": ("014-order-rate", "rating_received"),
    "rate_node_refused": ("014-order-rate", "node_refused"),
    "rate_uncertain": ("014-order-rate", "no_answer_uncertain"),
    "rate_out_of_range": ("014-order-rate", "rating_out_of_range"),
}
fixtures = {}
for key, (spec, name) in sorted(PICK.items()):
    cases = json.load(open(f"{cli}/specs/{spec}/contracts/examples.json"))["cases"]
    (case,) = [c for c in cases if c["name"] == name]
    fixtures[key] = {"exit_code": case["exit_code"], "stdout": case["stdout"]}
schema = json.load(open(schema_path))
schema["data"] = {"commands": [
    {k: c[k] for k in ("name", "cli_path", "output_modes")} for c in schema["data"]["commands"]
]}
fixtures["schema"] = {"exit_code": 0, "stdout": schema}
with open(out, "w") as f:
    json.dump(fixtures, f, indent=1, ensure_ascii=False, sort_keys=True)
    f.write("\n")
