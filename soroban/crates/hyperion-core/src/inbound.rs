//! The shape an inbound delivery takes on its way from a rail adapter into the router.
//!
//! These types live here rather than in the router because both sides of the handover need
//! them and neither side should have to link the other. An adapter that depended on the
//! router crate would drag the router's exported entry points into its own WASM, which is a
//! very confusing thing to find in a deployed contract.

use soroban_sdk::{contractclient, contracttype, Address, Bytes, BytesN, Env, String};

use crate::address::AddressKind;
use crate::error::HyperionError;
use crate::route::RouteKind;

/// A resolved Stellar recipient, as handed to the router by an adapter.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recipient {
    pub address: Address,
    pub kind: AddressKind,
    /// The raw payload the adapter decoded this from, kept so the event trail shows exactly
    /// what arrived rather than only what it was interpreted as.
    pub raw: Bytes,
}

/// Where an inbound transfer came from, as the adapter parsed it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Origin {
    pub chain: String,
    /// The rail's own sequence number for this message, kept so a person reading the explorer
    /// can line our record up against Circle's or Axelar's. Not the replay key: see below.
    pub nonce: u64,
    /// The rail's message identifier at full width, and the only thing replay protection keys
    /// on. Thirty two bytes because that is what every rail here can produce without being
    /// squeezed, whether it starts life as a CCTP nonce, an Axelar message id or a hash.
    pub message_id: BytesN<32>,
    /// The sender on the far side, kept for the indexer. Not trusted for anything.
    pub sender: BytesN<32>,
}

/// The slice of the router an adapter is allowed to know about.
///
/// Two calls, and that is the whole surface. An adapter hands over funds it has verified and
/// asks where the fees go. It cannot pause anything, cannot register a token and cannot read
/// another adapter's configuration, which keeps a compromised adapter to the one rail it was
/// ever wired to.
#[contractclient(name = "RouterClient")]
pub trait Router {
    /// Deliver funds that arrived over `route`, pulled out of the calling adapter.
    fn bridge_in(
        env: Env,
        caller: Address,
        route: RouteKind,
        token: Address,
        amount: i128,
        recipient: Recipient,
        origin: Origin,
    ) -> Result<u64, HyperionError>;

    /// Where the protocol sends anything it collects.
    fn treasury(env: Env) -> Result<Address, HyperionError>;
}
