//! `watch_order` → `mostro-cli order watch`: wait for what happens next on an
//! order this identity holds. One call replays the window (or resumes at a
//! cursor), then returns at the first new node message or chat message, or
//! empty at the deadline, with a `cursor` for the next call. Read-only.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{flag, order_id, since_minutes};
use crate::error::{Result, invalid};

/// Seconds a call waits for news when the caller does not say.
pub const DEFAULT_WAIT_SECS: u32 = 20;

/// Longest wait one call may ask for. The CLI holds the server's one CLI slot
/// while it waits, so every other tool waits too, and many MCP clients give up
/// on a request after about a minute: keep calls short and loop.
pub const MAX_WAIT_SECS: u32 = 60;

/// Time the CLI may spend before its deadline starts counting: two replays
/// (the node's messages, then the chat) of up to 15 s each, plus connecting.
/// Fixed, so a short MOSTRO_MCP_COMMAND_TIMEOUT_SECS never kills a watch that
/// is still replaying.
pub const REPLAY_ALLOWANCE: Duration = Duration::from_secs(40);

/// Longest cursor the CLI prints (its own schema's bound).
const MAX_CURSOR_CHARS: usize = 13_020;

#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
pub struct WatchOrderParams {
    /// The id (UUID) of an order in this identity's local store.
    pub order_id: String,
    /// The `cursor` of the previous watch_order result, verbatim: only newer
    /// records come back. Omit it on the first call.
    #[serde(default)]
    pub after: Option<String>,
    /// First call only (not with `after`): how far back to replay, in minutes,
    /// 1 to 10080. The CLI replays the last 30 when omitted.
    #[serde(default)]
    pub since_minutes: Option<u32>,
    /// How long to wait for something new, in seconds, 1 to 60 (default 20).
    /// The call returns as soon as a record arrives.
    #[serde(default)]
    pub wait_seconds: Option<u32>,
}

pub fn args(params: &WatchOrderParams) -> Result<Vec<String>> {
    let mut args = vec![
        "order".to_string(),
        "watch".to_string(),
        flag("order", order_id("order_id", &params.order_id)?),
    ];
    match (&params.after, params.since_minutes) {
        (Some(_), Some(_)) => {
            return Err(invalid(
                "since_minutes",
                "give it only on a first call: with `after` the watch resumes at the cursor",
            ));
        }
        (Some(after), None) => args.push(flag("after", cursor("after", after)?)),
        (None, Some(minutes)) => args.push(flag("since", since_minutes("since_minutes", minutes)?)),
        (None, None) => {}
    }
    args.push(flag("timeout", wait_seconds(params)?));
    Ok(args)
}

/// How long the call may run beyond the server's per-command timeout: the
/// deadline it gives the CLI plus [`REPLAY_ALLOWANCE`]. Only called once
/// [`args`] accepted the params.
pub fn wait(params: &WatchOrderParams) -> Duration {
    let deadline = Duration::from_secs(u64::from(params.wait_seconds.unwrap_or(DEFAULT_WAIT_SECS)));
    deadline + REPLAY_ALLOWANCE
}

fn wait_seconds(params: &WatchOrderParams) -> Result<u32> {
    let seconds = params.wait_seconds.unwrap_or(DEFAULT_WAIT_SECS);
    if !(1..=MAX_WAIT_SECS).contains(&seconds) {
        return Err(invalid(
            "wait_seconds",
            format!(
                "must be 1 to {MAX_WAIT_SECS} seconds; call again with the cursor to wait longer"
            ),
        ));
    }
    Ok(seconds)
}

