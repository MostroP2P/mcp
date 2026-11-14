//! `rate_counterparty` → `mostro-cli order rate`: the user's rating of the
//! counterparty of a finished trade, 1 to 5. State-changing and one-time:
//! refused unless the server runs with writes enabled.
//!
//! The CLI signs the request with the order's trade key, waits for the node's
//! `rate-received` and checks that it echoes the rating sent; this server only
//! validates the arguments and passes the outcome on.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::{flag, order_id};
use crate::error::{Result, invalid};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RateCounterpartyParams {
    /// The id (UUID) of the finished order whose counterparty is rated.
    pub order_id: String,
    /// The rating the user chose for the counterparty: 1 (worst) to 5 (best).
    /// Ask the user for it; never pick one yourself.
    #[schemars(range(min = 1, max = 5))]
    #[serde(deserialize_with = "whole_number")]
    pub rating: u8,
}

/// A `u8` written as a JSON integer or as a float with no fraction (`5.0`):
/// JSON Schema's `integer` allows both, and some hosts send every number as a
/// float. Anything else (a fraction, a string, out of range) is refused.
fn whole_number<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<u8, D::Error> {
    let number = serde_json::Number::deserialize(deserializer)?;
    let whole = number.as_u64().or_else(|| {
        number
            .as_f64()
            .filter(|f| f.fract() == 0.0 && (0.0..=f64::from(u8::MAX)).contains(f))
            .map(|f| f as u64)
    });
    whole.and_then(|n| u8::try_from(n).ok()).ok_or_else(|| {
        serde::de::Error::custom(format!("`{number}` is not a whole number from 0 to 255"))
    })
}

/// `order rate --order=<id> --rating=<n> --yes`.
pub fn args(params: &RateCounterpartyParams) -> Result<Vec<String>> {
    let order = order_id("order_id", &params.order_id)?;
    if !(1..=5).contains(&params.rating) {
        return Err(invalid("rating", "must be 1 to 5"));
    }
    Ok(vec![
        "order".to_string(),
        "rate".to_string(),
        flag("order", order),
        flag("rating", params.rating),
        "--yes".to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ffdc768a-37d8-429e-bfcc-0968be56b6f9";

    fn rating(rating: u8) -> RateCounterpartyParams {
        RateCounterpartyParams {
            order_id: ID.to_uppercase(),
            rating,
        }
    }

    #[test]
    fn a_rating_names_its_command_the_canonical_id_and_confirms() {
        assert_eq!(
            args(&rating(4)).unwrap(),
            [
                "order",
                "rate",
                &format!("--order={ID}"),
                "--rating=4",
                "--yes"
            ]
        );
    }

    #[test]
    fn the_lowest_and_highest_ratings_are_accepted() {
        assert_eq!(args(&rating(1)).unwrap()[3], "--rating=1");
        assert_eq!(args(&rating(5)).unwrap()[3], "--rating=5");
    }

    #[test]
    fn a_rating_outside_one_to_five_never_reaches_the_cli() {
        for out_of_range in [0, 6, u8::MAX] {
            let error = args(&rating(out_of_range)).unwrap_err().to_string();
            assert!(
                error.contains("rating") && error.contains("must be 1 to 5"),
                "{out_of_range}: {error}"
            );
        }
    }

    #[test]
    fn a_bad_id_never_reaches_the_cli() {
        for bad in ["--order=x", "not-a-uuid", ""] {
            let params = RateCounterpartyParams {
                order_id: bad.into(),
                rating: 5,
            };
            let error = args(&params).unwrap_err().to_string();
            assert!(error.contains("order_id"), "{bad}: {error}");
        }
    }

    #[test]
    fn a_whole_number_is_a_rating_however_json_writes_it() {
        for (written, rating) in [
            ("1", 1),
            ("3", 3),
            ("5", 5),
            ("5.0", 5),
            ("5e0", 5),
            ("0", 0),
        ] {
            let params: RateCounterpartyParams = serde_json::from_value(json_of(written))
                .unwrap_or_else(|e| panic!("{written}: {e}"));
            assert_eq!(params.rating, rating, "{written}");
        }
        // Read as 0 or 6, still refused before the CLI runs.
        for out_of_range in ["0.0", "6.0"] {
            let params: RateCounterpartyParams =
                serde_json::from_value(json_of(out_of_range)).unwrap();
            assert!(args(&params).is_err(), "{out_of_range}");
        }
    }

    #[test]
    fn a_rating_that_is_not_a_small_whole_number_is_refused_by_its_type() {
        for bad in [
            "-1", "256", "256.0", "4.5", "5.5", "-0.5", "\"5\"", "null", "true",
        ] {
            assert!(
                serde_json::from_value::<RateCounterpartyParams>(json_of(bad)).is_err(),
                "{bad}"
            );
        }
        let missing = serde_json::json!({ "order_id": ID });
        assert!(serde_json::from_value::<RateCounterpartyParams>(missing).is_err());
    }

    fn json_of(rating: &str) -> serde_json::Value {
        serde_json::from_str(&format!(r#"{{"order_id":"{ID}","rating":{rating}}}"#)).unwrap()
    }
}
