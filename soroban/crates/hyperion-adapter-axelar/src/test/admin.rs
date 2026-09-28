//! Configuration, the admin role, and the two housekeeping calls anybody may make.

use hyperion_core::{HyperionError, RouteKind};
use soroban_sdk::{
    testutils::{MockAuth, MockAuthInvoke},
    Address, BytesN, IntoVal, String,
};

use super::setup::{World, FLOW_LIMIT, HUNDRED, STRANGE_TOKEN_ID};
use crate::rail::TokenManagerType;
use crate::{AxelarAdapter, AxelarAdapterClient};

/// A registered but deliberately unconfigured instance, for the questions that are only
/// interesting before `initialize` runs.
fn blank(w: &World) -> AxelarAdapterClient<'_> {
    let id = w.env.register(AxelarAdapter, ());
    AxelarAdapterClient::new(&w.env, &id)
}

// ------------------------------------------------------------------------------------------
// Coming up
// ------------------------------------------------------------------------------------------

#[test]
fn an_adapter_nobody_configured_yet_refuses_everything() {
    let w = World::unlinked();
    let fresh = blank(&w);

    // A half finished deploy script is a normal Tuesday. What matters is that the thing it
    // leaves behind says no to everything rather than falling back on a default.
    assert_eq!(
        fresh.try_get_config(),
        Err(Ok(HyperionError::NotInitialized))
    );
    assert_eq!(
        fresh.try_keep_alive(&w.ethereum()),
        Err(Ok(HyperionError::NotInitialized))
    );
    assert_eq!(
        fresh.try_sweep(&w.relayer, &w.token_id),
        Err(Ok(HyperionError::NotInitialized))
    );
    assert_eq!(
        fresh.try_link_chain(&w.ethereum(), &w.axelar_ethereum(), &w.peer()),
        Err(Ok(HyperionError::NotInitialized))
    );
    assert_eq!(
        fresh.try_map_token(&w.token_id, &w.burn_id()),
        Err(Ok(HyperionError::NotInitialized))
    );
    assert_eq!(
        fresh.try_manager_type_for(&w.token_id),
        Err(Ok(HyperionError::NotInitialized))
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
        Err(Ok(HyperionError::NotInitialized))
    );
}

#[test]
fn the_wiring_is_only_ever_set_once() {
    let w = World::unlinked();
    // Re-running a deploy script should be boring. Letting the second run quietly repoint the
    // router or ITS address would make it the most dangerous command in the repository.
    assert_eq!(
        w.adapter()
            .try_initialize(&w.relayer, &w.router_id, &w.its_id, &RouteKind::AxelarGmp),
        Err(Ok(HyperionError::AlreadyInitialized))
    );
    assert_eq!(w.adapter().get_config().admin, w.admin);
}

#[test]
fn an_instance_can_only_ever_be_one_of_the_two_axelar_shapes() {
    let w = World::unlinked();
    // This code knows how to talk to ITS and nothing else. Handing it a CCTP route would
    // produce an adapter the router happily registers and that nothing can ever use, which is
    // the sort of mistake that sits quiet until the first transfer.
    for route in [RouteKind::Cctp, RouteKind::Allbridge] {
        assert_eq!(
            blank(&w).try_initialize(&w.admin, &w.router_id, &w.its_id, &route),
            Err(Ok(HyperionError::UnsupportedRoute))
        );
    }
    // And the two it does speak both take.
    for route in [RouteKind::AxelarGmp, RouteKind::AxelarIts] {
        let fresh = blank(&w);
        fresh.initialize(&w.admin, &w.router_id, &w.its_id, &route);
        assert_eq!(fresh.get_config().route, route);
    }
}

