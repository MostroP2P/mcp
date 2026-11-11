//! Disputes: `open_dispute` → `mostro-cli dispute open`. State-changing:
//! refused unless the server runs with writes enabled.
//!
//! The dispute chat and the list of disputes are not here: mostro-cli 0.17.1
//! prints them only for humans, and this server only reads JSON documents.

use super::trade_step::OrderStepParams;
use super::{flag, order_id};
use crate::error::Result;

/// `dispute open --order=<id> --yes`: one request, signed by the order's
/// trade key, on an order this identity holds.
pub fn open_args(params: &OrderStepParams) -> Result<Vec<String>> {
    Ok(vec![
        "dispute".to_string(),
        "open".to_string(),
        flag("order", order_id("order_id", &params.order_id)?),
        "--yes".to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ffdc768a-37d8-429e-bfcc-0968be56b6f9";

    #[test]
    fn opening_a_dispute_names_its_command_and_confirms() {
        let params = OrderStepParams {
            order_id: ID.to_uppercase(),
        };
        assert_eq!(
            open_args(&params).unwrap(),
            ["dispute", "open", &format!("--order={ID}"), "--yes"]
        );
    }

    #[test]
    fn a_bad_id_never_reaches_the_cli() {
        for bad in ["--order=x", "not-a-uuid", ""] {
            let params = OrderStepParams {
                order_id: bad.into(),
            };
            let error = open_args(&params).unwrap_err().to_string();
            assert!(error.contains("order_id"), "{bad}: {error}");
        }
    }
}
