//! Wiring, roles, the relay float, and the housekeeping anybody is allowed to do.

use hyperion_core::{HyperionError, RouteKind};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, IntoVal, String,
};

use super::setup::{
    World, BASE_CHAIN_ID, GAS_FLOAT, HUNDRED, NOWHERE_CHAIN_ID, POOL_FEE_BPS, RELAY_COST,
};
use crate::{AllbridgeAdapter, AllbridgeAdapterClient};

/// A registered but deliberately unconfigured instance, for the questions that are only
/// interesting before `initialize` runs.
fn blank(w: &World) -> AllbridgeAdapterClient<'_> {
    let id = w.env.register(AllbridgeAdapter, ());
    AllbridgeAdapterClient::new(&w.env, &id)
}

// ------------------------------------------------------------------------------------------
// Before there is anything to talk to
// ------------------------------------------------------------------------------------------

#[test]
fn an_adapter_nobody_configured_yet_refuses_everything() {
    let w = World::new();
    let fresh = blank(&w);
    let nothing = HyperionError::NotInitialized;

    // A deploy script that stops halfway leaves this behind. It has to be inert, not hopeful.
    assert_eq!(fresh.try_get_config(), Err(Ok(nothing)));
    assert_eq!(fresh.try_route(), Err(Ok(nothing)));
    assert_eq!(fresh.try_gas_balance(), Err(Ok(nothing)));
    assert_eq!(fresh.try_quote_gas(&w.ethereum()), Err(Ok(nothing)));
    assert_eq!(fresh.try_keep_alive(&w.ethereum()), Err(Ok(nothing)));
    assert_eq!(fresh.try_pool_for(&w.token_id), Err(Ok(nothing)));
    assert_eq!(fresh.try_rail_open(), Err(Ok(nothing)));
    assert_eq!(fresh.try_sweep(&w.relayer, &w.token_id), Err(Ok(nothing)));
    assert_eq!(fresh.try_fund_gas(&w.relayer, &HUNDRED), Err(Ok(nothing)));
    assert_eq!(fresh.try_withdraw_gas(&w.admin, &HUNDRED), Err(Ok(nothing)));
    assert_eq!(
        fresh.try_link_chain(&w.ethereum(), &BASE_CHAIN_ID),
        Err(Ok(nothing))
    );
    assert_eq!(
        fresh.try_link_asset(&w.token_id, &w.ethereum(), &w.receive_token()),
        Err(Ok(nothing))
    );
    assert_eq!(
        fresh.try_dispatch(
            &w.router_id,
            &w.token_id,
            &HUNDRED,
            &w.ethereum(),
            &w.evm(0x01),
            &1,
        ),
        Err(Ok(nothing))
    );
}

#[test]
fn the_wiring_is_only_ever_set_once() {
    let w = World::new();
    // Re-running a deploy script is a normal accident. Letting it repoint the router would hand
    // a stranger the one address allowed to call `dispatch`.
    assert_eq!(
        w.adapter()
            .try_initialize(&w.relayer, &w.relayer, &w.bridge_id, &w.native_id,),
        Err(Ok(HyperionError::AlreadyInitialized))
    );
    let cfg = w.adapter().get_config();
    assert_eq!(cfg.admin, w.admin);
    assert_eq!(cfg.router, w.router_id);
    assert_eq!(cfg.bridge, w.bridge_id);
    assert_eq!(cfg.native, w.native_id);
}

#[test]
fn the_only_route_this_shape_ever_answers_to_is_the_pooled_one() {
    let w = World::new();
    // One WASM, one rail. Allbridge has no inbound leg here, so there is nothing to select
    // between and nothing to get wrong at deploy time.
    assert_eq!(w.adapter().route(), RouteKind::Allbridge);
}

#[test]
fn a_freshly_wired_adapter_knows_no_chains_and_no_assets() {
    let w = World::unlinked();
    assert_eq!(
        w.adapter().try_get_lane(&w.ethereum()),
        Err(Ok(HyperionError::UnknownChain))
    );
    assert_eq!(
        w.adapter().try_chain_for(&BASE_CHAIN_ID),
        Err(Ok(HyperionError::UnknownChain))
    );
    assert_eq!(
        w.adapter().try_get_asset(&w.token_id, &w.ethereum()),
        Err(Ok(HyperionError::UnknownChain))
    );
    assert_eq!(w.adapter().gas_balance(), 0);
}

// ------------------------------------------------------------------------------------------
// Lanes
// ------------------------------------------------------------------------------------------

#[test]
fn a_lane_reads_back_under_both_of_its_names() {
    let w = World::new();
    let lane = w.adapter().get_lane(&w.ethereum());
    assert_eq!(lane.chain, w.ethereum());
    assert_eq!(lane.allbridge_chain_id, super::setup::ETH_CHAIN_ID);
    // Both directions, because the indexer reads a chain id off an event and needs the name back.
    assert_eq!(
        w.adapter().chain_for(&super::setup::ETH_CHAIN_ID),
        w.ethereum()
    );
}

