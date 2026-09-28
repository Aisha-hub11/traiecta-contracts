//! Hyperion's adapter for Allbridge Core.
//!
//! Allbridge Core is the pooled rail. There is no attestation to wait for on the way out and no
//! wrapped asset at the far end: a transfer sells into a pool on this side, crosses as an
//! internal unit Allbridge calls vUSD, and buys out of a pool on the other. That makes it the
//! fastest route Hyperion can offer and the only one where the amount that arrives is not the
//! amount that left. Slippage is the price of not waiting.
//!
//! # Outbound only, and why
//!
//! This adapter has no inbound leg. Not as a simplification, and not as something to finish
//! later. Allbridge's arrival call is
//!
//! ```text
//! receive_tokens(sender, amount, recipient: Address, source_chain_id, receive_token,
//!                nonce, receive_amount_min, extra_gas)
//! ```
//!
//! and the hash the validators signed covers `(amount, recipient, source_chain_id,
//! destination_chain_id, receive_token, nonce)`. There is no payload field, no callback, and no
//! spare byte in the attested message. `recipient` is a plain Stellar address and the pool pays
//! it directly.
//!
//! So a Hyperion destination has nowhere to travel. Sending funds to this contract and hoping to
//! work out who they belong to would mean guessing from an amount and a nonce, which is how you
//! lose somebody's money. Instead the router's inbound receiver for the Allbridge route is left
//! unset, which makes `bridge_in` on this route fail closed by construction rather than by
//! intent, and inbound traffic from the pooled rail is simply Allbridge's own flow paying the
//! recipient directly. That is the honest shape of the rail, and it is also perfectly usable:
//! the user gets their tokens, they just do not get them through Hyperion.
//!
//! One consequence worth stating plainly: `receive_amount_min`, the slippage guard, is chosen by
//! whoever calls `receive_tokens` on the destination chain. Outbound from Stellar that is not
//! Hyperion, and `swap_and_bridge` has no minimum of its own, so this contract cannot promise an
//! amount out. The route planner quotes Allbridge as the lossy option for that reason.
//!
//! # The gas float
//!
//! Allbridge charges for relaying, in native token, at the moment the transfer is made. There is
//! no equivalent of Axelar's `add_gas` to top a message up afterwards, so the payment has to be
//! attached up front or the message is never carried.
//!
//! The bridge takes it two ways: `gas_amount`, pulled from the sender in native token, or
//! `fee_token_amount`, carved out of the transfer itself and converted at the gas oracle's
//! price. Hyperion pays in native. Paying from the transfer would mean this contract reproducing
//! Allbridge's fee conversion arithmetic to work out how much to carve off, getting it wrong by
//! a rounding unit on a price that moves, and silently delivering less than the router told the
//! user it would.
//!
//! So the contract holds a small native float and spends from it. Anybody may top it up and the
//! balance is a view, so a keeper watches one number and a transfer that would arrive unrelayed
//! is refused before it starts rather than after the funds are gone. The cost is read live from
//! the bridge and the messenger on every dispatch, never configured, because it moves with the
//! destination chain's gas price. `sweep` refuses to touch the native asset, so the float cannot
//! be tidied away by a well meaning keeper.
//!
//! # What this contract does not do
//!
//! It does not verify anything. It holds no pool, quotes no price, and trusts Allbridge's own
//! numbers for the pool address, the relay cost and the rebalancer. Every one of those is read
//! at the moment it is used rather than written down here, so there is nothing to keep in step.

#![no_std]

mod events;
mod storage;

// Public so the rail interface this adapter was built against is readable from outside it, and
// so the deploy tooling can generate bindings from these shapes rather than transcribe them a
// second time.
pub mod rail;
pub mod types;

#[cfg(test)]
mod test;

use hyperion_core::{
    address::bytes32_to_evm, codec, inbound::RouterClient, HyperionError, RouteKind,
};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, token, vec, Address, BytesN, Env, IntoVal, String, Symbol, U256,
};

use rail::{AllbridgeBridgeClient, AllbridgeMessengerClient, STELLAR_CHAIN_ID};
use types::{AssetLink, ChainLane, Config};