#[test]
fn a_freshly_wired_adapter_knows_no_chains_and_no_assets() {
    let w = World::unlinked();
    let a = w.adapter();

    // Empty rather than permissive. Every lane and every asset is a decision somebody has to
    // make on purpose.
    assert_eq!(
        a.try_get_link(&w.ethereum()),
        Err(Ok(HyperionError::UnknownChain))
    );
    assert_eq!(
        a.try_chain_for(&w.axelar_ethereum()),
        Err(Ok(HyperionError::UnknownChain))
    );
    assert_eq!(
        a.try_token_id_for(&w.token_id),
        Err(Ok(HyperionError::TokenNotMapped))
    );
    assert_eq!(
        a.try_token_for(&w.burn_id()),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

// ------------------------------------------------------------------------------------------
// Lanes
// ------------------------------------------------------------------------------------------

#[test]
fn a_lane_reads_back_under_both_of_its_names() {
    let w = World::new();
    let link = w.adapter().get_link(&w.ethereum());

    // Hyperion calls it "ethereum", Axelar calls it "Ethereum", and the whole point of storing
    // both is that neither side has to guess at the other's capitalisation.
    assert_eq!(link.chain, w.ethereum());
    assert_eq!(link.axelar_chain, w.axelar_ethereum());
    assert_eq!(link.peer, w.peer());
    assert_eq!(w.adapter().chain_for(&w.axelar_ethereum()), w.ethereum());
}

#[test]
fn a_lane_needs_a_name_on_both_sides() {
    let w = World::unlinked();
    let empty = String::from_str(&w.env, "");

    assert_eq!(
        w.adapter()
            .try_link_chain(&empty, &w.axelar_ethereum(), &w.peer()),
        Err(Ok(HyperionError::UnknownChain))
    );
    assert_eq!(
        w.adapter().try_link_chain(&w.ethereum(), &empty, &w.peer()),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn a_chain_axelar_will_not_route_to_cannot_be_linked() {
    let w = World::unlinked();
    // The lane would save fine and then fail on its first transfer, somewhere deep inside a
    // contract nobody in this repository wrote. Finding out while an admin is looking at a
    // terminal is worth one extra cross-contract read.
    assert_eq!(
        w.adapter().try_link_chain(
            &String::from_str(&w.env, "polygon"),
            &String::from_str(&w.env, "Polygon"),
            &w.peer(),
        ),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn the_shape_that_carries_a_note_insists_on_knowing_who_may_send_one() {
    let w = World::unlinked();
    let nobody = BytesN::from_array(&w.env, &[0u8; 20]);

    // A GMP lane with no peer is a lane that will accept a payload from any contract on the far
    // side, which is the same as having no inbound authorisation at all.
    assert_eq!(
        w.adapter()
            .try_link_chain(&w.ethereum(), &w.axelar_ethereum(), &nobody),
        Err(Ok(HyperionError::ZeroAddressKey))
    );

    // The bare shape never reads a payload and never gets called back, so there is nobody to
    // check and nothing to insist on.
    w.bare()
        .link_chain(&w.ethereum(), &w.axelar_ethereum(), &nobody);
    assert_eq!(w.bare().get_link(&w.ethereum()).peer, nobody);
}

#[test]
fn relinking_a_lane_replaces_it_rather_than_stacking_a_second_one() {
    let w = World::new();
    let moved = BytesN::from_array(&w.env, &[0x5Au8; 20]);
    // Contracts on the far side get redeployed. The lane has to be able to follow them without
    // leaving the old address able to send payloads.
    w.adapter()
        .link_chain(&w.ethereum(), &w.axelar_ethereum(), &moved);
    assert_eq!(w.adapter().get_link(&w.ethereum()).peer, moved);
    assert_eq!(w.adapter().chain_for(&w.axelar_ethereum()), w.ethereum());
}

// ------------------------------------------------------------------------------------------
// Assets
// ------------------------------------------------------------------------------------------

#[test]
fn a_mapping_reads_back_both_ways() {
    let w = World::new();
    assert_eq!(w.adapter().token_id_for(&w.token_id), w.burn_id());
    assert_eq!(w.adapter().token_for(&w.burn_id()), w.token_id);
    assert_eq!(w.adapter().token_id_for(&w.lock_asset), w.lock_id());
    assert_eq!(w.adapter().token_for(&w.lock_id()), w.lock_asset);
}

#[test]
fn a_token_id_axelar_has_never_heard_of_cannot_be_mapped() {
    let w = World::unlinked();
    // Typing a token id by hand is how this gets configured, and a typo in thirty two bytes is
    // not something anybody spots by reading it back.
    assert_eq!(
        w.adapter()
            .try_map_token(&w.token_id, &BytesN::from_array(&w.env, &STRANGE_TOKEN_ID)),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

#[test]
fn a_token_id_that_names_a_different_asset_cannot_be_mapped() {
    let w = World::unlinked();
    // Both halves exist and both are real. The pairing is the mistake, and an adapter that took
    // it would send one asset out and expect the other back.
    assert_eq!(
        w.adapter().try_map_token(&w.lock_asset, &w.burn_id()),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

#[test]
fn the_views_answer_with_axelars_own_numbers_rather_than_a_cached_guess() {
    let w = World::new();

    // How ITS takes the funds is the one thing the outbound path authorises, so it is read live
    // on every dispatch. These views are the same read, exposed so a route planner can explain
    // a transfer before anybody signs it.
    assert_eq!(
        w.adapter().manager_type_for(&w.token_id),
        TokenManagerType::MintBurn
    );
    assert_eq!(
        w.adapter().manager_type_for(&w.lock_asset),
        TokenManagerType::LockUnlock
    );

    // Axelar's operators can set a flow limit of their own, and the tighter of theirs and
    // Hyperion's is what actually binds. Better to surface it than to let a user discover it
    // from a refusal.
    assert_eq!(w.adapter().rail_flow_limit(&w.token_id), None);
    w.its().set_flow_limit(&w.burn_id(), &FLOW_LIMIT);
    assert_eq!(w.adapter().rail_flow_limit(&w.token_id), Some(FLOW_LIMIT));
}

#[test]
fn the_views_refuse_an_asset_nobody_mapped() {
    let w = World::unlinked();
    assert_eq!(
        w.adapter().try_manager_type_for(&w.token_id),
        Err(Ok(HyperionError::TokenNotMapped))
    );
    assert_eq!(
        w.adapter().try_rail_flow_limit(&w.token_id),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

// ------------------------------------------------------------------------------------------
// Who is allowed to change things
// ------------------------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Unauthorized")]
fn a_stranger_cannot_open_a_lane() {
    let w = World::unlinked();
    w.env.set_auths(&[]);
    w.adapter()
        .link_chain(&w.ethereum(), &w.axelar_ethereum(), &w.peer());
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn a_stranger_cannot_map_an_asset() {
    let w = World::unlinked();
    w.env.set_auths(&[]);
    w.adapter().map_token(&w.token_id, &w.burn_id());
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
    // The upgrade path is the one that matters most. Everything else on this list can be
    // undone by an admin; this one replaces the admin.
    w.adapter()
        .upgrade(&BytesN::from_array(&w.env, &[0x11u8; 32]));
}

#[test]
fn handing_the_role_over_moves_it_completely() {
    let w = World::new();
    w.adapter().set_admin(&w.guardian);
    assert_eq!(w.adapter().get_config().admin, w.guardian);

    // The new admin can work straight away.
    w.adapter()
        .link_chain(&w.ethereum(), &w.axelar_ethereum(), &w.peer());
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

// ------------------------------------------------------------------------------------------
// Housekeeping anybody may do
// ------------------------------------------------------------------------------------------

#[test]
fn keeping_a_quiet_lane_alive_is_open_to_anybody_and_changes_nothing() {
    let w = World::new();
    // No caller argument and no signature, because the call has no effect a stranger could
    // benefit from. Storage on Stellar is archived when nobody touches it, and a lane that goes
    // three months without a transfer is exactly the lane somebody needs on the fourth.
    w.env.set_auths(&[]);
    w.adapter().keep_alive(&w.ethereum());

    let link = w.adapter().get_link(&w.ethereum());
    assert_eq!(link.axelar_chain, w.axelar_ethereum());
    assert_eq!(link.peer, w.peer());
}

#[test]
fn keeping_a_lane_that_does_not_exist_alive_is_a_quiet_no_op() {
    let w = World::new();
    // Keeper jobs are driven off a config file that will drift from the contract eventually.
    // A stale entry should cost a fee and nothing else, not take the whole batch down.
    w.adapter().keep_alive(&String::from_str(&w.env, "polygon"));
    assert_eq!(
        w.adapter()
            .try_get_link(&String::from_str(&w.env, "polygon")),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn dust_left_behind_can_be_swept_to_the_treasury_by_anybody() {
    let w = World::new();
    w.fund_adapter(HUNDRED);

    // An admin only rescue would be a standing power to move tokens out of a contract users
    // send tokens to. The destination is read live from the router instead, so the worst a
    // caller can do here is pay a fee to tidy up after somebody.
    let swept = w.adapter().sweep(&w.relayer, &w.token_id);
    assert_eq!(swept, HUNDRED);
    assert_eq!(w.token().balance(&w.treasury), HUNDRED);
    assert_eq!(w.token().balance(&w.adapter_id), 0);
    assert_eq!(w.token().balance(&w.relayer), 0);
}

#[test]
fn sweeping_nothing_is_not_an_error() {
    let w = World::new();
    // The usual case. A keeper that treated an empty balance as a failure would fill its logs
    // with alarms about a contract behaving exactly as intended.
    assert_eq!(w.adapter().sweep(&w.relayer, &w.token_id), 0);
    assert_eq!(w.token().balance(&w.treasury), 0);
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
fn the_admin_is_not_special_on_the_permissionless_calls() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Written down because it is easy to lose by accident: these two are the calls a keeper
    // makes, and a keeper is not the admin.
    let keeper: Address = w.relayer.clone();
    w.adapter().keep_alive(&w.ethereum());
    assert_eq!(w.adapter().sweep(&keeper, &w.token_id), HUNDRED);
}
