//! One world builder shared by every test in this crate.
//!
//! Two adapters are wired up rather than one, because the interesting thing about this contract
//! is that the same code answers to two different routes and behaves differently on each. A
//! world that only ever built the shape carrying a note would never notice the day the bare
//! shape grew an inbound leg.

use hyperion_core::{
    axelar::{InboundNote, NOTE_VERSION},
    codec,
    strkey::{StrkeyDestination, STRKEY_LEN_MUXED, STRKEY_LEN_PLAIN},
    AddressKind, RouteKind, DEFAULT_FLOW_WINDOW_LEDGERS, STELLAR_DECIMALS,
};
use hyperion_router::{
    AdminAction, Destination, HyperionRouter, HyperionRouterClient, OutboundRequest,
    MIN_TIMELOCK_DELAY,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Bytes, BytesN, Env, String,
};

use super::its::{MockIts, MockItsClient, MockTokenManager, MockTokenManagerClient};
use crate::rail::TokenManagerType;
use crate::{AxelarAdapter, AxelarAdapterClient};

/// Ten basis points, the protocol fee the router is set up with.
pub const FEE_BPS: u32 = 10;
/// USDC on every EVM chain Hyperion targets. Seven decimals here, six there.
pub const EVM_DECIMALS: u32 = 6;
/// One hundred units of a seven decimal asset.
pub const HUNDRED: i128 = 1_000_000_000;
pub const FLOW_LIMIT: i128 = 10_000_000_000;

/// Axelar's id for the asset whose manager burns, which is three of the four kinds.
pub const MB_TOKEN_ID: [u8; 32] = [0x7A; 32];
/// And for the one that locks instead, which is the only kind that names a third party.
pub const LU_TOKEN_ID: [u8; 32] = [0x10; 32];
/// A token id Axelar has never heard of.
pub const STRANGE_TOKEN_ID: [u8; 32] = [0x99; 32];

/// Hyperion's own contract on the far side, which is the only address a note is accepted from.
pub const PEER: [u8; 20] = [
    0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF,
    0xC0, 0xC1, 0xC2, 0xC3,
];

pub struct World {
    pub env: Env,
    /// The shape that carries a note, which is the one most tests are about.
    pub adapter_id: Address,
    /// The bare shape, which hands ITS the recipient directly and has no inbound leg.
    pub bare_id: Address,
    pub router_id: Address,
    pub its_id: Address,
    pub manager_id: Address,
    /// The asset whose Axelar token id burns on the way out.
    pub token_id: Address,
    /// The asset whose token id locks instead.
    pub lock_asset: Address,
    pub admin: Address,
    pub guardian: Address,
    pub treasury: Address,
    pub user: Address,
    pub relayer: Address,
    /// Where a well formed note pays by default. A contract, because a contract holds an asset
    /// the moment somebody sends it one.
    pub recipient: Address,
    pub recipient_key: [u8; 32],
    /// A real classic account with no trustline for the asset, which is how a delivery ends up
    /// parked as a claim rather than handed over.
    pub classic: Address,
    pub classic_key: [u8; 32],
}

