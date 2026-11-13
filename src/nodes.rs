//! The trusted Mostro nodes, and the node this server runs the CLI against.
//!
//! The catalog mirrors the nodes listed at <https://mostro.community> (source:
//! `MostroP2P/community`, `src/config/communities.ts`) plus Mostro Global, the
//! default. Each node's relays are the relay list it publishes (kind 10002),
//! with `wss://relay.mostro.network` first: mostro.community finds every node
//! there. The list is a constant on purpose: a node list a remote page can
//! change is a node list an attacker can change, so updating it is a pull
//! request.

use std::sync::{Arc, PoisonError, RwLock};

use serde::Serialize;

/// One node of the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TrustedNode {
    /// Stable identifier the agent passes to `select_node`.
    pub id: &'static str,
    /// The name the node gives itself in its info event (kind 38385).
    pub name: &'static str,
    /// The market it serves.
    pub region: &'static str,
    /// Lowercase hex public key.
    pub pubkey: &'static str,
    pub relays: &'static [&'static str],
}

/// The node used when `MOSTRO_PUBKEY` is unset.
pub const DEFAULT_NODE_ID: &str = "global";

const MOSTRO_RELAY: &str = "wss://relay.mostro.network";
const SHADOWBIP: &str = "wss://relay.shadowbip.com";
const MOSTRO_P2P_TECH: &str = "wss://mostro-p2p.tech";
const NOS_LOL: &str = "wss://nos.lol";
const KILOMBINO: &str = "wss://relay.kilombino.com";
const LULUS_MX: &str = "wss://nostrmxn.lulus.com.mx";
const BITMAXIS: &str = "wss://relay.bitmaxis.com";

pub const CATALOG: &[TrustedNode] = &[
    TrustedNode {
        id: "global",
        name: "Mostro Global",
        region: "Global",
        pubkey: "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390",
        relays: &[MOSTRO_RELAY, MOSTRO_P2P_TECH, SHADOWBIP],
    },
    TrustedNode {
        id: "cuba",
        name: "Kmbalache",
        region: "Cuba",
        pubkey: "00000235a3e904cfe1213a8a54d6f1ec1bef7cc6bfaabd6193e82931ccf1366a",
        relays: &[MOSTRO_RELAY, NOS_LOL, SHADOWBIP, MOSTRO_P2P_TECH],
    },
    TrustedNode {
        id: "spain",
        name: "NostroMostro",
        region: "España",
        pubkey: "0000cc02101ec29eea9ce623258752b9d7da66c27845ed26846dd0b0fc736b40",
        relays: &[MOSTRO_RELAY, MOSTRO_P2P_TECH, KILOMBINO, LULUS_MX],
    },
    TrustedNode {
        id: "colombia",
        name: "MostroColomBia",
        region: "Colombia",
        pubkey: "00000978acc594c506976c655b6decbf2d4af25ffdaa6680f2a9568b0a88441b",
        relays: &[MOSTRO_RELAY, SHADOWBIP, MOSTRO_P2P_TECH, NOS_LOL],
    },
    TrustedNode {
        id: "bolivia",
        name: "Mostro Boliviano",
        region: "Bolivia",
        pubkey: "00007cb3305fb972f5cc83f83a8fbca1e64e93c9d1369880a9fd62ef95d23f91",
        relays: &[MOSTRO_RELAY, SHADOWBIP, NOS_LOL, MOSTRO_P2P_TECH],
    },
    TrustedNode {
        id: "venezuela",
        name: "Mostro Venezuela",
        region: "Venezuela",
        pubkey: "000009ee1e4b1dc7add19ab30e4ef854d7b562e208b62686fd9002b50b24dabb",
        relays: &[MOSTRO_RELAY, MOSTRO_P2P_TECH, LULUS_MX, SHADOWBIP],
    },
    TrustedNode {
        id: "brazil",
        name: "Mostro Brasil",
        region: "Brasil",
        pubkey: "00037abd44e7a846689e230d5446abcd0d56a344fa81fff85c09d1929feda486",
        relays: &[MOSTRO_RELAY, SHADOWBIP, MOSTRO_P2P_TECH],
    },
    TrustedNode {
        id: "argentina",
        name: "MostrAR",
        region: "Argentina",
        pubkey: "b3626fe91b602bdbca3673bec0855221f41dc8f6d0e4027e51eaa525d68d87f2",
        relays: &[MOSTRO_RELAY, SHADOWBIP, MOSTRO_P2P_TECH, NOS_LOL],
    },
    TrustedNode {
        id: "europe",
        name: "MostroEuropa",
        region: "Europa",
        pubkey: "da23a31d75572138ab8149911a04224812a34bda679caba7cb1824fdf7c592ec",
        relays: &[MOSTRO_RELAY, NOS_LOL, MOSTRO_P2P_TECH, SHADOWBIP, KILOMBINO],
    },
    TrustedNode {
        id: "ecuador",
        name: "BitMaxis - Ecuador",
        region: "Ecuador",
        pubkey: "001bd4747d7d265edfe3bd3b7299886146ad850d51095fe77b763c32015685b9",
        relays: &[MOSTRO_RELAY, SHADOWBIP, BITMAXIS, MOSTRO_P2P_TECH],
    },
    TrustedNode {
        id: "mexico",
        name: "Mostro México",
        region: "México",
        pubkey: "00003f6be51b51a1a0cf9c94232ab1bba1f5c1bfd5a0e8687e9647558536b791",
        relays: &[MOSTRO_RELAY, LULUS_MX, SHADOWBIP],
    },
];

