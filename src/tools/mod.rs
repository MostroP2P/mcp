//! The four tools, each as a typed parameter struct plus a pure function that
//! turns it into `mostro-cli` arguments. Building is kept apart from running so
//! every rule here is unit-tested without a node.
//!
//! Arguments are always passed as `--flag=value`: a value that starts with `-`
//! then stays a value and can never be read by the CLI as another flag.

pub mod create_order;
pub mod get_order;
pub mod list_orders;
pub mod take_order;

use std::fmt::Display;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Result, invalid};

/// An order's side, named as Mostro names it: by what its maker does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OrderKind {
    /// The maker buys sats and pays fiat.
    Buy,
    /// The maker sells sats and receives fiat.
    Sell,
}

impl OrderKind {
    pub fn as_cli(self) -> &'static str {
        match self {
            OrderKind::Buy => "buy",
            OrderKind::Sell => "sell",
        }
    }
}

pub(crate) fn flag(name: &str, value: impl Display) -> String {
    format!("--{name}={value}")
}

/// Longest invoice accepted, in characters: BOLT11 strings stay well under it.
const MAX_INVOICE_CHARS: usize = 2048;

/// A fiat currency code: 3 to 5 ASCII letters, returned uppercase.
pub(crate) fn fiat_code(field: &'static str, code: &str) -> Result<String> {
    let code = code.trim();
    if !(3..=5).contains(&code.len()) || !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(invalid(
            field,
            format!("`{code}` is not a fiat currency code (3 to 5 letters, e.g. USD)"),
        ));
    }
    Ok(code.to_ascii_uppercase())
}

/// A canonical (lowercase, hyphenated) order UUID.
pub(crate) fn order_id(field: &'static str, id: &str) -> Result<String> {
    uuid::Uuid::parse_str(id.trim())
        .map(|uuid| uuid.hyphenated().to_string())
        .map_err(|_| invalid(field, format!("`{id}` is not an order id (a UUID)")))
}

/// Free text such as a payment method: trimmed, non-empty, at most `max`
/// characters, no control characters.
pub(crate) fn text(field: &'static str, value: &str, max: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid(field, "must not be empty"));
    }
    if value.chars().count() > max {
        return Err(invalid(field, format!("must be at most {max} characters")));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(field, "must not contain control characters"));
    }
    Ok(value.to_string())
}

/// A BOLT11 invoice or a Lightning address (`user@domain`).
pub(crate) fn invoice(field: &'static str, value: &str) -> Result<String> {
    let value = value.trim();
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-' | '+');
    if value.is_empty()
        || value.len() > MAX_INVOICE_CHARS
        || value.starts_with('-')
        || !value.chars().all(allowed)
    {
        return Err(invalid(
            field,
            "expected a BOLT11 invoice or a Lightning address (user@domain)",
        ));
    }
    Ok(value.to_string())
}

/// A positive amount the CLI can hold (it parses amounts as `i64`).
pub(crate) fn positive(field: &'static str, amount: u64) -> Result<u64> {
    if amount == 0 || amount > i64::MAX as u64 {
        return Err(invalid(field, "must be a positive amount"));
    }
    Ok(amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiat_codes_are_uppercased_and_must_be_letters() {
        assert_eq!(fiat_code("c", "ars").unwrap(), "ARS");
        assert_eq!(fiat_code("c", " usd ").unwrap(), "USD");
        assert!(fiat_code("c", "U$D").is_err());
        assert!(fiat_code("c", "--help").is_err());
        assert!(fiat_code("c", "US").is_err());
        assert!(fiat_code("c", "TOOLONG").is_err());
    }

    #[test]
    fn order_ids_must_be_uuids_and_come_back_canonical() {
        assert_eq!(
            order_id("o", "FFDC768A-37D8-429E-BFCC-0968BE56B6F9").unwrap(),
            "ffdc768a-37d8-429e-bfcc-0968be56b6f9"
        );
        assert!(order_id("o", "not-a-uuid").is_err());
        assert!(order_id("o", "").is_err());
        assert!(order_id("o", "--orderid=x").is_err());
    }

    #[test]
    fn text_is_trimmed_bounded_and_free_of_control_characters() {
        assert_eq!(text("m", "  bank transfer ", 50).unwrap(), "bank transfer");
        assert!(text("m", "   ", 50).is_err());
        assert!(text("m", &"x".repeat(51), 50).is_err());
        assert!(text("m", "bank\ntransfer", 50).is_err());
    }

    #[test]
    fn invoices_are_bolt11_or_lightning_addresses() {
        assert!(invoice("i", "lnbcrt10u1pjexample0").is_ok());
        assert!(invoice("i", "alice@getalby.com").is_ok());
        assert!(invoice("i", "lnbc1 2").is_err());
        assert!(invoice("i", "$(whoami)").is_err());
        assert!(invoice("i", "").is_err());
    }
}
