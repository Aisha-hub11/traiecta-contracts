//! Storage keys and the reads and writes that go with them.

use hyperion_core::HyperionError;
use soroban_sdk::{contracttype, Address, Env, String};

use crate::types::{AssetLink, ChainLane, Config};

/// Extend when there is less than a day of life left, and extend to thirty days.
pub const BUMP_THRESHOLD: u32 = 17_280;
pub const BUMP_TO: u32 = 518_400;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Config,
    /// Hyperion's chain name to Allbridge's chain id.
    Lane(String),
    /// Allbridge's chain id back to Hyperion's chain name.
    Chain(u32),
    /// A local asset plus a lane to the token it becomes at the far end.
    Asset(Address, u32),
}

pub fn config(env: &Env) -> Result<Config, HyperionError> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(HyperionError::NotInitialized)
}

pub fn set_config(env: &Env, cfg: &Config) {
    env.storage().instance().set(&DataKey::Config, cfg);
    env.storage().instance().extend_ttl(BUMP_THRESHOLD, BUMP_TO);
}

pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Config)
}

pub fn lane(env: &Env, chain: &String) -> Option<ChainLane> {
    let key = DataKey::Lane(chain.clone());
    let found: Option<ChainLane> = env.storage().persistent().get(&key);
    if found.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, BUMP_THRESHOLD, BUMP_TO);
    }
    found
}

/// The Hyperion name for an Allbridge chain id, which is what a status record wants to carry.
pub fn chain_of(env: &Env, allbridge_chain_id: u32) -> Option<String> {
    env.storage()
        .persistent()
        .get(&DataKey::Chain(allbridge_chain_id))
}

pub fn set_lane(env: &Env, lane: &ChainLane) {
    let by_name = DataKey::Lane(lane.chain.clone());
    let by_id = DataKey::Chain(lane.allbridge_chain_id);
    env.storage().persistent().set(&by_name, lane);
    env.storage().persistent().set(&by_id, &lane.chain);
    env.storage()
        .persistent()
        .extend_ttl(&by_name, BUMP_THRESHOLD, BUMP_TO);
    env.storage()
        .persistent()
        .extend_ttl(&by_id, BUMP_THRESHOLD, BUMP_TO);
}

pub fn asset(env: &Env, token: &Address, allbridge_chain_id: u32) -> Option<AssetLink> {
    let key = DataKey::Asset(token.clone(), allbridge_chain_id);
    let found: Option<AssetLink> = env.storage().persistent().get(&key);
    if found.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, BUMP_THRESHOLD, BUMP_TO);
    }
    found
}

pub fn set_asset(env: &Env, link: &AssetLink) {
    let key = DataKey::Asset(link.token.clone(), link.allbridge_chain_id);
    env.storage().persistent().set(&key, link);
    env.storage()
        .persistent()
        .extend_ttl(&key, BUMP_THRESHOLD, BUMP_TO);
}

/// Push a lane's expiry out without changing it. No-op when there is no such lane.
pub fn touch_lane(env: &Env, chain: &String) {
    let key = DataKey::Lane(chain.clone());
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, BUMP_THRESHOLD, BUMP_TO);
    }
}
