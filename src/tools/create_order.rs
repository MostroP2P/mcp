//! `create_order` → `mostro-cli neworder`: publish a new buy or sell order.
//! State-changing: refused unless the server runs with writes enabled.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{OrderKind, fiat_code, flag, invoice, positive, text};
use crate::error::{Result, invalid};

/// Longest payment method accepted, in characters.
pub const MAX_PAYMENT_METHOD_CHARS: usize = 200;

/// A premium at or below this percentage prices the sats at nothing or less.
pub const MIN_PREMIUM: i64 = -100;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreateOrderParams {
    /// `sell`: you sell sats and receive fiat. `buy`: you buy sats and pay
    /// fiat.
    pub kind: OrderKind,
    /// Fiat currency code, e.g. `USD`, `ARS`.
    pub fiat_code: String,
    /// Fixed fiat amount. Give this, or `fiat_amount_min` and
    /// `fiat_amount_max` for a range order — never both.
    #[serde(default)]
    pub fiat_amount: Option<u64>,
    /// Lower bound of a range order's fiat amount.
    #[serde(default)]
    pub fiat_amount_min: Option<u64>,
    /// Upper bound of a range order's fiat amount.
    #[serde(default)]
    pub fiat_amount_max: Option<u64>,
    /// How the fiat is paid, e.g. `bank transfer`, `Mercado Pago`.
    pub payment_method: String,
    /// Fixed amount in sats. Omit it (or 0) to price at market rate, where
    /// `premium` applies.
    #[serde(default)]
    pub amount_sats: Option<u64>,
    /// Percent above (positive) or below (negative) the market price.
    #[serde(default)]
    pub premium: Option<i64>,
    /// Buy orders only: the Lightning invoice or address the sats are paid
    /// to. Without it the node asks for one once the order is taken. A BOLT11
    /// invoice must still be valid for the node's invoice window (one hour by
    /// default).
    #[serde(default)]
    pub invoice: Option<String>,
    /// Days the order stays pending before it expires; omitted uses the
    /// node's default.
    #[serde(default)]
    pub expiration_days: Option<u32>,
}