/// A cursor as the CLI prints it: `<unix seconds>` or
/// `<unix seconds>:<64-hex id>,<64-hex id>...`.
fn cursor(field: &'static str, value: &str) -> Result<String> {
    let value = value.trim();
    let refuse = || {
        invalid(
            field,
            "pass the `cursor` of the previous watch_order result verbatim",
        )
    };
    if value.len() > MAX_CURSOR_CHARS {
        return Err(refuse());
    }
    let (seconds, ids) = match value.split_once(':') {
        Some((seconds, ids)) => (seconds, Some(ids)),
        None => (value, None),
    };
    let decimal = !seconds.is_empty()
        && seconds.len() <= 20
        && seconds.chars().all(|c| c.is_ascii_digit())
        && (seconds == "0" || !seconds.starts_with('0'));
    let hex_ids = ids.is_none_or(|ids| {
        ids.split(',')
            .all(|id| id.len() == 64 && id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
    });
    if decimal && hex_ids {
        Ok(value.to_string())
    } else {
        Err(refuse())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ffdc768a-37d8-429e-bfcc-0968be56b6f9";
    const CURSOR: &str = "1792108800:0707070707070707070707070707070707070707070707070707070707070707,2222222222222222222222222222222222222222222222222222222222222222";

    fn watch() -> WatchOrderParams {
        WatchOrderParams {
            order_id: ID.to_uppercase(),
            ..Default::default()
        }
    }

    #[test]
    fn a_first_call_names_the_order_and_always_states_its_deadline() {
        assert_eq!(
            args(&watch()).unwrap(),
            ["order", "watch", &format!("--order={ID}"), "--timeout=20"]
        );
        assert_eq!(wait(&watch()), Duration::from_secs(20) + REPLAY_ALLOWANCE);
    }

    #[test]
    fn a_cursor_and_a_wait_become_flags() {
        let params = WatchOrderParams {
            after: Some(format!(" {CURSOR} ")),
            wait_seconds: Some(45),
            ..watch()
        };
        assert_eq!(
            args(&params).unwrap(),
            [
                "order",
                "watch",
                &format!("--order={ID}"),
                &format!("--after={CURSOR}"),
                "--timeout=45"
            ]
        );
        assert_eq!(wait(&params), Duration::from_secs(45) + REPLAY_ALLOWANCE);
    }

    #[test]
    fn a_window_becomes_since_on_a_first_call() {
        let params = WatchOrderParams {
            since_minutes: Some(120),
            ..watch()
        };
        assert!(args(&params).unwrap().contains(&"--since=120".to_string()));
    }

    #[test]
    fn a_cursor_with_a_window_is_refused() {
        let params = WatchOrderParams {
            after: Some("0".into()),
            since_minutes: Some(10),
            ..watch()
        };
        let error = args(&params).unwrap_err().to_string();
        assert!(error.contains("since_minutes"), "{error}");
    }

    #[test]
    fn only_cursors_shaped_like_the_clis_are_passed_on() {
        for good in ["0", "1792108800", CURSOR] {
            let params = WatchOrderParams {
                after: Some(good.into()),
                ..watch()
            };
            assert!(args(&params).is_ok(), "{good}");
        }
        let long = format!("1:{}", vec!["a".repeat(64); 300].join(","));
        for bad in [
            "",
            "01",
            "--help",
            "12:",
            "12:abc",
            "12:0707070707070707070707070707070707070707070707070707070707070707,",
            "123456789012345678901",
            long.as_str(),
        ] {
            let params = WatchOrderParams {
                after: Some(bad.into()),
                ..watch()
            };
            let error = args(&params).unwrap_err().to_string();
            assert!(error.contains("after"), "{bad}: {error}");
        }
    }

    #[test]
    fn a_wait_outside_one_minute_or_a_bad_id_is_refused() {
        for seconds in [0, MAX_WAIT_SECS + 1] {
            let params = WatchOrderParams {
                wait_seconds: Some(seconds),
                ..watch()
            };
            assert!(args(&params).is_err(), "{seconds}");
        }
        let params = WatchOrderParams {
            order_id: "--order=x".into(),
            ..Default::default()
        };
        assert!(args(&params).is_err());
    }
}
