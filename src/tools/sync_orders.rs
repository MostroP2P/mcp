//! `sync_orders` → `mostro-cli order sync`: adopts the orders the node created
//! for this identity that the local store lacks (the next order of a range,
//! a child order). State-changing: it saves rows in the CLI's store, so it is
//! refused unless the server runs with writes enabled.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{flag, since_minutes};
use crate::error::Result;

#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SyncOrdersParams {
    /// How far back to read, in minutes: 1 to 10080 (one week, the CLI's
    /// default).
    #[serde(default)]
    pub since_minutes: Option<u32>,
}

pub fn args(params: &SyncOrdersParams) -> Result<Vec<String>> {
    let mut args = vec!["order".to_string(), "sync".to_string()];
    if let Some(minutes) = params.since_minutes {
        args.push(flag("since", since_minutes("since_minutes", minutes)?));
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_order_sync_with_an_optional_window() {
        assert_eq!(
            args(&SyncOrdersParams::default()).unwrap(),
            ["order", "sync"]
        );
        let params = SyncOrdersParams {
            since_minutes: Some(60),
        };
        assert_eq!(args(&params).unwrap(), ["order", "sync", "--since=60"]);
        let too_far = SyncOrdersParams {
            since_minutes: Some(20_000),
        };
        assert!(args(&too_far).is_err());
    }
}
