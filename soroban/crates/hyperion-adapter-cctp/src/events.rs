//! What this adapter announces.
//!
//! Soroban allows four topics per event, and the pair at the front of each one here spends two of
//! them on `hyperion` and on the rail plus direction. That leaves two for the fields a subscriber
//! actually filters on, which in practice means the asset and the far side's domain. Everything
//! else lives in the body, where it costs nothing to carry and is still part of the contract spec,
//! so the indexer reads these as real types rather than as loose maps.
//!
//! All four configuration changes share one topic pair on purpose. The monitoring job that cares
//! about them cares about all of them at once, and wants a single subscription that fires whenever
//! somebody rewires a rail.

use soroban_sdk::{contractevent, Address, BytesN, String};

/// Funds handed to Circle on the way out.
#[contractevent(topics = ["hyperion", "cctp_out"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispatched {
    #[topic]
    pub token: Address,
    #[topic]
    pub destination_domain: u32,
    pub amount: i128,
    pub mint_recipient: BytesN<32>,
    pub max_fee: i128,
    pub min_finality_threshold: u32,
    /// The router's own outbound nonce, so the two halves of the trail can be joined up.
    pub router_nonce: u64,
}

/// A message Circle attested and minted, and that this adapter passed on to the router.
#[contractevent(topics = ["hyperion", "cctp_in"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Received {
    #[topic]
    pub token: Address,
    #[topic]
    pub source_domain: u32,
    /// What was actually minted, which is what the sender sent minus whatever Circle charged.
    pub amount: i128,
    pub nonce: BytesN<32>,
    pub source_chain: String,
    pub recipient: Address,
    /// Nonzero when the router had to park the delivery instead of handing it straight over.
    pub claim_id: u64,
    pub relayer: Address,
}

/// Leftovers moved out of the adapter and into the treasury.
#[contractevent(topics = ["hyperion", "cctp_dust"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Swept {
    #[topic]
    pub token: Address,
    pub amount: i128,
    pub treasury: Address,
    pub caller: Address,
}

#[contractevent(topics = ["hyperion", "cctp_cfg"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkSet {
    #[topic]
    pub domain: u32,
    pub chain: String,
    pub destination_caller: BytesN<32>,
}

#[contractevent(topics = ["hyperion", "cctp_cfg"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetMapped {
    #[topic]
    pub domain: u32,
    pub burn_token: BytesN<32>,
    pub local: Address,
}

#[contractevent(topics = ["hyperion", "cctp_cfg"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeePolicySet {
    pub max_fee_bps: u32,
    pub min_finality_threshold: u32,
}

#[contractevent(topics = ["hyperion", "cctp_cfg"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminChanged {
    #[topic]
    pub old: Address,
    #[topic]
    pub new: Address,
}