#[test]
fn a_lane_needs_a_name() {
    let w = World::unlinked();
    assert_eq!(
        w.adapter()
            .try_link_chain(&String::from_str(&w.env, ""), &BASE_CHAIN_ID),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn a_lane_pointing_back_at_stellar_is_refused() {
    let w = World::unlinked();
    // Allbridge refuses a destination equal to its own chain id, so a lane pointing home is a
    // lane whose first transfer reverts from inside somebody else's contract.
    assert_eq!(
        w.adapter()
            .try_link_chain(&w.base(), &crate::rail::STELLAR_CHAIN_ID),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn a_chain_allbridge_has_no_bridge_for_cannot_be_linked() {
    let w = World::unlinked();
    // Checked against Allbridge rather than taken on trust. One cross contract read while an
    // admin is watching beats a transfer that reverts two contracts away next week.
    assert_eq!(
        w.adapter().try_link_chain(&w.base(), &NOWHERE_CHAIN_ID),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn relinking_a_lane_moves_it_rather_than_stacking_a_second_one() {
    let w = World::new();
    // Allbridge renumbers nothing, but Hyperion's own name for a chain can be repointed while
    // a route is being retired. The forward direction is the one `dispatch` reads, so that is
    // the one that has to follow.
    w.adapter().link_chain(&w.ethereum(), &BASE_CHAIN_ID);
    assert_eq!(
        w.adapter().get_lane(&w.ethereum()).allbridge_chain_id,
        BASE_CHAIN_ID
    );
    assert_eq!(w.adapter().chain_for(&BASE_CHAIN_ID), w.ethereum());
}

// ------------------------------------------------------------------------------------------
// Assets
// ------------------------------------------------------------------------------------------

#[test]
fn a_mapping_reads_back_with_the_lane_it_was_made_on() {
    let w = World::new();
    let link = w.adapter().get_asset(&w.token_id, &w.ethereum());
    assert_eq!(link.token, w.token_id);
    assert_eq!(link.allbridge_chain_id, super::setup::ETH_CHAIN_ID);
    assert_eq!(link.receive_token, w.receive_token());
}

#[test]
fn an_asset_cannot_be_mapped_onto_a_lane_nobody_opened() {
    let w = World::unlinked();
    assert_eq!(
        w.adapter()
            .try_link_asset(&w.token_id, &w.ethereum(), &w.receive_token()),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn an_asset_allbridge_holds_no_pool_for_cannot_be_mapped() {
    let w = World::new();
    // A real asset with no pool behind it. Mapping it would produce a transfer that gets as far
    // as asking Allbridge where to sell and then reverts holding the money.
    assert_eq!(
        w.adapter()
            .try_link_asset(&w.orphan_id, &w.ethereum(), &w.receive_token()),
        Err(Ok(HyperionError::TokenNotMapped))
    );
    assert_eq!(
        w.adapter().try_pool_for(&w.orphan_id),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

#[test]
fn a_receive_token_the_far_side_does_not_accept_cannot_be_mapped() {
    let w = World::new();
    // Both halves are checked. Getting this one wrong produces a transfer that sells into the
    // pool here and then has nothing to be paid out as over there.
    assert_eq!(
        w.adapter()
            .try_link_asset(&w.token_id, &w.ethereum(), &w.unknown_receive_token()),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

#[test]
fn a_mapping_is_per_lane_rather_than_per_asset() {
    let w = World::new();
    w.adapter().link_chain(&w.base(), &BASE_CHAIN_ID);
    // Mapped on Ethereum and nowhere else. The same asset can become a different token on every
    // chain, so a mapping that leaked across lanes would pay out the wrong thing.
    assert_eq!(
        w.adapter().try_get_asset(&w.token_id, &w.base()),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

// ------------------------------------------------------------------------------------------
// Who may do what
// ------------------------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Unauthorized")]
fn a_stranger_cannot_open_a_lane() {
    let w = World::new();
    w.env.set_auths(&[]);
    w.adapter().link_chain(&w.base(), &BASE_CHAIN_ID);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn a_stranger_cannot_map_an_asset() {
    let w = World::new();
    w.env.set_auths(&[]);
    w.adapter()
        .link_asset(&w.token_id, &w.ethereum(), &w.receive_token());
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn a_stranger_cannot_hand_themselves_the_admin_role() {
    let w = World::new();
    w.env.set_auths(&[]);
    w.adapter().set_admin(&w.relayer);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn a_stranger_cannot_replace_the_code() {
    let w = World::new();
    w.env.set_auths(&[]);
    w.adapter()
        .upgrade(&BytesN::from_array(&w.env, &[0x11u8; 32]));
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn a_stranger_cannot_drain_the_float() {
    let w = World::new();
    // The one call on this contract that sends value somewhere of its caller's choosing, which
    // is why it is the only one gated this tightly.
    w.env.set_auths(&[]);
    w.adapter().withdraw_gas(&w.relayer, &GAS_FLOAT);
}

#[test]
fn handing_the_role_over_moves_it_completely() {
    let w = World::new();
    w.adapter().set_admin(&w.guardian);
    assert_eq!(w.adapter().get_config().admin, w.guardian);
    // And the new admin can actually use it, which is the half that a config write alone would
    // not prove.
    w.adapter().link_chain(&w.base(), &BASE_CHAIN_ID);
    assert_eq!(
        w.adapter().get_lane(&w.base()).allbridge_chain_id,
        BASE_CHAIN_ID
    );
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn the_previous_admin_keeps_nothing() {
    let w = World::new();
    w.adapter().set_admin(&w.guardian);
    // Hand over the old admin's signature and nobody else's. A role that leaves a residue
    // behind is not a handover, it is a second key.
    w.env.mock_auths(&[MockAuth {
        address: &w.admin,
        invoke: &MockAuthInvoke {
            contract: &w.adapter_id,
            fn_name: "set_admin",
            args: (w.admin.clone(),).into_val(&w.env),
            sub_invokes: &[],
        },
    }]);
    w.adapter().set_admin(&w.admin);
}

#[test]
fn the_admin_is_not_special_on_the_calls_anybody_may_make() {
    let w = World::new();
    // Funding, sweeping and keeping a lane alive are open to everybody, and the admin gets no
    // shortcut on any of them. A stranger's call and the admin's call are the same call.
    let by_stranger = w.fund_gas(HUNDRED);
    assert_eq!(by_stranger, GAS_FLOAT + HUNDRED);
    w.mint_native(&w.admin, HUNDRED);
    assert_eq!(
        w.adapter().fund_gas(&w.admin, &HUNDRED),
        GAS_FLOAT + HUNDRED * 2
    );
    assert_eq!(w.adapter().sweep(&w.admin, &w.token_id), 0);
    w.adapter().keep_alive(&w.ethereum());
}

// ------------------------------------------------------------------------------------------
// The relay float
// ------------------------------------------------------------------------------------------

#[test]
fn anybody_can_top_the_float_up_and_the_balance_is_a_view() {
    let w = World::unlinked();
    assert_eq!(w.adapter().gas_balance(), 0);
    // Deliberately permissionless. There is nothing to gain by filling it and quite a lot to
    // lose by having only one address able to, so on the day the keeper is down, anybody can.
    assert_eq!(w.fund_gas(GAS_FLOAT), GAS_FLOAT);
    assert_eq!(w.adapter().gas_balance(), GAS_FLOAT);
    assert_eq!(w.native().balance(&w.adapter_id), GAS_FLOAT);
}

#[test]
fn funding_the_float_with_nothing_is_refused() {
    let w = World::new();
    for amount in [0i128, -1, -GAS_FLOAT] {
        assert_eq!(
            w.adapter().try_fund_gas(&w.relayer, &amount),
            Err(Ok(HyperionError::InvalidAmount))
        );
    }
}

#[test]
fn the_float_can_be_taken_back_out_by_the_admin_and_only_as_far_as_it_goes() {
    let w = World::new();
    let left = w.adapter().withdraw_gas(&w.treasury, &(GAS_FLOAT / 2));
    assert_eq!(left, GAS_FLOAT / 2);
    assert_eq!(w.native().balance(&w.treasury), GAS_FLOAT / 2);

    // It exists so a retired adapter's float is not stranded, not as a way to invent one.
    assert_eq!(
        w.adapter().try_withdraw_gas(&w.treasury, &GAS_FLOAT),
        Err(Ok(HyperionError::GasFloatTooLow))
    );
    for amount in [0i128, -1] {
        assert_eq!(
            w.adapter().try_withdraw_gas(&w.treasury, &amount),
            Err(Ok(HyperionError::InvalidAmount))
        );
    }
    assert_eq!(w.adapter().gas_balance(), GAS_FLOAT / 2);
}

#[test]
fn the_float_survives_being_spent_down_to_nothing_and_refilled() {
    let w = World::unlinked();
    w.link_ethereum();
    w.map_token();
    w.fund_user(HUNDRED * 10);
    w.fund_gas(RELAY_COST as i128);

    w.router().bridge_out(&w.user, &w.request(HUNDRED));
    assert_eq!(w.adapter().gas_balance(), 0);
    // Empty is not broken. The next transfer waits for a top up and then goes.
    assert_eq!(
        w.adapter().try_dispatch(
            &w.router_id,
            &w.token_id,
            &HUNDRED,
            &w.ethereum(),
            &w.evm(0x01),
            &99,
        ),
        Err(Ok(HyperionError::GasFloatTooLow))
    );
    w.fund_gas(RELAY_COST as i128);
    w.router().bridge_out(&w.user, &w.request(HUNDRED));
    assert_eq!(w.bridge().sent_count(), 2);
}

// ------------------------------------------------------------------------------------------
// Sweeping
// ------------------------------------------------------------------------------------------

#[test]
fn dust_left_behind_can_be_swept_to_the_treasury_by_anybody() {
    let w = World::new();
    w.fund_adapter(HUNDRED);

    // Pooled routes round, and people send tokens to contracts by hand. An admin only rescue
    // would be a standing power to move tokens out of a contract users send tokens to, so the
    // destination is read live from the router and the call is open to everybody.
    let swept = w.adapter().sweep(&w.relayer, &w.token_id);
    assert_eq!(swept, HUNDRED);
    assert_eq!(w.token().balance(&w.treasury), HUNDRED);
    assert_eq!(w.token().balance(&w.adapter_id), 0);
    assert_eq!(w.token().balance(&w.relayer), 0);
}

#[test]
fn sweeping_nothing_is_not_an_error() {
    let w = World::new();
    // The keeper sweeps on a schedule and most of the time there is nothing there. Failing on an
    // empty balance would mean a failed job every few minutes.
    assert_eq!(w.adapter().sweep(&w.relayer, &w.token_id), 0);
    assert_eq!(w.token().balance(&w.treasury), 0);
}

#[test]
fn the_float_itself_cannot_be_swept() {
    let w = World::new();
    // Sweeping is for balances nobody meant to leave here. The float is the opposite of that,
    // and a permissionless call that emptied it would stop every transfer on this route.
    assert_eq!(
        w.adapter().try_sweep(&w.relayer, &w.native_id),
        Err(Ok(HyperionError::ProtectedAsset))
    );
    assert_eq!(w.adapter().gas_balance(), GAS_FLOAT);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn a_sweep_still_needs_its_caller_to_sign() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Permissionless means anybody may ask, not that anybody may be volunteered. The caller
    // pays the fee, so the caller signs.
    w.env.set_auths(&[]);
    w.adapter().sweep(&w.relayer, &w.token_id);
}

#[test]
fn a_sweep_sends_dust_wherever_the_router_currently_says() {
    let w = World::new();
    let moved = Address::generate(&w.env);
    // Read live rather than copied at deploy time, so moving the treasury takes one action on
    // the router instead of one per adapter.
    w.run_action(hyperion_router::AdminAction::SetTreasury(moved.clone()));
    w.fund_adapter(HUNDRED);
    w.adapter().sweep(&w.relayer, &w.token_id);
    assert_eq!(w.token().balance(&moved), HUNDRED);
    assert_eq!(w.token().balance(&w.treasury), 0);
}

// ------------------------------------------------------------------------------------------
// Keeping the lights on
// ------------------------------------------------------------------------------------------

#[test]
fn keeping_a_quiet_lane_alive_is_open_to_anybody_and_changes_nothing() {
    let w = World::new();
    let before = w.adapter().get_lane(&w.ethereum());
    w.adapter().keep_alive(&w.ethereum());
    assert_eq!(w.adapter().get_lane(&w.ethereum()), before);
    assert_eq!(
        w.adapter().get_asset(&w.token_id, &w.ethereum()).token,
        w.token_id
    );
}

#[test]
fn keeping_a_lane_that_does_not_exist_alive_is_a_quiet_no_op() {
    let w = World::new();
    // The keeper walks a list it read a while ago. A lane that has since been retired should
    // cost it a no op, not a failed job.
    w.adapter().keep_alive(&w.base());
}

// ------------------------------------------------------------------------------------------
// Views that read the rail rather than a cached guess
// ------------------------------------------------------------------------------------------

#[test]
fn the_views_answer_with_allbridges_own_numbers() {
    let w = World::new();
    assert_eq!(w.adapter().pool_for(&w.token_id), w.pool_id);
    assert!(w.adapter().rail_open());
    assert_eq!(w.adapter().quote_gas(&w.ethereum()), RELAY_COST);

    // Move the pool underneath the adapter and ask again. Nothing here is remembered, so
    // nothing here can go stale.
    let moved = w.env.register(super::allbridge::MockPool, ());
    super::allbridge::MockPoolClient::new(&w.env, &moved).initialize(
        &w.bridge_id,
        &w.token_id,
        &POOL_FEE_BPS,
    );
    w.bridge().add_pool(&moved, &w.token_key(&w.token_id));
    assert_eq!(w.adapter().pool_for(&w.token_id), moved);
}
