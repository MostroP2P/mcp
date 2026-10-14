//! `get_order` → `mostro-cli ordersinfo`: the node's own record of an order.
//!
//! The daemon answers it per identity: it describes orders this identity
//! made or took, and nothing else. A stranger's order is read from the book
//! (`list_orders`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{flag, order_id};
use crate::error::Result;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetOrderParams {
    /// The order's id (a UUID), as shown by `list_orders` or returned by
    /// `create_order`.
    pub order_id: String,
}

pub fn args(params: &GetOrderParams) -> Result<Vec<String>> {
    Ok(vec![
        "ordersinfo".to_string(),
        flag("orderids", order_id("order_id", &params.order_id)?),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_ordersinfo_for_the_canonical_id() {
        let params = GetOrderParams {
            order_id: "FFDC768A-37D8-429E-BFCC-0968BE56B6F9".into(),
        };
        assert_eq!(
            args(&params).unwrap(),
            [
                "ordersinfo",
                "--orderids=ffdc768a-37d8-429e-bfcc-0968be56b6f9"
            ]
        );
    }

    #[test]
    fn an_id_that_is_not_a_uuid_never_reaches_the_cli() {
        let params = GetOrderParams {
            order_id: "1; rm -rf ~".into(),
        };
        assert!(args(&params).is_err());
    }
}
