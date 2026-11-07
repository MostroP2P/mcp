//! `list_messages` → `mostro-cli message list`: what the node said about this
//! identity's trades, newest first: the step it asks for next, with any
//! invoice to pay as data. Read-only: it reads the relays and changes nothing.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{flag, order_id, since_minutes};
use crate::error::Result;

#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ListMessagesParams {
    /// Only the messages of this order (a UUID). Omitted: every trade of this
    /// identity.
    #[serde(default)]
    pub order_id: Option<String>,
    /// How far back to read, in minutes: 1 to 10080 (one week). The CLI
    /// reads the last 30 when omitted.
    #[serde(default)]
    pub since_minutes: Option<u32>,
}

pub fn args(params: &ListMessagesParams) -> Result<Vec<String>> {
    let mut args = vec!["message".to_string(), "list".to_string()];
    if let Some(id) = &params.order_id {
        args.push(flag("order", order_id("order_id", id)?));
    }
    if let Some(minutes) = params.since_minutes {
        args.push(flag("since", since_minutes("since_minutes", minutes)?));
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_filters_it_reads_every_trade() {
        assert_eq!(
            args(&ListMessagesParams::default()).unwrap(),
            ["message", "list"]
        );
    }

    #[test]
    fn an_order_and_a_window_become_flags() {
        let params = ListMessagesParams {
            order_id: Some("FFDC768A-37D8-429E-BFCC-0968BE56B6F9".into()),
            since_minutes: Some(120),
        };
        assert_eq!(
            args(&params).unwrap(),
            [
                "message",
                "list",
                "--order=ffdc768a-37d8-429e-bfcc-0968be56b6f9",
                "--since=120"
            ]
        );
    }

    #[test]
    fn a_window_outside_one_week_or_a_bad_id_is_rejected() {
        for since in [0, 10_081] {
            let params = ListMessagesParams {
                since_minutes: Some(since),
                ..Default::default()
            };
            assert!(args(&params).is_err(), "{since}");
        }
        let params = ListMessagesParams {
            order_id: Some("--help".into()),
            ..Default::default()
        };
        assert!(args(&params).is_err());
    }
}
