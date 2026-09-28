//! One world builder shared by every test in this crate.
//!
//! The router here is the real one. Allbridge's whole shape depends on funds already having been
//! moved into this contract before `dispatch` is called, and a stand-in router would move them
//! in whatever way made the adapter pass.

use hyperion_core::{codec, RouteKind, DEFAULT_FLOW_WINDOW_LEDGERS, STELLAR_DECIMALS};
use hyperion_router::{
    AdminAction, Destination, HyperionRouter, HyperionRouterClient, OutboundRequest,
    MIN_TIMELOCK_DELAY,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String,
};

use super::allbridge::{
    MockBridge, MockBridgeClient, MockMessenger, MockMessengerClient, MockPool, MockPoolClient,
};
use crate::{AllbridgeAdapter, AllbridgeAdapterClient};

/// Ten basis points, the protocol fee the router is set up with.
pub const FEE_BPS: u32 = 10;
/// USDC on every EVM chain Hyperion targets. Seven decimals here, six there.
pub const EVM_DECIMALS: u32 = 6;
/// One hundred units of a seven decimal asset.
pub const HUNDRED: i128 = 1_000_000_000;
pub const FLOW_LIMIT: i128 = 10_000_000_000;

/// Allbridge's own numbering. Stellar is seven, which is why a lane pointing at seven is refused.
pub const ETH_CHAIN_ID: u32 = 1;
/// A second chain, for the tests that need two lanes to be genuinely separate.
pub const BASE_CHAIN_ID: u32 = 9;
/// A chain id nobody registered a bridge for.
pub const NOWHERE_CHAIN_ID: u32 = 42;

/// What Allbridge charges to carry a message to Ethereum, in native stroops.
pub const BRIDGE_COST: u128 = 3_000_000;
/// And what the messenger charges on top, which is the half that is easy to forget.
pub const MESSENGER_COST: u128 = 2_000_000;
/// The two together, which is the number that actually has to be affordable.
pub const RELAY_COST: u128 = BRIDGE_COST + MESSENGER_COST;

/// Thirty basis points of slippage in the pool, so a test can tell the amount that went in apart
/// from the amount that crossed.
pub const POOL_FEE_BPS: u32 = 30;

/// What the adapter starts with to pay for relaying, in native stroops.
pub const GAS_FLOAT: i128 = 500_000_000;

pub struct World {
    pub env: Env,
    pub adapter_id: Address,
    pub router_id: Address,
    pub bridge_id: Address,
    pub messenger_id: Address,
    pub pool_id: Address,
    /// The asset being bridged.
    pub token_id: Address,
    /// Native, which is what relaying is paid in and the one asset `sweep` refuses to touch.
    pub native_id: Address,
    /// An asset Allbridge holds no pool for, for the mapping refusals.
    pub orphan_id: Address,
    pub admin: Address,
    pub guardian: Address,
    pub treasury: Address,
    pub user: Address,
    pub relayer: Address,
    /// Allbridge's rebalancer, which is somebody else by default and occasionally this adapter.
    pub rebalancer: Address,
}

impl World {
    /// Everything wired, Ethereum linked, the asset mapped, the user funded, the float filled.
    pub fn new() -> Self {
        let world = Self::unlinked();
        world.link_ethereum();
        world.map_token();
        world.fund_user(HUNDRED * 10);
        world.fund_gas(GAS_FLOAT);
        world
    }