pub fn default_node() -> &'static TrustedNode {
    by_id(DEFAULT_NODE_ID).unwrap_or(&CATALOG[0])
}

/// The catalog node with this identifier, ignoring case and surrounding space.
pub fn by_id(id: &str) -> Option<&'static TrustedNode> {
    let id = id.trim();
    CATALOG.iter().find(|node| node.id.eq_ignore_ascii_case(id))
}

/// The catalog node with this public key, given as hex or npub.
pub fn by_pubkey(key: &str) -> Option<&'static TrustedNode> {
    let hex = if key.starts_with("npub1") {
        npub_to_hex(key)?
    } else {
        key.to_ascii_lowercase()
    };
    CATALOG.iter().find(|node| node.pubkey == hex)
}

/// The node the CLI runs against: its public key and relays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeTarget {
    pub pubkey: String,
    pub relays: Vec<String>,
}

impl NodeTarget {
    pub fn of(node: &TrustedNode) -> Self {
        Self {
            pubkey: node.pubkey.to_string(),
            relays: node.relays.iter().map(|relay| relay.to_string()).collect(),
        }
    }

    /// The catalog node this target is, when it is one with the catalog's own
    /// relays. A catalog key with operator-chosen relays is not that node's
    /// catalog entry, and must not be shown as one.
    pub fn catalog_node(&self) -> Option<&'static TrustedNode> {
        by_pubkey(&self.pubkey).filter(|node| node.relays == self.relays.as_slice())
    }
}

/// The node in use, shared by the server (which changes it on `select_node`)
/// and the CLI backend (which reads it for every run).
#[derive(Debug, Clone)]
pub struct NodeSelection(Arc<RwLock<NodeTarget>>);

impl NodeSelection {
    pub fn new(target: NodeTarget) -> Self {
        Self(Arc::new(RwLock::new(target)))
    }

    pub fn current(&self) -> NodeTarget {
        // The lock only ever guards a plain assignment, so a poisoned one
        // still holds a whole value.
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, target: NodeTarget) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = target;
    }
}

const BECH32_CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";