#[contract]
pub struct AllbridgeAdapter;

#[contractimpl]
impl AllbridgeAdapter {
    /// Wire the adapter up.
    ///
    /// Starts with no lane and no asset, so a deploy script that stops halfway leaves something
    /// that refuses every transfer rather than something that guesses.
    pub fn initialize(
        env: Env,
        admin: Address,
        router: Address,
        bridge: Address,
        native: Address,
    ) -> Result<(), HyperionError> {
        if storage::is_initialized(&env) {
            return Err(HyperionError::AlreadyInitialized);
        }
        storage::set_config(
            &env,
            &Config {
                admin,
                router,
                bridge,
                native,
            },
        );
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // Outbound
    // ---------------------------------------------------------------------------------------

    /// Sell funds the router has already moved here into Allbridge's pool and send them on.
    ///
    /// By the time this runs the money is sitting in this contract's balance, so the work is
    /// working out who to pay, what it costs to be relayed, and authorising exactly the two
    /// calls Allbridge will make back through this contract's identity.
    pub fn dispatch(
        env: Env,
        caller: Address,
        token: Address,
        amount: i128,
        destination_chain: String,
        destination: BytesN<32>,
        nonce: u64,
    ) -> Result<(), HyperionError> {
        caller.require_auth();
        let cfg = storage::config(&env)?;
        if caller != cfg.router {
            return Err(HyperionError::Unauthorized);
        }
        if amount <= 0 {
            return Err(HyperionError::InvalidAmount);
        }
        let lane = storage::lane(&env, &destination_chain).ok_or(HyperionError::UnknownChain)?;
        let link = storage::asset(&env, &token, lane.allbridge_chain_id)
            .ok_or(HyperionError::TokenNotMapped)?;

        // Allbridge wants the recipient as thirty two bytes and will take any thirty two bytes
        // it is given. Checking here means a mistyped destination comes back as a Hyperion
        // refusal instead of arriving somewhere unrecoverable.
        let _ = bytes32_to_evm(&env, &destination)?;

        let this = env.current_contract_address();
        let bridge = AllbridgeBridgeClient::new(&env, &cfg.bridge);
        let bridge_cfg = bridge.get_config();

        // Read live. The pool for an asset can be repointed by Allbridge's admin, and a stale
        // copy here would authorise a transfer to the wrong contract.
        let token_key = codec::contract_key(&env, &token)?;
        let pool = bridge.get_pool_address(&BytesN::from_array(&env, &token_key));

        // The relay is billed in two pieces and the bridge checks the sum, so both have to be
        // asked for. Neither is stored.
        let messenger = AllbridgeMessengerClient::new(&env, &bridge_cfg.messenger);
        let cost = bridge
            .get_transaction_cost(&lane.allbridge_chain_id)
            .checked_add(messenger.get_transaction_cost(&lane.allbridge_chain_id))
            .ok_or(HyperionError::DecimalOverflow)?;
        let cost_i128 = i128::try_from(cost).map_err(|_| HyperionError::DecimalOverflow)?;

        let native = token::Client::new(&env, &cfg.native);
        if native.balance(&this) < cost_i128 {
            return Err(HyperionError::GasFloatTooLow);
        }

        let amount_u128 = u128::try_from(amount).map_err(|_| HyperionError::InvalidAmount)?;
        // Allbridge waives the pool fee for its own rebalancer and passes that decision down as
        // an argument, so the authorisation has to name the same value the bridge will. Computed
        // rather than assumed false, because an authorisation that disagrees by one boolean
        // fails at the bottom of somebody's transfer.
        let zero_fee = bridge_cfg.rebalancer == this;

        env.authorize_as_current_contract(vec![
            &env,
            // The pool asks this contract to sign for the sale, and then the token asks again
            // one frame deeper for the transfer into the pool.
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: pool.clone(),
                    fn_name: Symbol::new(&env, "swap_to_v_usd"),
                    args: (this.clone(), amount_u128, zero_fee).into_val(&env),
                },
                sub_invocations: vec![
                    &env,
                    InvokerContractAuthEntry::Contract(SubContractInvocation {
                        context: ContractContext {
                            contract: token.clone(),
                            fn_name: Symbol::new(&env, "transfer"),
                            args: (this.clone(), pool.clone(), amount).into_val(&env),
                        },
                        sub_invocations: vec![&env],
                    }),
                ],
            }),
            // And the bridge takes the relay fee out of the float on its own account.
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: cfg.native.clone(),
                    fn_name: Symbol::new(&env, "transfer"),
                    args: (this.clone(), cfg.bridge.clone(), cost_i128).into_val(&env),
                },
                sub_invocations: vec![&env],
            }),
        ]);

        // The router's nonce is globally unique and monotonic, which is what keeps Allbridge's
        // own sent-message hash from colliding across two identical transfers.
        let rail_nonce = U256::from_u128(&env, u128::from(nonce));

        bridge.swap_and_bridge(
            &this,
            &token,
            &amount_u128,
            &destination,
            &lane.allbridge_chain_id,
            &link.receive_token,
            &rail_nonce,
            &cost,
            // Zero, always. The float pays, not the transfer.
            &0u128,
        );

        events::Dispatched {
            token,
            chain: lane.chain,
            allbridge_chain_id: lane.allbridge_chain_id,
            amount,
            recipient: destination,
            receive_token: link.receive_token,
            nonce: rail_nonce,
            gas_spent: cost,
        }
        .publish(&env);
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // The gas float
    // ---------------------------------------------------------------------------------------

    /// Put native token into the float. Open to anybody.
    ///
    /// Deliberately permissionless. The float is not a balance anybody can take out, so there is
    /// nothing to gain by filling it and quite a lot to lose by having only one address able to.
    /// A keeper normally does this; on the day the keeper is down, anybody can.
    pub fn fund_gas(env: Env, funder: Address, amount: i128) -> Result<i128, HyperionError> {
        funder.require_auth();
        let cfg = storage::config(&env)?;
        if amount <= 0 {
            return Err(HyperionError::InvalidAmount);
        }
        let this = env.current_contract_address();
        let native = token::Client::new(&env, &cfg.native);
        native.transfer(&funder, &this, &amount);
        let balance = native.balance(&this);
        events::GasFunded {
            funder,
            amount,
            balance,
        }
        .publish(&env);
        Ok(balance)
    }

    /// Take native token back out of the float. Admin only.
    ///
    /// The one call on this contract that moves value somewhere an admin chooses, which is why
    /// it is the only one gated this tightly. It exists so a retired adapter's float is not
    /// stranded, and because the alternative to having it is topping the float up forever.
    pub fn withdraw_gas(env: Env, to: Address, amount: i128) -> Result<i128, HyperionError> {
        let cfg = require_admin(&env)?;
        if amount <= 0 {
            return Err(HyperionError::InvalidAmount);
        }
        let this = env.current_contract_address();
        let native = token::Client::new(&env, &cfg.native);
        if native.balance(&this) < amount {
            return Err(HyperionError::GasFloatTooLow);
        }
        native.transfer(&this, &to, &amount);
        let balance = native.balance(&this);
        events::GasWithdrawn {
            to,
            amount,
            balance,
        }
        .publish(&env);
        Ok(balance)
    }

    /// What the float holds right now.
    pub fn gas_balance(env: Env) -> Result<i128, HyperionError> {
        let cfg = storage::config(&env)?;
        Ok(token::Client::new(&env, &cfg.native).balance(&env.current_contract_address()))
    }

    /// What the next transfer to this chain will cost to be relayed, read live.
    ///
    /// Both halves of the bill, summed the way the bridge sums them. The route planner shows
    /// this so a user is told the route is short of gas before they sign rather than after.
    pub fn quote_gas(env: Env, chain: String) -> Result<u128, HyperionError> {
        let cfg = storage::config(&env)?;
        let lane = storage::lane(&env, &chain).ok_or(HyperionError::UnknownChain)?;
        let bridge = AllbridgeBridgeClient::new(&env, &cfg.bridge);
        let messenger = AllbridgeMessengerClient::new(&env, &bridge.get_config().messenger);
        bridge
            .get_transaction_cost(&lane.allbridge_chain_id)
            .checked_add(messenger.get_transaction_cost(&lane.allbridge_chain_id))
            .ok_or(HyperionError::DecimalOverflow)
    }

    // ---------------------------------------------------------------------------------------
    // Housekeeping anybody may do
    // ---------------------------------------------------------------------------------------

    /// Send a stray balance on to the treasury.
    ///
    /// Pooled routes round, and people send tokens to contracts by hand. An admin only rescue
    /// would be a standing power to move tokens out of a contract users send tokens to, so this
    /// is open to anybody and the destination is read live from the router. The native asset is
    /// refused, because that balance is the gas float and it is there on purpose.
    pub fn sweep(env: Env, caller: Address, token: Address) -> Result<i128, HyperionError> {
        caller.require_auth();
        let cfg = storage::config(&env)?;
        if token == cfg.native {
            return Err(HyperionError::ProtectedAsset);
        }
        let this = env.current_contract_address();
        let asset = token::Client::new(&env, &token);
        let balance = asset.balance(&this);
        if balance <= 0 {
            return Ok(0);
        }
        let treasury = RouterClient::new(&env, &cfg.router).treasury();
        asset.transfer(&this, &treasury, &balance);
        events::Swept {
            token,
            amount: balance,
            treasury,
            caller,
        }
        .publish(&env);
        Ok(balance)
    }

    /// Keep a quiet lane's storage from being archived. Permissionless, changes nothing.
    pub fn keep_alive(env: Env, chain: String) -> Result<(), HyperionError> {
        let _ = storage::config(&env)?;
        env.storage()
            .instance()
            .extend_ttl(storage::BUMP_THRESHOLD, storage::BUMP_TO);
        storage::touch_lane(&env, &chain);
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // Administration
    // ---------------------------------------------------------------------------------------

    /// Pair Hyperion's name for a chain with Allbridge's number for it.
    ///
    /// Checked against Allbridge rather than taken on trust. A chain id with no bridge
    /// registered on the far side is a lane whose first transfer would revert from two contracts
    /// away, and the sum of this check is one cross-contract read while an admin is watching.
    pub fn link_chain(
        env: Env,
        chain: String,
        allbridge_chain_id: u32,
    ) -> Result<(), HyperionError> {
        let cfg = require_admin(&env)?;
        if chain.is_empty() {
            return Err(HyperionError::UnknownChain);
        }
        // Allbridge refuses a destination equal to its own id, so a lane pointing home is a lane
        // that can never carry anything.
        if allbridge_chain_id == STELLAR_CHAIN_ID {
            return Err(HyperionError::UnknownChain);
        }
        AllbridgeBridgeClient::new(&env, &cfg.bridge)
            .try_get_another_bridge(&allbridge_chain_id)
            .map_err(|_| HyperionError::UnknownChain)?
            .map_err(|_| HyperionError::UnknownChain)?;

        let lane = ChainLane {
            chain,
            allbridge_chain_id,
        };
        storage::set_lane(&env, &lane);
        events::LaneSet {
            chain: lane.chain,
            allbridge_chain_id: lane.allbridge_chain_id,
        }
        .publish(&env);
        Ok(())
    }

    /// Say which token a local asset becomes at the far end of a lane.
    ///
    /// Both halves are checked: Allbridge has to hold a pool for the local asset, and the
    /// destination bridge has to accept the token being named. Getting either wrong produces a
    /// transfer that sells into a pool and then reverts on delivery.
    pub fn link_asset(
        env: Env,
        token: Address,
        chain: String,
        receive_token: BytesN<32>,
    ) -> Result<(), HyperionError> {
        let cfg = require_admin(&env)?;
        let lane = storage::lane(&env, &chain).ok_or(HyperionError::UnknownChain)?;
        let bridge = AllbridgeBridgeClient::new(&env, &cfg.bridge);

        let token_key = codec::contract_key(&env, &token)?;
        bridge
            .try_get_pool_address(&BytesN::from_array(&env, &token_key))
            .map_err(|_| HyperionError::TokenNotMapped)?
            .map_err(|_| HyperionError::TokenNotMapped)?;

        let remote = bridge
            .try_get_another_bridge(&lane.allbridge_chain_id)
            .map_err(|_| HyperionError::UnknownChain)?
            .map_err(|_| HyperionError::UnknownChain)?;
        if remote.tokens.get(receive_token.clone()) != Some(true) {
            return Err(HyperionError::TokenNotMapped);
        }

        let link = AssetLink {
            token,
            allbridge_chain_id: lane.allbridge_chain_id,
            receive_token,
        };
        storage::set_asset(&env, &link);
        events::AssetLinked {
            token: link.token,
            allbridge_chain_id: link.allbridge_chain_id,
            receive_token: link.receive_token,
        }
        .publish(&env);
        Ok(())
    }

    /// Hand the admin role to somebody else.
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), HyperionError> {
        let mut cfg = require_admin(&env)?;
        let old = cfg.admin.clone();
        cfg.admin = new_admin.clone();
        storage::set_config(&env, &cfg);
        events::AdminChanged {
            old,
            new: new_admin,
        }
        .publish(&env);
        Ok(())
    }

    /// Replace this contract's code.
    ///
    /// An adapter is the one part of Hyperion that has to change when somebody else's rail
    /// changes, so being able to upgrade it is the difference between shipping a fix and
    /// redeploying the world.
    pub fn upgrade(env: Env, wasm_hash: BytesN<32>) -> Result<(), HyperionError> {
        require_admin(&env)?;
        env.deployer()
            .update_current_contract(soroban_sdk::ContractExecutable::Wasm(wasm_hash));
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // Views
    // ---------------------------------------------------------------------------------------

    pub fn get_config(env: Env) -> Result<Config, HyperionError> {
        storage::config(&env)
    }

    /// The route this adapter serves, which is only ever the one.
    pub fn route(env: Env) -> Result<RouteKind, HyperionError> {
        let _ = storage::config(&env)?;
        Ok(RouteKind::Allbridge)
    }

    pub fn get_lane(env: Env, chain: String) -> Result<ChainLane, HyperionError> {
        storage::lane(&env, &chain).ok_or(HyperionError::UnknownChain)
    }

    /// Hyperion's name for the chain Allbridge numbers this.
    pub fn chain_for(env: Env, allbridge_chain_id: u32) -> Result<String, HyperionError> {
        storage::chain_of(&env, allbridge_chain_id).ok_or(HyperionError::UnknownChain)
    }

    pub fn get_asset(env: Env, token: Address, chain: String) -> Result<AssetLink, HyperionError> {
        let lane = storage::lane(&env, &chain).ok_or(HyperionError::UnknownChain)?;
        storage::asset(&env, &token, lane.allbridge_chain_id).ok_or(HyperionError::TokenNotMapped)
    }

    /// The pool Allbridge currently holds this asset in, read live.
    pub fn pool_for(env: Env, token: Address) -> Result<Address, HyperionError> {
        let cfg = storage::config(&env)?;
        let token_key = codec::contract_key(&env, &token)?;
        AllbridgeBridgeClient::new(&env, &cfg.bridge)
            .try_get_pool_address(&BytesN::from_array(&env, &token_key))
            .map_err(|_| HyperionError::TokenNotMapped)?
            .map_err(|_| HyperionError::TokenNotMapped)
    }

    /// Whether Allbridge is currently accepting transfers at all.
    ///
    /// Allbridge's own stop authority can halt swapping, and a route planner that did not ask
    /// would keep offering a rail that reverts on every attempt.
    pub fn rail_open(env: Env) -> Result<bool, HyperionError> {
        let cfg = storage::config(&env)?;
        Ok(AllbridgeBridgeClient::new(&env, &cfg.bridge)
            .get_config()
            .can_swap)
    }
}

/// Read the config and insist the admin signed for whatever is about to happen.
fn require_admin(env: &Env) -> Result<Config, HyperionError> {
    let cfg = storage::config(env)?;
    cfg.admin.require_auth();
    Ok(cfg)
}
