//! The CLI's outcome document and how it becomes a tool result.
//!
//! In `--output json` mode `mostro-cli` prints exactly one document on stdout
//! for success and for failure alike: `status` is `ok`, `error` or
//! `uncertain`. The exit code is checked against it but never trusted alone:
//! a document that says `ok` with a failing exit code, or the reverse, is not
//! believed.

use std::borrow::Cow;

use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::Value;

use crate::cli_backend::{redact_secrets, strip_ansi};

/// Largest document accepted, in bytes. A document is never cut: a half
/// document would be worse than none.
pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024;

/// What an agent is told first when the CLI could not tell whether the node
/// acted.
pub const UNCERTAIN_WARNING: &str = "UNCERTAIN: the request may have reached the node. \
Do NOT retry it. Check the current state first (get_order, list_messages), then decide.";

/// The same for a rating, which no tool can read back: checking the state
/// settles nothing, so the generic advice does not apply. An uncertain rating
/// is never resent, whoever asks (the maintainer's ruling on issue #10, by
/// rule 5 of AGENTS.md): mostrod versions differ in how they guard against a
/// duplicate rating and this server pins none, so nothing here may promise
/// that a second attempt counts once.
pub const RATING_UNCERTAIN_WARNING: &str = "UNCERTAIN RATING: the rating may have been \
sent, but whether it was recorded is unknown. Never call rate_counterparty again for this \
order and side, not even if the user asks: this server cannot tell whether the node guards \
against a duplicate rating. get_order does not show ratings, and a missing `rate-received` \
message does not prove the rating failed. Tell the user only what is known: the rating may \
have been sent, but whether it was recorded is unknown.";

/// The rating's warning for `outcome`, read only for an `uncertain` one.
/// `uncertain` with `pow_required` is the CLI saying it published with less
/// work than the node advertises (a late read after the silence), so the node
/// most likely dropped the rating unread: the warning names the work so POW
/// can be raised for future requests, and still never lets this rating be
/// sent again. Every other outcome gets [`RATING_UNCERTAIN_WARNING`].
pub fn rating_uncertain_warning(outcome: &Outcome) -> Cow<'static, str> {
    let error = outcome.document.get("error");
    if outcome.status != Status::Uncertain
        || error.and_then(|e| e.get("code")).and_then(Value::as_str) != Some("pow_required")
    {
        return Cow::Borrowed(RATING_UNCERTAIN_WARNING);
    }
    // Bit counts only: a number from 0 to 255, never echoed text.
    let bits = |field: &str| {
        error
            .and_then(|e| e.get(field))
            .and_then(Value::as_u64)
            .filter(|bits| *bits <= u64::from(u8::MAX))
    };
    let (asked, floor) = match (bits("required"), bits("configured")) {
        (Some(required), Some(configured)) => (
            format!("the node requires {required} bits; this server's POW is {configured}"),
            format!("POW set to at least {required} (`error.required`)"),
        ),
        (Some(required), None) => (
            format!("the node requires {required} bits"),
            format!("POW set to at least {required} (`error.required`)"),
        ),
        _ => (
            "how many bits the node requires is in `error.required`, which this outcome \
             does not carry"
                .to_string(),
            "POW raised to what the node advertises (`error.required`)".to_string(),
        ),
    };
    Cow::Owned(format!(
        "UNCERTAIN RATING, TOO LITTLE PROOF OF WORK: the rating was published with less \
         proof of work than the node asks for ({asked}). The node most likely dropped it \
         unread, but that is not proven. Never call rate_counterparty again for this order \
         and side, not even if the user asks. Tell the user the rating may have been sent, \
         but whether it was recorded is unknown, and that the proof of work was too low. \
         To avoid this on future requests the user can run this server with {floor}; POW \
         is read when this server starts. get_order does not show ratings, and a missing \
         `rate-received` message does not prove the rating failed."
    ))
}

/// A short, cleaned excerpt of output that is not an outcome document, for an
/// error message.
pub fn excerpt(output: &str) -> String {
    const MAX_CHARS: usize = 2000;
    let clean = redact_secrets(&strip_ansi(output));
    let mut text: String = clean.trim().chars().take(MAX_CHARS).collect();
    if clean.trim().chars().count() > MAX_CHARS {
        text.push_str("\n[excerpt truncated]");
    }
    text
}

/// A copy of `value` with every string redacted, object keys included.
/// serde_json caps nesting at 128 levels, so the recursion is bounded.
fn redact_document(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(redact_secrets(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(redact_document).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, field)| (redact_secrets(&key), redact_document(field)))
                .collect(),
        ),
        other => other,
    }
}

/// The version of the outcome document this server understands.
const SCHEMA_VERSION: &str = "1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Error,
    Uncertain,
}

/// One validated outcome document.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    status: Status,
    document: Value,
}