/// Decodes an `npub1…` to lowercase hex, checksum included (BIP-173), or
/// `None` when it is not a valid npub.
pub fn npub_to_hex(npub: &str) -> Option<String> {
    let data = npub.strip_prefix("npub1")?;
    if data.len() != 58 {
        return None;
    }
    let values = data
        .chars()
        .map(|c| BECH32_CHARSET.find(c).and_then(|i| u8::try_from(i).ok()))
        .collect::<Option<Vec<u8>>>()?;
    let mut checked: Vec<u8> = b"npub".iter().map(|b| b >> 5).collect();
    checked.push(0);
    checked.extend(b"npub".iter().map(|b| b & 31));
    checked.extend(&values);
    if bech32_polymod(&checked) != 1 {
        return None;
    }
    let payload = &values[..values.len() - 6];
    let mut bytes = Vec::with_capacity(32);
    let (mut acc, mut bits) = (0u32, 0u32);
    for value in payload {
        acc = ((acc << 5) | u32::from(*value)) & 0xfff;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push(u8::try_from((acc >> bits) & 0xff).ok()?);
        }
    }
    // Leftover bits are padding and must be zero.
    if bytes.len() != 32 || acc & ((1 << bits) - 1) != 0 {
        return None;
    }
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn bech32_polymod(values: &[u8]) -> u32 {
    const GENERATOR: [u32; 5] = [
        0x3b6a_57b2,
        0x2650_8e6d,
        0x1ea1_19fa,
        0x3d42_33dd,
        0x2a14_62b3,
    ];
    values.iter().fold(1u32, |chk, value| {
        let top = chk >> 25;
        let chk = ((chk & 0x01ff_ffff) << 5) ^ u32::from(*value);
        GENERATOR
            .iter()
            .enumerate()
            .filter(|(i, _)| (top >> i) & 1 == 1)
            .fold(chk, |chk, (_, g)| chk ^ g)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_mostro_global_plus_the_mostro_community_nodes() {
        let ids: Vec<_> = CATALOG.iter().map(|node| node.id).collect();
        assert_eq!(
            ids,
            [
                "global",
                "cuba",
                "spain",
                "colombia",
                "bolivia",
                "venezuela",
                "brazil",
                "argentina",
                "europe",
                "ecuador",
                "mexico"
            ]
        );
    }

    #[test]
    fn mostro_global_is_the_default_with_its_published_relays() {
        let node = default_node();
        assert_eq!(node.id, "global");
        assert_eq!(
            node.pubkey,
            "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390"
        );
        assert_eq!(
            node.relays,
            [
                "wss://relay.mostro.network",
                "wss://mostro-p2p.tech",
                "wss://relay.shadowbip.com"
            ]
        );
    }

    #[test]
    fn every_node_has_a_hex_key_and_relays_starting_with_the_mostro_relay() {
        for node in CATALOG {
            assert_eq!(node.pubkey.len(), 64, "{}", node.id);
            assert!(
                node.pubkey
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                "{}",
                node.id
            );
            assert_eq!(node.relays.first(), Some(&MOSTRO_RELAY), "{}", node.id);
            assert!(
                node.relays.iter().all(|relay| relay.starts_with("wss://")),
                "{}",
                node.id
            );
        }
    }

    #[test]
    fn identifiers_and_keys_are_unique() {
        for (i, node) in CATALOG.iter().enumerate() {
            for other in &CATALOG[i + 1..] {
                assert_ne!(node.id, other.id);
                assert_ne!(node.pubkey, other.pubkey);
            }
        }
    }

    #[test]
    fn nodes_are_found_by_identifier_ignoring_case() {
        assert_eq!(by_id(" Argentina ").map(|n| n.region), Some("Argentina"));
        assert!(by_id("atlantis").is_none());
        assert!(by_id("").is_none());
    }

    #[test]
    fn nodes_are_found_by_hex_or_npub_key() {
        assert_eq!(
            by_pubkey("B3626FE91B602BDBCA3673BEC0855221F41DC8F6D0E4027E51EAA525D68D87F2")
                .map(|n| n.id),
            Some("argentina")
        );
        assert_eq!(
            by_pubkey("npub1qqqvcqssrmpfa65uuc3jtp6jh8ta5ekz0pz76f5ydhgtplrnddqqrqe7xr")
                .map(|n| n.id),
            Some("spain")
        );
        assert!(by_pubkey(&"ab".repeat(32)).is_none());
    }

    #[test]
    fn an_npub_decodes_only_with_a_valid_checksum() {
        assert_eq!(
            npub_to_hex("npub1qqqr76l9rdg6rgx0nj2zx243hwsltsdl6ksws6r7jer4tpfkk7gsywukt9")
                .as_deref(),
            Some("00003f6be51b51a1a0cf9c94232ab1bba1f5c1bfd5a0e8687e9647558536b791")
        );
        // The last character changed: the checksum no longer matches.
        assert!(
            npub_to_hex("npub1qqqr76l9rdg6rgx0nj2zx243hwsltsdl6ksws6r7jer4tpfkk7gsywukt8")
                .is_none()
        );
        assert!(npub_to_hex("npub1short").is_none());
        assert!(npub_to_hex(&"ab".repeat(32)).is_none());
    }

    #[test]
    fn a_target_is_a_catalog_node_only_with_the_catalog_relays() {
        let argentina = by_id("argentina").unwrap();
        assert_eq!(
            NodeTarget::of(argentina).catalog_node().map(|n| n.id),
            Some("argentina")
        );
        let custom = NodeTarget {
            pubkey: argentina.pubkey.to_string(),
            relays: vec!["ws://localhost:7000".into()],
        };
        assert!(custom.catalog_node().is_none());
    }

    #[test]
    fn a_selection_is_shared_between_its_clones() {
        let selection = NodeSelection::new(NodeTarget::of(default_node()));
        let other = selection.clone();
        other.set(NodeTarget::of(by_id("mexico").unwrap()));
        assert_eq!(selection.current().pubkey, by_id("mexico").unwrap().pubkey);
    }
}
