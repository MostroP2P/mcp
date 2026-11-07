//! The steps of a running trade, one order each: `fiat_sent` → `mostro-cli
//! order fiat-sent`, `release` → `order release`, `cancel` → `order cancel`.
//! State-changing: refused unless the server runs with writes enabled.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::order_step;
use crate::error::{Result, invalid};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct OrderStepParams {
    /// The id (UUID) of the order.
    pub order_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ReleaseParams {
    /// The id (UUID) of the order whose sats you release.
    pub order_id: String,
    /// Must be `true`: the user confirmed that the fiat arrived in their
    /// account. Releasing sends the sats to the buyer and cannot be undone.
    pub fiat_received: bool,
}

pub fn fiat_sent_args(params: &OrderStepParams) -> Result<Vec<String>> {
    order_step("fiat-sent", "order_id", &params.order_id)
}

/// The one irreversible step: refused before anything runs unless the caller
/// states that the fiat arrived.
pub fn release_args(params: &ReleaseParams) -> Result<Vec<String>> {
    if !params.fiat_received {
        return Err(invalid(
            "fiat_received",
            "must be true: release only after the user confirms the fiat arrived, \
             it sends the sats and cannot be undone",
        ));
    }
    order_step("release", "order_id", &params.order_id)
}

pub fn cancel_args(params: &OrderStepParams) -> Result<Vec<String>> {
    order_step("cancel", "order_id", &params.order_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ffdc768a-37d8-429e-bfcc-0968be56b6f9";

    fn order() -> OrderStepParams {
        OrderStepParams {
            order_id: ID.to_uppercase(),
        }
    }

    #[test]
    fn each_step_names_its_command_and_confirms() {
        let id = format!("--order={ID}");
        assert_eq!(
            fiat_sent_args(&order()).unwrap(),
            ["order", "fiat-sent", &id, "--yes"]
        );
        assert_eq!(
            cancel_args(&order()).unwrap(),
            ["order", "cancel", &id, "--yes"]
        );
        let release = ReleaseParams {
            order_id: ID.into(),
            fiat_received: true,
        };
        assert_eq!(
            release_args(&release).unwrap(),
            ["order", "release", &id, "--yes"]
        );
    }

    #[test]
    fn release_is_refused_unless_the_fiat_was_received() {
        let release = ReleaseParams {
            order_id: ID.into(),
            fiat_received: false,
        };
        let error = release_args(&release).unwrap_err().to_string();
        assert!(error.contains("fiat_received"), "{error}");
    }

    #[test]
    fn a_bad_id_never_reaches_the_cli() {
        let bad = OrderStepParams {
            order_id: "--orderid=x".into(),
        };
        assert!(fiat_sent_args(&bad).is_err());
        assert!(cancel_args(&bad).is_err());
    }
}