    /// Wired and open, but with no lane linked and no asset mapped yet.
    pub fn unlinked() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| {
            li.sequence_number = 100_000;
            li.timestamp = 1_700_000_000;
            li.min_persistent_entry_ttl = 4096;
            li.min_temp_entry_ttl = 16;
            li.max_entry_ttl = 6_312_000;
        });

        let admin = Address::generate(&env);
        let guardian = Address::generate(&env);
        let treasury = Address::generate(&env);
        let user = Address::generate(&env);
        let relayer = Address::generate(&env);
        let rebalancer = Address::generate(&env);
        // Whoever issues the test assets. Not part of the world, because nothing under test ever
        // needs to know who it is.
        let issuer = Address::generate(&env);

        let token_id = env
            .register_stellar_asset_contract_v2(issuer.clone())
            .address();
        let native_id = env
            .register_stellar_asset_contract_v2(issuer.clone())
            .address();
        let orphan_id = env
            .register_stellar_asset_contract_v2(issuer.clone())
            .address();

        let messenger_id = env.register(MockMessenger, ());
        MockMessengerClient::new(&env, &messenger_id).initialize(&native_id);

        let bridge_id = env.register(MockBridge, ());
        MockBridgeClient::new(&env, &bridge_id).initialize(&native_id, &messenger_id, &rebalancer);

        let pool_id = env.register(MockPool, ());
        MockPoolClient::new(&env, &pool_id).initialize(&bridge_id, &token_id, &POOL_FEE_BPS);

        let router_id = env.register(HyperionRouter, ());
        HyperionRouterClient::new(&env, &router_id).initialize(
            &admin,
            &guardian,
            &treasury,
            &FEE_BPS,
            &DEFAULT_FLOW_WINDOW_LEDGERS,
            &MIN_TIMELOCK_DELAY,
        );

        let adapter_id = env.register(AllbridgeAdapter, ());
        AllbridgeAdapterClient::new(&env, &adapter_id)
            .initialize(&admin, &router_id, &bridge_id, &native_id);

        let world = World {
            env,
            adapter_id,
            router_id,
            bridge_id,
            messenger_id,
            pool_id,
            token_id,
            native_id,
            orphan_id,
            admin,
            guardian,
            treasury,
            user,
            relayer,
            rebalancer,
        };

        world.wire_rail();

        world.run_action(AdminAction::SetAdapter(
            RouteKind::Allbridge,
            world.adapter_id.clone(),
        ));
        world.run_action(AdminAction::EnableRoute(RouteKind::Allbridge));
        // Deliberately no `SetRailReceiver`. Allbridge's attested message has no room for a
        // Hyperion destination, so there is no inbound leg, and leaving the slot empty is what
        // makes `bridge_in` on this route impossible rather than merely unimplemented.
        world.run_action(AdminAction::RegisterToken(
            world.token_id.clone(),
            STELLAR_DECIMALS,
            FLOW_LIMIT,
        ));
        world
    }

    /// Teach the stand-in rail about itself: a pool, two remote bridges, and what relaying costs.
    fn wire_rail(&self) {
        let bridge = self.bridge();
        bridge.add_pool(&self.pool_id, &self.token_key(&self.token_id));
        bridge.register_bridge(&ETH_CHAIN_ID, &self.remote_bridge(0xE1));
        bridge.add_bridge_token(&ETH_CHAIN_ID, &self.receive_token());
        bridge.register_bridge(&BASE_CHAIN_ID, &self.remote_bridge(0xBA));
        bridge.add_bridge_token(&BASE_CHAIN_ID, &self.receive_token());
        bridge.set_cost(&ETH_CHAIN_ID, &BRIDGE_COST);
        bridge.set_cost(&BASE_CHAIN_ID, &BRIDGE_COST);
        self.messenger().set_cost(&ETH_CHAIN_ID, &MESSENGER_COST);
        self.messenger().set_cost(&BASE_CHAIN_ID, &MESSENGER_COST);

        // Upstream's bridge pays the messenger out of its own balance and only afterwards pulls
        // the sender's gas, so a bridge that has never carried anything has to start with a
        // float of its own or the very first transfer fails for a reason that has nothing to do
        // with this adapter.
        self.mint_native(&self.bridge_id, GAS_FLOAT);
    }

    // -------------------------------------------------------------------------------------
    // Clients
    // -------------------------------------------------------------------------------------

    pub fn adapter(&self) -> AllbridgeAdapterClient<'_> {
        AllbridgeAdapterClient::new(&self.env, &self.adapter_id)
    }

    pub fn router(&self) -> HyperionRouterClient<'_> {
        HyperionRouterClient::new(&self.env, &self.router_id)
    }

    pub fn bridge(&self) -> MockBridgeClient<'_> {
        MockBridgeClient::new(&self.env, &self.bridge_id)
    }

    pub fn messenger(&self) -> MockMessengerClient<'_> {
        MockMessengerClient::new(&self.env, &self.messenger_id)
    }

    pub fn pool(&self) -> MockPoolClient<'_> {
        MockPoolClient::new(&self.env, &self.pool_id)
    }

    pub fn token(&self) -> token::Client<'_> {
        token::Client::new(&self.env, &self.token_id)
    }

    pub fn native(&self) -> token::Client<'_> {
        token::Client::new(&self.env, &self.native_id)
    }

    // -------------------------------------------------------------------------------------
    // Wiring helpers
    // -------------------------------------------------------------------------------------

    /// Queue an admin action on the router, wait out the delay, execute it.
    pub fn run_action(&self, action: AdminAction) {
        let id = self.router().queue_action(&self.admin, &action);
        self.env.ledger().with_mut(|li| {
            li.timestamp += MIN_TIMELOCK_DELAY + 1;
        });
        self.router().execute_action(&self.admin, &id);
    }

    pub fn link_ethereum(&self) {
        self.adapter().link_chain(&self.ethereum(), &ETH_CHAIN_ID);
    }

    pub fn map_token(&self) {
        self.adapter()
            .link_asset(&self.token_id, &self.ethereum(), &self.receive_token());
    }

    pub fn fund_user(&self, amount: i128) {
        token::StellarAssetClient::new(&self.env, &self.token_id).mint(&self.user, &amount);
    }

    /// Put the asset straight into the adapter, which is where the router leaves it just before
    /// it calls `dispatch`. Used by the tests that call `dispatch` on its own.
    pub fn fund_adapter(&self, amount: i128) {
        token::StellarAssetClient::new(&self.env, &self.token_id).mint(&self.adapter_id, &amount);
    }

    pub fn mint_native(&self, to: &Address, amount: i128) {
        token::StellarAssetClient::new(&self.env, &self.native_id).mint(to, &amount);
    }

    /// Top the adapter's relay float up the way anybody could: mint to a stranger, have the
    /// stranger pay it in. Nothing here uses a back door the keeper would not have.
    pub fn fund_gas(&self, amount: i128) -> i128 {
        self.mint_native(&self.relayer, amount);
        self.adapter().fund_gas(&self.relayer, &amount)
    }

    // -------------------------------------------------------------------------------------
    // Names and ids
    // -------------------------------------------------------------------------------------

    pub fn ethereum(&self) -> String {
        String::from_str(&self.env, "ethereum")
    }

    pub fn base(&self) -> String {
        String::from_str(&self.env, "base")
    }

    pub fn token_key(&self, token: &Address) -> BytesN<32> {
        BytesN::from_array(&self.env, &codec::contract_key(&self.env, token).unwrap())
    }

    /// The far side's Allbridge bridge, as the thirty two bytes the registry holds.
    pub fn remote_bridge(&self, fill: u8) -> BytesN<32> {
        BytesN::from_array(&self.env, &[fill; 32])
    }

    /// USDC on the far side, as Allbridge names it.
    pub fn receive_token(&self) -> BytesN<32> {
        let mut raw = [0u8; 32];
        raw[12..].copy_from_slice(&[0xA0; 20]);
        BytesN::from_array(&self.env, &raw)
    }

    /// A token the destination bridge has never heard of.
    pub fn unknown_receive_token(&self) -> BytesN<32> {
        let mut raw = [0u8; 32];
        raw[12..].copy_from_slice(&[0x5D; 20]);
        BytesN::from_array(&self.env, &raw)
    }

    /// A left-padded EVM address ending in `last`, which is what a destination word looks like.
    pub fn evm(&self, last: u8) -> BytesN<32> {
        let mut raw = [0u8; 32];
        raw[12..].copy_from_slice(&[0x7C; 20]);
        raw[31] = last;
        BytesN::from_array(&self.env, &raw)
    }

    pub fn zero_word(&self) -> BytesN<32> {
        BytesN::from_array(&self.env, &[0u8; 32])
    }

    // -------------------------------------------------------------------------------------
    // Outbound requests
    // -------------------------------------------------------------------------------------

    /// The default request these tests send, ready to be bent out of shape.
    pub fn request(&self, amount: i128) -> OutboundRequest {
        OutboundRequest {
            token: self.token_id.clone(),
            amount,
            route: RouteKind::Allbridge,
            destination: Destination {
                chain: self.ethereum(),
                address: self.evm(0x01),
            },
            destination_decimals: EVM_DECIMALS,
            min_destination_amount: 0,
        }
    }
}

// ------------------------------------------------------------------------------------------
// The arithmetic, repeated here so a test can say what it expects
// ------------------------------------------------------------------------------------------

/// What the router keeps.
pub fn fee_of(amount: i128) -> i128 {
    amount * i128::from(FEE_BPS) / 10_000
}

/// What reaches the adapter, once the fee is off and the remainder has been floored for six
/// decimals.
pub fn net_of(amount: i128) -> i128 {
    let net = amount - fee_of(amount);
    net - net % 10
}

/// What leaves the sender's balance.
pub fn gross_of(amount: i128) -> i128 {
    fee_of(amount) + net_of(amount)
}

/// What survives the pool, which is the number the far side is eventually paid from.
pub fn after_pool(net: i128) -> u128 {
    let net = net as u128;
    net - net * u128::from(POOL_FEE_BPS) / 10_000
}
