//! `add_invoice` → `mostro-cli order add-invoice` and `add_bond_invoice` →
//! `mostro-cli bond add-invoice`: an invoice the node asked this identity for.
//! State-changing: refused unless the server runs with writes enabled.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{bolt11, flag, invoice, order_id};
use crate::error::Result;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AddInvoiceParams {
    /// The id (UUID) of the order the node asked an invoice for.
    pub order_id: String,
    /// `add_invoice`: a BOLT11 invoice or Lightning address for the sats you
    /// buy; a BOLT11 invoice must be for the order's amount and still valid for
    /// the node's invoice window. `add_bond_invoice`: a BOLT11 invoice for
    /// exactly your share of the slashed bond.
    pub invoice: String,
}

pub fn args(params: &AddInvoiceParams) -> Result<Vec<String>> {
    Ok(vec![
        "order".to_string(),
        "add-invoice".to_string(),
        flag("order", order_id("order_id", &params.order_id)?),
        flag("invoice", invoice("invoice", &params.invoice)?),
        "--yes".to_string(),
    ])
}

pub fn bond_args(params: &AddInvoiceParams) -> Result<Vec<String>> {
    Ok(vec![
        "bond".to_string(),
        "add-invoice".to_string(),
        flag("order", order_id("order_id", &params.order_id)?),
        flag("invoice", bolt11("invoice", &params.invoice)?),
        "--yes".to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ffdc768a-37d8-429e-bfcc-0968be56b6f9";

    fn params(invoice: &str) -> AddInvoiceParams {
        AddInvoiceParams {
            order_id: ID.into(),
            invoice: invoice.into(),
        }
    }

    #[test]
    fn an_invoice_for_the_sats_is_order_add_invoice() {
        assert_eq!(
            args(&params("lnbcrt1pexample")).unwrap(),
            [
                "order",
                "add-invoice",
                &format!("--order={ID}"),
                "--invoice=lnbcrt1pexample",
                "--yes"
            ]
        );
        assert!(args(&params("alice@getalby.com")).is_ok());
    }

    #[test]
    fn a_bond_share_is_bond_add_invoice_and_takes_bolt11_only() {
        assert_eq!(
            bond_args(&params("lnbcrt1pexample")).unwrap(),
            [
                "bond",
                "add-invoice",
                &format!("--order={ID}"),
                "--invoice=lnbcrt1pexample",
                "--yes"
            ]
        );
        assert!(bond_args(&params("alice@getalby.com")).is_err());
    }

    #[test]
    fn stdin_markers_flags_and_bad_ids_never_reach_the_cli() {
        // `-` makes the CLI read the invoice from standard input.
        for bad in ["-", "--help", ""] {
            assert!(args(&params(bad)).is_err(), "{bad}");
            assert!(bond_args(&params(bad)).is_err(), "{bad}");
        }
        let bad_id = AddInvoiceParams {
            order_id: "1; reboot".into(),
            ..params("lnbcrt1pexample")
        };
        assert!(args(&bad_id).is_err());
    }
}