impl World {
    /// Everything wired, Ethereum linked on both shapes, both assets mapped, the user funded.
    pub fn new() -> Self {
        let world = Self::unlinked();
        world.link_ethereum();
        world.map_tokens();
        world.fund_user(HUNDRED * 10);
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

        // Two destinations, because the two kinds behave differently and the difference is the
        // interesting part. `Address::generate` hands back contract ids, so this one is a C
        // address and can be paid on the spot.
        let recipient = Address::generate(&env);
        let recipient_key = codec::contract_key(&env, &recipient).unwrap();

        // And a real G address, built from a key the tests can also write into a note. It has no
        // trustline for the asset and no way to open one, which is precisely the situation the
        // router's parked claims exist for.
        let classic_key = [0x44u8; 32];
        let classic = Address::from_string(&codec::account_strkey(&env, &classic_key));

        // ITS has to exist before the assets do, because it is their admin. That is what lets it
        // mint without a mocked signature: it is the direct invoker of the token, so its
        // authority is implicit rather than granted by the test harness.
        let its_id = env.register(MockIts, ());
        let token_id = env
            .register_stellar_asset_contract_v2(its_id.clone())
            .address();
        let lock_asset = env
            .register_stellar_asset_contract_v2(its_id.clone())
            .address();
        let manager_id = env.register(MockTokenManager, ());

        let its = MockItsClient::new(&env, &its_id);
        its.register(
            &BytesN::from_array(&env, &MB_TOKEN_ID),
            &token_id,
            &manager_id,
            &TokenManagerType::MintBurn,
        );
        its.register(
            &BytesN::from_array(&env, &LU_TOKEN_ID),
            &lock_asset,
            &manager_id,
            &TokenManagerType::LockUnlock,
        );
        its.set_trusted(&String::from_str(&env, "Ethereum"), &true);

        let router_id = env.register(HyperionRouter, ());
        HyperionRouterClient::new(&env, &router_id).initialize(
            &admin,
            &guardian,
            &treasury,
            &FEE_BPS,
            &DEFAULT_FLOW_WINDOW_LEDGERS,
            &MIN_TIMELOCK_DELAY,
        );

        let adapter_id = env.register(AxelarAdapter, ());
        AxelarAdapterClient::new(&env, &adapter_id).initialize(
            &admin,
            &router_id,
            &its_id,
            &RouteKind::AxelarGmp,
        );
        let bare_id = env.register(AxelarAdapter, ());
        AxelarAdapterClient::new(&env, &bare_id).initialize(
            &admin,
            &router_id,
            &its_id,
            &RouteKind::AxelarIts,
        );

        let world = World {
            env,
            adapter_id,
            bare_id,
            router_id,
            its_id,
            manager_id,
            token_id,
            lock_asset,
            admin,
            guardian,
            treasury,
            user,
            relayer,
            recipient,
            recipient_key,
            classic,
            classic_key,
        };

        world.run_action(AdminAction::SetAdapter(
            RouteKind::AxelarGmp,
            world.adapter_id.clone(),
        ));
        world.run_action(AdminAction::SetRailReceiver(
            RouteKind::AxelarGmp,
            world.adapter_id.clone(),
        ));
        world.run_action(AdminAction::EnableRoute(RouteKind::AxelarGmp));

        // The bare shape gets an adapter and an open route and deliberately no rail receiver.
        // Nothing ever arrives on it, so there is nobody for the router to accept a `bridge_in`
        // from, and leaving the slot empty is what makes that true rather than merely intended.
        world.run_action(AdminAction::SetAdapter(
            RouteKind::AxelarIts,
            world.bare_id.clone(),
        ));
        world.run_action(AdminAction::EnableRoute(RouteKind::AxelarIts));

        world.run_action(AdminAction::RegisterToken(
            world.token_id.clone(),
            STELLAR_DECIMALS,
            FLOW_LIMIT,
        ));
        world.run_action(AdminAction::RegisterToken(
            world.lock_asset.clone(),
            STELLAR_DECIMALS,
            FLOW_LIMIT,
        ));
        world
    }

    // -------------------------------------------------------------------------------------
    // Clients
    // -------------------------------------------------------------------------------------

