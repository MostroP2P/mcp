//! `rate_counterparty` → `mostro-cli order rate`: the user's rating of the
//! counterparty of a finished trade, 1 to 5. State-changing and one-time:
//! refused unless the server runs with writes enabled.
//!
//! The CLI signs the request with the order's trade key, waits for the node's
//! `rate-received` and checks that it echoes the rating sent; this server only
//! validates the arguments and passes the outcome on.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{flag, order_id};
use crate::error::{Result, invalid};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RateCounterpartyParams {
    /// The id (UUID) of the finished order whose counterparty is rated.
    pub order_id: String,
    /// The rating the user chose for the counterparty: 1 (worst) to 5 (best).
    /// Ask the user for it; never pick one yourself.
    #[schemars(range(min = 1, max = 5))]
    pub rating: u8,
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
    fn a_rating_that_is_not_a_small_whole_number_is_refused_by_its_type() {
        for bad in [
            json_of("-1"),
            json_of("256"),
            json_of("4.5"),
            json_of("\"5\""),
        ] {
            assert!(
                serde_json::from_value::<RateCounterpartyParams>(bad.clone()).is_err(),
                "{bad}"
            );
        }
        assert!(serde_json::from_value::<RateCounterpartyParams>(json_of("3")).is_ok());
    }

    fn json_of(rating: &str) -> serde_json::Value {
        serde_json::from_str(&format!(r#"{{"order_id":"{ID}","rating":{rating}}}"#)).unwrap()
    }
}
