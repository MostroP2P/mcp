//! The check that the installed `mostro-cli` fits this server.
//!
//! The CLI publishes its own contract: `mostro-cli --output json schema` lists
//! every command with the output modes it supports. At startup the server
//! checks that each command its enabled tools need is listed with `json`, and
//! stops with the missing names rather than failing later in odd ways.

use serde_json::Value;

use crate::error::{Error, Result};
use crate::outcome::{Outcome, Status};

/// The canonical commands the read-only tools need.
const READ: [&str; 6] = [
    "order.list",
    "order.info",
    "order.mine",
    "message.list",
    "order.watch",
    "order.chat",
];

/// The canonical commands the state-changing tools need.
const WRITE: [&str; 11] = [
    "order.new",
    "order.take-sell",
    "order.take-buy",
    "order.add-invoice",
    "bond.add-invoice",
    "order.fiat-sent",
    "order.release",
    "order.cancel",
    "order.sync",
    "dispute.open",
    "order.rate",
];

/// The commands the enabled tools need: the reads always, the writes only on a
/// server that allows them.
pub fn required(allow_writes: bool) -> Vec<&'static str> {
    let mut needed = READ.to_vec();
    if allow_writes {
        needed.extend(WRITE);
    }
    needed
}

/// Check the outcome of `schema` against `needed`.
pub fn verify(schema: &Outcome, needed: &[&str]) -> Result<()> {
    // A failure is the CLI's own diagnosis (bad configuration, bad arguments):
    // pass it on rather than a generic "no list of commands".
    if schema.status() != Status::Ok {
        let error = schema.document().get("error").unwrap_or(&Value::Null);
        return Err(Error::Config(format!(
            "mostro-cli answered `schema` with `{}`: {error}",
            schema.document()["status"].as_str().unwrap_or("?")
        )));
    }
    let listed = (schema.command() == Some("schema"))
        .then(|| schema.data())
        .flatten()
        .and_then(|data| data.get("commands"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            Error::Config("mostro-cli did not answer `schema` with its list of commands".into())
        })?;
    let with_json = |name: &str| {
        listed.iter().any(|entry| {
            entry.get("name").and_then(Value::as_str) == Some(name)
                && entry
                    .get("output_modes")
                    .and_then(Value::as_array)
                    .is_some_and(|modes| modes.iter().any(|mode| mode == "json"))
        })
    };
    let missing: Vec<String> = needed
        .iter()
        .filter(|name| !with_json(name))
        .map(|name| (*name).to_string())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Error::ContractMismatch { missing })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTCOMES: &str = include_str!("../tests/fixtures/outcomes.json");

    fn schema_with(edit: impl FnOnce(&mut Value)) -> Outcome {
        let all: Value = serde_json::from_str(OUTCOMES).unwrap();
        let mut stdout = all["schema"]["stdout"].clone();
        edit(&mut stdout);
        Outcome::parse(&stdout.to_string(), Some(0)).unwrap()
    }

    #[test]
    fn the_cli_of_this_contract_has_everything_every_tool_needs() {
        let schema = schema_with(|_| {});
        assert!(verify(&schema, &required(true)).is_ok());
        assert!(verify(&schema, &required(false)).is_ok());
    }

    #[test]
    fn a_missing_command_or_one_without_json_is_named() {
        let schema = schema_with(|doc| {
            let commands = doc["data"]["commands"].as_array_mut().unwrap();
            commands.retain(|c| c["name"] != "order.take-buy");
            for command in commands.iter_mut() {
                if command["name"] == "order.new" {
                    command["output_modes"] = serde_json::json!(["human"]);
                }
            }
        });
        let Err(Error::ContractMismatch { missing }) = verify(&schema, &required(true)) else {
            panic!("expected a mismatch");
        };
        assert_eq!(missing, ["order.new", "order.take-buy"]);
        // The read-only server does not need them.
        assert!(verify(&schema, &required(false)).is_ok());
    }

    #[test]
    fn a_cli_that_is_not_the_new_one_is_refused_clearly() {
        // The old CLI's `schema` does not exist; any other command's outcome
        // is not a list of commands.
        let not_schema = Outcome::parse(
            &serde_json::json!({"schema_version":"1","command":"order.list","status":"ok",
                "data":{"orders":[]},"error":null,"warnings":[],"meta":{}})
            .to_string(),
            Some(0),
        )
        .unwrap();
        assert!(matches!(
            verify(&not_schema, &required(false)),
            Err(Error::Config(_))
        ));
        let refused = Outcome::parse(
            &serde_json::json!({"schema_version":"1","command":null,"status":"error",
                "data":null,"error":{"code":"invalid_arguments"},"warnings":[],"meta":{}})
            .to_string(),
            Some(2),
        )
        .unwrap();
        assert!(verify(&refused, &required(false)).is_err());
    }

    #[test]
    fn a_failed_schema_keeps_the_error_of_the_cli() {
        let failed = Outcome::parse(
            &serde_json::json!({"schema_version":"1","command":"schema","status":"error",
                "data":null,"error":{"code":"invalid_arguments",
                "message":"The command arguments are invalid."},"warnings":[],"meta":{}})
            .to_string(),
            Some(2),
        )
        .unwrap();
        let Err(Error::Config(message)) = verify(&failed, &required(false)) else {
            panic!("expected a configuration error");
        };
        assert!(
            message.contains("invalid_arguments")
                && message.contains("The command arguments are invalid."),
            "{message}"
        );
    }

    #[test]
    fn the_writes_are_required_only_when_enabled() {
        assert_eq!(
            required(false),
            [
                "order.list",
                "order.info",
                "order.mine",
                "message.list",
                "order.watch",
                "order.chat"
            ]
        );
        for write in [
            "dispute.open",
            "order.take-sell",
            "order.add-invoice",
            "bond.add-invoice",
            "order.fiat-sent",
            "order.release",
            "order.cancel",
            "order.sync",
            "order.rate",
        ] {
            assert!(required(true).contains(&write), "{write}");
        }
    }
}