    pub fn adapter(&self) -> AxelarAdapterClient<'_> {
        AxelarAdapterClient::new(&self.env, &self.adapter_id)
    }

    pub fn bare(&self) -> AxelarAdapterClient<'_> {
        AxelarAdapterClient::new(&self.env, &self.bare_id)
    }

    pub fn router(&self) -> HyperionRouterClient<'_> {
        HyperionRouterClient::new(&self.env, &self.router_id)
    }

    pub fn its(&self) -> MockItsClient<'_> {
        MockItsClient::new(&self.env, &self.its_id)
    }

    pub fn manager(&self) -> MockTokenManagerClient<'_> {
        MockTokenManagerClient::new(&self.env, &self.manager_id)
    }

    pub fn token(&self) -> token::Client<'_> {
        token::Client::new(&self.env, &self.token_id)
    }

    pub fn lock_token(&self) -> token::Client<'_> {
        token::Client::new(&self.env, &self.lock_asset)
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

    /// Open the lane on both shapes. The bare one has no peer because it never reads one.
    pub fn link_ethereum(&self) {
        self.adapter()
            .link_chain(&self.ethereum(), &self.axelar_ethereum(), &self.peer());
        self.bare().link_chain(
            &self.ethereum(),
            &self.axelar_ethereum(),
            &BytesN::from_array(&self.env, &[0u8; 20]),
        );
    }

    pub fn map_tokens(&self) {
        for adapter in [self.adapter(), self.bare()] {
            adapter.map_token(&self.token_id, &self.burn_id());
            adapter.map_token(&self.lock_asset, &self.lock_id());
        }
    }

    pub fn fund_user(&self, amount: i128) {
        self.its().faucet(&self.token_id, &self.user, &amount);
        self.its().faucet(&self.lock_asset, &self.user, &amount);
    }

    pub fn fund_adapter(&self, amount: i128) {
        self.its().faucet(&self.token_id, &self.adapter_id, &amount);
    }

    // -------------------------------------------------------------------------------------
    // Names and ids
    // -------------------------------------------------------------------------------------

    /// What Hyperion calls the chain.
    pub fn ethereum(&self) -> String {
        String::from_str(&self.env, "ethereum")
    }

    /// What Axelar calls it. The two differ only in case here, which is the real footgun in
    /// miniature: close enough to look right in a config file and not equal in any comparison.
    pub fn axelar_ethereum(&self) -> String {
        String::from_str(&self.env, "Ethereum")
    }

    pub fn peer(&self) -> BytesN<20> {
        BytesN::from_array(&self.env, &PEER)
    }

    pub fn burn_id(&self) -> BytesN<32> {
        BytesN::from_array(&self.env, &MB_TOKEN_ID)
    }

    pub fn lock_id(&self) -> BytesN<32> {
        BytesN::from_array(&self.env, &LU_TOKEN_ID)
    }

    /// Hyperion's contract on the far side, as the twenty bytes a note carries.
    pub fn peer_bytes(&self) -> Bytes {
        Bytes::from_array(&self.env, &PEER)
    }

    /// A left-padded EVM address ending in `last`, which is what a destination word looks like.
    pub fn evm(&self, last: u8) -> BytesN<32> {
        let mut raw = [0u8; 32];
        raw[12..].copy_from_slice(&[0x7C; 20]);
        raw[31] = last;
        BytesN::from_array(&self.env, &raw)
    }

    // -------------------------------------------------------------------------------------
    // Outbound requests
    // -------------------------------------------------------------------------------------

    /// The default request these tests send, ready to be bent out of shape.
    pub fn request(&self, amount: i128) -> OutboundRequest {
        OutboundRequest {
            token: self.token_id.clone(),
            amount,
            route: RouteKind::AxelarGmp,
            destination: Destination {
                chain: self.ethereum(),
                address: self.evm(0x01),
            },
            destination_decimals: EVM_DECIMALS,
            min_destination_amount: 0,
        }
    }

    // -------------------------------------------------------------------------------------
    // Notes
    // -------------------------------------------------------------------------------------

    /// A note naming the world's default recipient, which is a contract.
    pub fn note_to_recipient(&self, nonce: u64) -> Bytes {
        self.note(
            AddressKind::Contract,
            codec::contract_strkey(&self.env, &self.recipient_key),
            nonce,
        )
    }

    /// A note naming the world's classic account, trustline and all.
    pub fn note_to_classic(&self, nonce: u64) -> Bytes {
        self.note(
            AddressKind::Account,
            codec::account_strkey(&self.env, &self.classic_key),
            nonce,
        )
    }

    /// A note naming a muxed sub-account of that same classic account.
    pub fn note_to_muxed(&self, nonce: u64, muxed_id: u64) -> Bytes {
        self.note(
            AddressKind::MuxedAccount,
            codec::muxed_strkey(&self.env, &self.classic_key, muxed_id),
            nonce,
        )
    }

    pub fn note_to_contract(&self, addr: &Address, nonce: u64) -> Bytes {
        let key = codec::contract_key(&self.env, addr).unwrap();
        self.note(
            AddressKind::Contract,
            codec::contract_strkey(&self.env, &key),
            nonce,
        )
    }

    fn note(&self, kind: AddressKind, value: String, nonce: u64) -> Bytes {
        InboundNote::new(StrkeyDestination::new(kind, value), nonce)
            .encode(&self.env)
            .unwrap()
    }

    /// A note from a version of Hyperion this build has never met.
    pub fn note_from_the_future(&self, nonce: u64) -> Bytes {
        let mut raw = self.note_to_recipient(nonce);
        raw.set(0, NOTE_VERSION + 1);
        raw
    }

    /// A note whose tag says contract and whose strkey is an account.
    ///
    /// Built byte by byte rather than through `encode`, because that refuses to serialise a
    /// destination whose kind and strkey disagree in the first place, and the point here is to
    /// watch the contract on the receiving end refuse it too.
    pub fn note_that_lies_about_its_kind(&self, nonce: u64) -> Bytes {
        let strkey = codec::account_strkey(&self.env, &self.classic_key);
        let mut buf = [0u8; STRKEY_LEN_MUXED as usize];
        let slice = &mut buf[..STRKEY_LEN_PLAIN as usize];
        strkey.copy_into_slice(slice);

        let mut raw = Bytes::new(&self.env);
        raw.push_back(NOTE_VERSION);
        raw.push_back(AddressKind::Contract.tag());
        raw.push_back(STRKEY_LEN_PLAIN as u8);
        raw.append(&Bytes::from_slice(&self.env, slice));
        raw.append(&Bytes::from_array(&self.env, &nonce.to_be_bytes()));
        raw
    }
}

// ------------------------------------------------------------------------------------------
// The router's arithmetic, repeated here so a test can say what it expects
// ------------------------------------------------------------------------------------------

/// What the router keeps.
pub fn fee_of(amount: i128) -> i128 {
    amount * i128::from(FEE_BPS) / 10_000
}

/// What crosses, once the fee is off and the remainder has been floored for six decimals.
pub fn net_of(amount: i128) -> i128 {
    let net = amount - fee_of(amount);
    net - net % 10
}

/// What leaves the sender's balance, which is the fee plus the floored net and not the amount
/// they asked for. The difference is dust the far side could not have represented.
pub fn gross_of(amount: i128) -> i128 {
    fee_of(amount) + net_of(amount)
}