impl Outcome {
    /// Parse `stdout` of a CLI run that ended with `code`. The error is a
    /// reason fit to show an agent.
    pub fn parse(stdout: &str, code: Option<i32>) -> Result<Self, String> {
        // The runner keeps one byte past the limit, so the size is not shown.
        if stdout.len() > MAX_DOCUMENT_BYTES {
            return Err(format!(
                "the output is over the {MAX_DOCUMENT_BYTES} byte limit"
            ));
        }
        // Redacted before it is parsed, so what is returned is what was checked,
        // and again once parsed: a JSON escape (`\u006e`) hides a key from the
        // first pass and decoding brings it back.
        let clean = redact_secrets(&strip_ansi(stdout));
        let document: Value = serde_json::from_str(clean.trim())
            .map(redact_document)
            .map_err(|_| "the output is not one JSON outcome document".to_string())?;
        let status = match document.get("status").and_then(Value::as_str) {
            Some("ok") => Status::Ok,
            Some("error") => Status::Error,
            Some("uncertain") => Status::Uncertain,
            _ => return Err("the document has no valid `status`".to_string()),
        };
        let versioned =
            document.get("schema_version").and_then(Value::as_str) == Some(SCHEMA_VERSION);
        let named = matches!(
            document.get("command"),
            Some(Value::String(_) | Value::Null)
        );
        if !versioned || !named {
            return Err(format!(
                "the document is not an outcome of version {SCHEMA_VERSION}"
            ));
        }
        if (status == Status::Ok) != (code == Some(0)) {
            return Err(format!(
                "the document says `{}` but the exit status is {}",
                document["status"].as_str().unwrap_or("?"),
                code.map_or("unknown".to_string(), |code| code.to_string())
            ));
        }
        Ok(Self { status, document })
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn document(&self) -> &Value {
        &self.document
    }

    /// The `command` the CLI says it ran, when it resolved one.
    pub fn command(&self) -> Option<&str> {
        self.document.get("command").and_then(Value::as_str)
    }

    /// The `data` of a success, or of an uncertain outcome that carries some.
    pub fn data(&self) -> Option<&Value> {
        self.document.get("data").filter(|data| !data.is_null())
    }

    /// The tool result: the document as text and as structured content. A
    /// failure is a tool error; an uncertain outcome says first not to retry.
    pub fn into_tool_result(self) -> CallToolResult {
        self.into_tool_result_warned(UNCERTAIN_WARNING)
    }

    /// [`Outcome::into_tool_result`] with `warning` heading an `uncertain`
    /// result instead of the generic one.
    pub fn into_tool_result_warned(self, warning: &str) -> CallToolResult {
        let text = self.document.to_string();
        match self.status {
            Status::Ok => CallToolResult::structured(self.document),
            Status::Error => CallToolResult::structured_error(self.document),
            Status::Uncertain => {
                let mut result = CallToolResult::structured_error(self.document);
                result.content = vec![ContentBlock::text(format!("{warning}\n\n{text}"))];
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTCOMES: &str = include_str!("../tests/fixtures/outcomes.json");

    /// A fixture's stdout (re-serialized as the CLI prints it) and exit code.
    fn fixture(name: &str) -> (String, i32) {
        let all: Value = serde_json::from_str(OUTCOMES).unwrap();
        let case = &all[name];
        (
            case["stdout"].to_string(),
            i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
        )
    }

    fn parsed(name: &str) -> Outcome {
        let (stdout, code) = fixture(name);
        Outcome::parse(&stdout, Some(code)).unwrap_or_else(|reason| panic!("{name}: {reason}"))
    }

    fn text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect()
    }

    #[test]
    fn every_example_of_the_cli_parses_and_its_status_agrees_with_its_exit_code() {
        let all: Value = serde_json::from_str(OUTCOMES).unwrap();
        for name in all.as_object().unwrap().keys() {
            let (_, code) = fixture(name);
            let outcome = parsed(name);
            assert_eq!(outcome.status() == Status::Ok, code == 0, "{name}");
        }
    }

    #[test]
    fn a_success_is_a_successful_result_with_the_document_as_text_and_structure() {
        let result = parsed("release_range_continued").into_tool_result();
        assert_ne!(result.is_error, Some(true));
        let document = result
            .structured_content
            .clone()
            .expect("structured content");
        assert_eq!(document["data"]["next_step"], "sync_child_order");
        assert_eq!(
            serde_json::from_str::<Value>(&text(&result)).unwrap(),
            document
        );
    }

    #[test]
    fn an_error_is_a_tool_error_that_keeps_the_code_and_the_reason() {
        let result = parsed("release_node_refused").into_tool_result();
        assert_eq!(result.is_error, Some(true));
        let body = text(&result);
        assert!(
            body.contains("node_refused") && body.contains("invalid_peer"),
            "{body}"
        );
        assert!(!body.starts_with("UNCERTAIN"));
    }

    #[test]
    fn an_uncertain_outcome_says_first_not_to_retry() {
        for name in ["release_uncertain", "take_uncertain"] {
            let result = parsed(name).into_tool_result();
            assert_eq!(result.is_error, Some(true), "{name}");
            let body = text(&result);
            assert!(body.starts_with(UNCERTAIN_WARNING), "{name}: {body}");
            assert!(body.contains("reply_timeout"), "{name}");
            assert_eq!(
                result.structured_content.unwrap()["status"],
                "uncertain",
                "{name}"
            );
        }
    }

    #[test]
    fn only_an_uncertain_pow_required_rating_gets_the_proof_of_work_warning() {
        let with_error = |status: &str, code: i32, error: Value| {
            let (stdout, _) = fixture("rate_uncertain");
            let mut document: Value = serde_json::from_str(&stdout).unwrap();
            document["status"] = status.into();
            document["error"] = error;
            Outcome::parse(&document.to_string(), Some(code)).unwrap()
        };
        let pow = serde_json::json!({"code": "pow_required", "message": "m",
            "required": 22, "configured": 0});
        let uncertain = rating_uncertain_warning(&with_error("uncertain", 1, pow.clone()));
        assert!(
            uncertain.starts_with("UNCERTAIN RATING, TOO LITTLE PROOF OF WORK"),
            "{uncertain}"
        );
        assert!(uncertain.contains("22 bits"), "{uncertain}");
        // Nothing was published on an `error`: not this warning's case.
        let unsent = rating_uncertain_warning(&with_error("error", 1, pow));
        assert_eq!(unsent, RATING_UNCERTAIN_WARNING);
        let silent = rating_uncertain_warning(&parsed("rate_uncertain"));
        assert_eq!(silent, RATING_UNCERTAIN_WARNING);
    }

    #[test]
    fn a_next_step_to_pay_survives_untouched_as_data() {
        let outcome = parsed("take_pay_bond");
        let data = outcome.data().expect("data");
        assert_eq!(data["next_step"], "pay_bond_invoice");
        assert!(data["payment_request"].as_str().unwrap().starts_with("ln"));
    }

    #[test]
    fn what_is_not_one_document_is_refused_with_a_reason() {
        let (good, _) = fixture("new_created");
        let cases = [
            ("", "not one JSON"),
            ("📋 List Orders\n(table)", "not one JSON"),
            ("[1,2]", "no valid `status`"),
            (r#"{"status":"ok"}"#, "version"),
            (
                r#"{"schema_version":"2","command":null,"status":"ok"}"#,
                "version",
            ),
            (
                r#"{"schema_version":"1","command":null,"status":"maybe"}"#,
                "`status`",
            ),
        ];
        for (stdout, reason) in cases {
            let error = Outcome::parse(stdout, Some(0)).unwrap_err();
            assert!(error.contains(reason), "{stdout:?}: {error}");
        }
        // Two documents are not one.
        assert!(Outcome::parse(&format!("{good}\n{good}"), Some(0)).is_err());
    }

    #[test]
    fn the_exit_code_is_checked_against_the_status_in_both_directions() {
        let (ok, _) = fixture("new_created");
        let (failed, _) = fixture("release_node_refused");
        assert!(
            Outcome::parse(&ok, Some(1))
                .unwrap_err()
                .contains("exit status is 1")
        );
        assert!(Outcome::parse(&ok, None).is_err());
        assert!(
            Outcome::parse(&failed, Some(0))
                .unwrap_err()
                .contains("`error`")
        );
        assert!(Outcome::parse(&failed, Some(2)).is_ok());
    }

    #[test]
    fn a_document_over_the_limit_is_refused_not_cut() {
        let big = format!(
            r#"{{"schema_version":"1","command":null,"status":"ok","pad":"{}"}}"#,
            "x".repeat(MAX_DOCUMENT_BYTES)
        );
        let error = Outcome::parse(&big, Some(0)).unwrap_err();
        assert!(error.contains("byte limit"), "{error}");
    }

    #[test]
    fn a_secret_key_is_redacted_before_the_document_is_parsed() {
        let key = format!("nsec1{}", "q".repeat(58));
        let stdout = format!(
            r#"{{"schema_version":"1","command":"x","status":"ok","data":{{"note":"{key}"}}}}"#
        );
        let result = Outcome::parse(&stdout, Some(0)).unwrap().into_tool_result();
        assert!(!text(&result).contains(&key));
        assert!(
            !result
                .structured_content
                .unwrap()
                .to_string()
                .contains(&key)
        );
    }

    #[test]
    fn a_secret_key_hidden_by_a_json_escape_is_redacted_once_decoded() {
        let body = "q".repeat(58);
        let key = format!("nsec1{body}");
        // `\u006e` is `n`: the raw text has no `nsec1`, the parsed string has.
        let escaped = format!(r"\u006esec1{body}");
        let stdout = format!(
            r#"{{"schema_version":"1","command":"x","status":"ok","data":{{"notes":["{escaped}"],"{escaped}":1}}}}"#
        );
        let result = Outcome::parse(&stdout, Some(0)).unwrap().into_tool_result();
        assert!(!text(&result).contains(&key), "{}", text(&result));
        let document = result.structured_content.unwrap().to_string();
        assert!(!document.contains(&key), "{document}");
        assert!(document.contains("[redacted nsec]"), "{document}");
    }

    #[test]
    fn colour_codes_around_the_document_do_not_matter() {
        let (stdout, _) = fixture("new_created");
        let coloured = format!("\u{1b}[32m{stdout}\u{1b}[0m");
        assert!(Outcome::parse(&coloured, Some(0)).is_ok());
    }
}
