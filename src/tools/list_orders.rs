//! `list_orders` → `mostro-cli listorders`: the node's public order book
//! (Kind 38383 events), read from the relays.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{OrderKind, fiat_code, flag};
use crate::error::Result;

#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ListOrdersParams {
    /// Only orders of this side. `sell`: the maker sells sats, so taking it
    /// means you buy sats. `buy`: the maker buys sats, so taking it means you
    /// sell sats.
    #[serde(default)]
    pub kind: Option<OrderKind>,
    /// Only orders in this fiat currency, e.g. `USD`, `ARS`, `VES`.
    #[serde(default)]
    pub currency: Option<String>,
    /// Book status to list. Defaults to `pending`: orders that can be taken.
    #[serde(default)]
    pub status: Option<BookStatus>,
}

/// The statuses an order shows in the public book (NIP-69). A trade's
/// private status never reaches the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BookStatus {
    Pending,
    InProgress,
    Success,
    Canceled,
}

impl BookStatus {
    fn as_cli(self) -> &'static str {
        match self {
            BookStatus::Pending => "pending",
            BookStatus::InProgress => "in-progress",
            BookStatus::Success => "success",
            BookStatus::Canceled => "canceled",
        }
    }
}

pub fn args(params: &ListOrdersParams) -> Result<Vec<String>> {
    let mut args = vec!["listorders".to_string()];
    if let Some(kind) = params.kind {
        args.push(flag("kind", kind.as_cli()));
    }
    if let Some(currency) = &params.currency {
        args.push(flag("currency", fiat_code("currency", currency)?));
    }
    if let Some(status) = params.status {
        args.push(flag("status", status.as_cli()));
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_filters_lists_the_pending_book() {
        assert_eq!(args(&ListOrdersParams::default()).unwrap(), ["listorders"]);
    }

    #[test]
    fn every_filter_becomes_a_flag() {
        let params = ListOrdersParams {
            kind: Some(OrderKind::Sell),
            currency: Some("usd".into()),
            status: Some(BookStatus::InProgress),
        };
        assert_eq!(
            args(&params).unwrap(),
            [
                "listorders",
                "--kind=sell",
                "--currency=USD",
                "--status=in-progress"
            ]
        );
    }

    #[test]
    fn a_currency_that_is_not_a_code_is_rejected() {
        let params = ListOrdersParams {
            currency: Some("--help".into()),
            ..Default::default()
        };
        assert!(args(&params).is_err());
    }

    #[test]
    fn status_and_kind_deserialize_from_their_wire_names() {
        let params: ListOrdersParams =
            serde_json::from_str(r#"{"kind":"buy","status":"in-progress"}"#).unwrap();
        assert_eq!(params.kind, Some(OrderKind::Buy));
        assert_eq!(params.status, Some(BookStatus::InProgress));
    }
}
