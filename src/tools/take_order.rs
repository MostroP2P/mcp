//! `take_order` → `mostro-cli takesell` / `takebuy`. One tool, two CLI
//! commands: which one depends on the side of the order being taken, which
//! the caller must state — it is never guessed.
//! State-changing: refused unless the server runs with writes enabled.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{OrderKind, flag, invoice, order_id};
use crate::error::{Result, invalid};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct TakeOrderParams {
    /// The id (UUID) of the order to take, from `list_orders`.
    pub order_id: String,
    /// The side of the order as the book lists it. `sell`: the maker sells
    /// sats — you BUY sats and pay fiat. `buy`: the maker buys sats — you
    /// SELL sats and receive fiat (you will be asked to pay a hold invoice).
    pub order_kind: OrderKind,
    /// Range orders only: the fiat amount you want, inside the order's range.
    #[serde(default)]
    pub fiat_amount: Option<u32>,
    /// Taking a `sell` order only: the Lightning invoice or address your sats
    /// are paid to. Without it the node asks for one. A BOLT11 invoice must
    /// still be valid for the node's invoice window (one hour by default).
    #[serde(default)]
    pub invoice: Option<String>,
}

pub fn args(params: &TakeOrderParams) -> Result<Vec<String>> {
    let id = order_id("order_id", &params.order_id)?;
    let mut args = match params.order_kind {
        OrderKind::Sell => vec!["takesell".to_string(), flag("orderid", &id)],
        OrderKind::Buy => {
            if params.invoice.is_some() {
                return Err(invalid(
                    "invoice",
                    "taking a buy order means you sell sats: there is nothing for you \
                     to receive by invoice",
                ));
            }
            vec!["takebuy".to_string(), flag("orderid", &id)]
        }
    };
    if let Some(value) = &params.invoice {
        args.push(flag("invoice", invoice("invoice", value)?));
    }
    if let Some(amount) = params.fiat_amount {
        if amount == 0 {
            return Err(invalid("fiat_amount", "must be greater than zero"));
        }
        args.push(flag("amount", amount));
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ffdc768a-37d8-429e-bfcc-0968be56b6f9";

    fn take(kind: OrderKind) -> TakeOrderParams {
        TakeOrderParams {
            order_id: ID.into(),
            order_kind: kind,
            fiat_amount: None,
            invoice: None,
        }
    }

    #[test]
    fn taking_a_sell_order_is_takesell() {
        assert_eq!(
            args(&take(OrderKind::Sell)).unwrap(),
            ["takesell", &format!("--orderid={ID}")]
        );
    }

    #[test]
    fn taking_a_buy_order_is_takebuy() {
        assert_eq!(
            args(&take(OrderKind::Buy)).unwrap(),
            ["takebuy", &format!("--orderid={ID}")]
        );
    }

    #[test]
    fn a_buyer_taking_a_sell_order_may_bring_an_invoice_and_an_amount() {
        let params = TakeOrderParams {
            fiat_amount: Some(150),
            invoice: Some("lnbcrt1pexample".into()),
            ..take(OrderKind::Sell)
        };
        assert_eq!(
            args(&params).unwrap(),
            [
                "takesell",
                &format!("--orderid={ID}"),
                "--invoice=lnbcrt1pexample",
                "--amount=150"
            ]
        );
    }

    #[test]
    fn a_seller_has_no_invoice_to_give() {
        let params = TakeOrderParams {
            invoice: Some("lnbcrt1pexample".into()),
            ..take(OrderKind::Buy)
        };
        assert!(args(&params).is_err());
    }

    #[test]
    fn a_zero_amount_or_a_bad_id_is_rejected() {
        let zero = TakeOrderParams {
            fiat_amount: Some(0),
            ..take(OrderKind::Sell)
        };
        assert!(args(&zero).is_err());
        let bad_id = TakeOrderParams {
            order_id: "--help".into(),
            ..take(OrderKind::Sell)
        };
        assert!(args(&bad_id).is_err());
    }
}
