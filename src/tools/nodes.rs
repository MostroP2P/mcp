//! `list_nodes` and `select_node`: the trusted node catalog and the switch to
//! one of its nodes. Neither builds CLI arguments of its own; `select_node`
//! reads `order mine` to refuse a switch that would strand an open order.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{Result, invalid};
use crate::nodes::{CATALOG, NodeTarget, TrustedNode, by_id};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SelectNodeParams {
    /// Identifier of a node from list_nodes (e.g. `global`, `argentina`).
    pub node: String,
    /// Switch even though the local store lists open orders. Set it only when
    /// the user confirmed in this conversation that those orders are finished
    /// or abandoned: an open trade can only be completed on its own node.
    #[serde(default)]
    pub leave_open_orders: bool,
}

/// The catalog node `params` names; anything else is refused before any run.
pub fn target(params: &SelectNodeParams) -> Result<&'static TrustedNode> {
    by_id(&params.node).ok_or_else(|| {
        let known: Vec<_> = CATALOG.iter().map(|node| node.id).collect();
        invalid(
            "node",
            format!(
                "`{}` is not a trusted node; use one of: {}",
                params.node.trim(),
                known.join(", ")
            ),
        )
    })
}

/// One catalog node as the agent sees it.
pub fn describe(node: &TrustedNode) -> Value {
    json!({
        "id": node.id,
        "name": node.name,
        "region": node.region,
        "pubkey": node.pubkey,
        "relays": node.relays,
    })
}

/// The node in use: its catalog entry, or its key and relays when it is not
/// one (a node configured with `MOSTRO_PUBKEY` / `RELAYS`).
pub fn describe_target(target: &NodeTarget) -> Value {
    match target.catalog_node() {
        Some(node) => describe(node),
        None => json!({ "id": null, "pubkey": target.pubkey, "relays": target.relays }),
    }
}

/// `list_nodes`: the catalog, with the node in use marked.
pub fn catalog(current: &NodeTarget) -> Value {
    let in_use = current.catalog_node().map(|node| node.id);
    let nodes: Vec<Value> = CATALOG
        .iter()
        .map(|node| {
            let mut entry = describe(node);
            entry["in_use"] = json!(Some(node.id) == in_use);
            entry
        })
        .collect();
    json!({ "nodes": nodes, "current": describe_target(current) })
}

/// Statuses after which an order needs nothing more from its node.
const FINISHED: &[&str] = &[
    "success",
    "canceled",
    "canceled-by-admin",
    "settled-by-admin",
    "completed-by-admin",
    "cooperatively-canceled",
    "expired",
];

/// The orders of an `order mine` document that are not finished, as
/// `{id, status}`. A row without a readable status counts as open: unsure
/// means stay.
pub fn open_orders(data: &Value) -> Option<Vec<Value>> {
    let orders = data.get("orders")?.as_array()?;
    Some(
        orders
            .iter()
            .filter(|order| {
                order
                    .get("status")
                    .and_then(Value::as_str)
                    .is_none_or(|status| !FINISHED.contains(&status))
            })
            .map(|order| json!({ "id": order.get("id"), "status": order.get("status") }))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::default_node;

    fn params(node: &str) -> SelectNodeParams {
        SelectNodeParams {
            node: node.into(),
            leave_open_orders: false,
        }
    }

    #[test]
    fn only_catalog_identifiers_are_accepted() {
        assert_eq!(target(&params("Argentina")).unwrap().id, "argentina");
        let error = target(&params(&"ab".repeat(32))).unwrap_err().to_string();
        assert!(error.contains("not a trusted node"), "{error}");
        assert!(error.contains("global"), "{error}");
        assert!(target(&params("")).is_err());
    }

    #[test]
    fn the_catalog_marks_the_node_in_use() {
        let listed = catalog(&NodeTarget::of(default_node()));
        let in_use: Vec<_> = listed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| node["in_use"] == true)
            .map(|node| node["id"].clone())
            .collect();
        assert_eq!(in_use, [json!("global")]);
        assert_eq!(listed["current"]["id"], "global");
        assert_eq!(listed["nodes"].as_array().unwrap().len(), CATALOG.len());
    }

    #[test]
    fn a_node_outside_the_catalog_is_shown_by_key_and_none_is_in_use() {
        let custom = NodeTarget {
            pubkey: "ab".repeat(32),
            relays: vec!["ws://localhost:7000".into()],
        };
        let listed = catalog(&custom);
        assert_eq!(listed["current"]["id"], Value::Null);
        assert_eq!(listed["current"]["pubkey"], "ab".repeat(32));
        assert!(
            listed["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|node| node["in_use"] == false)
        );
    }

    #[test]
    fn finished_orders_are_not_open_and_unreadable_ones_are() {
        let data = json!({ "orders": [
            { "id": "a", "status": "pending" },
            { "id": "b", "status": "success" },
            { "id": "c", "status": "fiat-sent" },
            { "id": "d", "status": "canceled" },
            { "id": "e" },
        ]});
        let ids: Vec<_> = open_orders(&data)
            .unwrap()
            .iter()
            .map(|order| order["id"].clone())
            .collect();
        assert_eq!(ids, [json!("a"), json!("c"), json!("e")]);
    }

    #[test]
    fn a_document_without_orders_is_not_readable() {
        assert!(open_orders(&json!({})).is_none());
        assert_eq!(open_orders(&json!({ "orders": [] })), Some(vec![]));
    }
}