pub fn args(params: &CreateOrderParams) -> Result<Vec<String>> {
    let fiat_amount = match (
        params.fiat_amount,
        params.fiat_amount_min,
        params.fiat_amount_max,
    ) {
        (Some(amount), None, None) => positive("fiat_amount", amount)?.to_string(),
        (None, Some(min), Some(max)) => {
            positive("fiat_amount_min", min)?;
            positive("fiat_amount_max", max)?;
            if min >= max {
                return Err(invalid(
                    "fiat_amount_max",
                    format!("must be greater than fiat_amount_min ({min})"),
                ));
            }
            format!("{min}-{max}")
        }
        _ => {
            return Err(invalid(
                "fiat_amount",
                "give either fiat_amount, or both fiat_amount_min and fiat_amount_max",
            ));
        }
    };
    if params.invoice.is_some() && params.kind == OrderKind::Sell {
        return Err(invalid(
            "invoice",
            "only a buy order receives sats; a sell order has no invoice to give",
        ));
    }

    let mut args = vec![
        "neworder".to_string(),
        flag("kind", params.kind.as_cli()),
        flag("fiatcode", fiat_code("fiat_code", &params.fiat_code)?),
        flag("fiatamount", fiat_amount),
        flag(
            "paymentmethod",
            text(
                "payment_method",
                &params.payment_method,
                MAX_PAYMENT_METHOD_CHARS,
            )?,
        ),
    ];
    if let Some(sats) = params.amount_sats.filter(|&sats| sats > 0) {
        args.push(flag("amount", positive("amount_sats", sats)?));
    }
    if let Some(premium) = params.premium.filter(|&premium| premium != 0) {
        if premium <= MIN_PREMIUM {
            return Err(invalid(
                "premium",
                format!(
                    "must be above {MIN_PREMIUM} (percent): at {MIN_PREMIUM} the sats would be \
                     given away"
                ),
            ));
        }
        args.push(flag("premium", premium));
    }
    if let Some(value) = &params.invoice {
        args.push(flag("invoice", invoice("invoice", value)?));
    }
    if let Some(days) = params.expiration_days.filter(|&days| days > 0) {
        args.push(flag("expirationdays", days));
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sell(fiat_amount: u64) -> CreateOrderParams {
        CreateOrderParams {
            kind: OrderKind::Sell,
            fiat_code: "usd".into(),
            fiat_amount: Some(fiat_amount),
            fiat_amount_min: None,
            fiat_amount_max: None,
            payment_method: "bank transfer".into(),
            amount_sats: None,
            premium: None,
            invoice: None,
            expiration_days: None,
        }
    }

    #[test]
    fn a_fixed_order_maps_to_neworder_flags() {
        let params = CreateOrderParams {
            amount_sats: Some(10_000),
            ..sell(10)
        };
        assert_eq!(
            args(&params).unwrap(),
            [
                "neworder",
                "--kind=sell",
                "--fiatcode=USD",
                "--fiatamount=10",
                "--paymentmethod=bank transfer",
                "--amount=10000",
            ]
        );
    }

    #[test]
    fn a_range_order_sends_min_dash_max() {
        let params = CreateOrderParams {
            fiat_amount: None,
            fiat_amount_min: Some(100),
            fiat_amount_max: Some(500),
            ..sell(0)
        };
        let args = args(&params).unwrap();
        assert!(args.contains(&"--fiatamount=100-500".to_string()));
    }

    #[test]
    fn fixed_and_range_amounts_are_exclusive_and_one_is_required() {
        let both = CreateOrderParams {
            fiat_amount_min: Some(1),
            fiat_amount_max: Some(2),
            ..sell(10)
        };
        assert!(args(&both).is_err());
        let neither = CreateOrderParams {
            fiat_amount: None,
            ..sell(10)
        };
        assert!(args(&neither).is_err());
        let half_range = CreateOrderParams {
            fiat_amount: None,
            fiat_amount_min: Some(1),
            ..sell(10)
        };
        assert!(args(&half_range).is_err());
    }

    #[test]
    fn amounts_must_be_positive_and_a_range_ordered() {
        assert!(args(&sell(0)).is_err());
        let inverted = CreateOrderParams {
            fiat_amount: None,
            fiat_amount_min: Some(500),
            fiat_amount_max: Some(100),
            ..sell(0)
        };
        assert!(args(&inverted).is_err());
    }

    #[test]
    fn a_negative_premium_stays_a_value() {
        let params = CreateOrderParams {
            premium: Some(-3),
            ..sell(10)
        };
        assert!(args(&params).unwrap().contains(&"--premium=-3".to_string()));
    }

    #[test]
    fn a_premium_that_gives_the_sats_away_is_rejected() {
        for premium in [-100, -150, i64::MIN] {
            let params = CreateOrderParams {
                premium: Some(premium),
                ..sell(10)
            };
            assert!(args(&params).is_err(), "{premium}");
        }
        let steep_discount = CreateOrderParams {
            premium: Some(-99),
            ..sell(10)
        };
        assert!(args(&steep_discount).is_ok());
    }

    #[test]
    fn only_a_buy_order_carries_an_invoice() {
        let sell_with_invoice = CreateOrderParams {
            invoice: Some("lnbcrt1pexample".into()),
            ..sell(10)
        };
        assert!(args(&sell_with_invoice).is_err());
        let buy_with_invoice = CreateOrderParams {
            kind: OrderKind::Buy,
            invoice: Some("lnbcrt1pexample".into()),
            ..sell(10)
        };
        assert!(
            args(&buy_with_invoice)
                .unwrap()
                .contains(&"--invoice=lnbcrt1pexample".to_string())
        );
    }

    #[test]
    fn a_payment_method_that_looks_like_a_flag_is_still_one_value() {
        let params = CreateOrderParams {
            payment_method: "--kind=buy".into(),
            ..sell(10)
        };
        let args = args(&params).unwrap();
        assert!(args.contains(&"--paymentmethod=--kind=buy".to_string()));
        assert!(args.contains(&"--kind=sell".to_string()));
    }
}
