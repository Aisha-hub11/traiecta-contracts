//! The leg where USDC leaves Stellar and Circle burns it.

use hyperion_core::{HyperionError, RouteKind};
use hyperion_router::{Destination, OutboundRequest};
use soroban_sdk::{testutils::Address as _, Address, BytesN, String};

use super::setup::{net_of, World, ETHEREUM_DOMAIN, EVM_DECIMALS, HUNDRED};

/// A left-padded EVM word, which is the only shape the router will let through.
fn evm(world: &World, last: u8) -> BytesN<32> {
    let mut raw = [0u8; 32];
    for (i, slot) in raw.iter_mut().enumerate().skip(12) {
        *slot = if i == 31 { last } else { 0x7C };
    }
    BytesN::from_array(&world.env, &raw)
}

fn request(world: &World, amount: i128) -> OutboundRequest {
    OutboundRequest {
        token: world.token_id.clone(),
        amount,
        route: RouteKind::Cctp,
        destination: Destination {
            chain: world.ethereum(),
            address: evm(world, 0x11),
        },
        destination_decimals: EVM_DECIMALS,
        min_destination_amount: 0,
    }
}

#[test]
fn a_transfer_walks_from_the_router_through_the_adapter_into_circles_burn() {
    let w = World::new();

    w.router().bridge_out(&w.user, &request(&w, HUNDRED));

    let net = net_of(HUNDRED);
    let burn = w.messenger().last_burn();
    // Circle was called by the adapter, not by the router and not by the user. The adapter is the
    // only party that ever holds the funds between the fee split and the burn.
    assert_eq!(burn.caller, w.adapter_id);
    assert_eq!(burn.amount, net);
    assert_eq!(burn.burned, net);
    assert_eq!(burn.destination_domain, ETHEREUM_DOMAIN);
    assert_eq!(burn.mint_recipient, evm(&w, 0x11));
    assert_eq!(burn.burn_token, w.token_id);
    // A finalized transfer is free, so there is nothing to budget for.
    assert_eq!(burn.max_fee, 0);
    assert_eq!(burn.min_finality_threshold, 2000);
    // The EVM side mints straight to the recipient, so there is nothing to say in a hook.
    assert!(!burn.had_hook);

    // Nothing is left anywhere it should not be.
    assert_eq!(w.token().balance(&w.adapter_id), 0);
    assert_eq!(w.token().balance(&w.router_id), 0);
    assert_eq!(w.token().balance(&w.treasury), HUNDRED - net);
}

#[test]
fn the_allowance_circle_needed_is_closed_before_the_call_returns() {
    let w = World::new();
    w.router().bridge_out(&w.user, &request(&w, HUNDRED));
    // An allowance outliving the transaction that needed it is a standing claim on the adapter's
    // balance. Circle has no reason to keep one and this contract has no reason to grant one.
    assert_eq!(w.token().allowance(&w.adapter_id, &w.messenger_id), 0);
}

#[test]
fn the_destination_caller_is_whatever_the_admin_wrote_down_for_that_lane() {
    let w = World::new();
    // Naming a caller on the far side means only Hyperion's own EVM relayer can broadcast the
    // mint, which is how a lane gets closed to opportunistic third parties.
    let only_us = BytesN::from_array(&w.env, &[0x9Du8; 32]);
    w.adapter()
        .link_domain(&w.ethereum(), &ETHEREUM_DOMAIN, &only_us);

    w.router().bridge_out(&w.user, &request(&w, HUNDRED));
    assert_eq!(w.messenger().last_burn().destination_caller, only_us);
}

#[test]
fn an_unnamed_destination_caller_means_anybody_may_broadcast_it() {
    let w = World::new();
    w.router().bridge_out(&w.user, &request(&w, HUNDRED));
    assert_eq!(
        w.messenger().last_burn().destination_caller,
        BytesN::from_array(&w.env, &[0u8; 32])
    );
}

#[test]
fn nobody_but_the_router_can_put_anything_on_this_rail() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    let stranger = Address::generate(&w.env);

    // The adapter is holding real money and this call would burn it. Being able to reach the
    // function is not the same as being allowed to use it.
    assert_eq!(
        w.adapter().try_dispatch(
            &stranger,
            &w.token_id,
            &HUNDRED,
            &w.ethereum(),
            &evm(&w, 0x11),
            &1
        ),
        Err(Ok(HyperionError::Unauthorized))
    );
    assert_eq!(w.messenger().burn_count(), 0);
    assert_eq!(w.token().balance(&w.adapter_id), HUNDRED);
}

#[test]
fn a_chain_the_admin_never_linked_is_refused_before_any_approval_is_granted() {
    let w = World::new();
    w.fund_adapter(HUNDRED);

    assert_eq!(
        w.adapter().try_dispatch(
            &w.router_id,
            &w.token_id,
            &HUNDRED,
            &String::from_str(&w.env, "arbitrum"),
            &evm(&w, 0x11),
            &1
        ),
        Err(Ok(HyperionError::UnknownChain))
    );
    assert_eq!(w.token().allowance(&w.adapter_id, &w.messenger_id), 0);
}

#[test]
fn an_all_zero_mint_recipient_is_refused_here_rather_than_two_contracts_away() {
    let w = World::new();
    w.fund_adapter(HUNDRED);

    // Circle refuses this too, with its own error. Catching it early is the difference between a
    // reason the app can show a user and a panic from inside somebody else's contract.
    assert_eq!(
        w.adapter().try_dispatch(
            &w.router_id,
            &w.token_id,
            &HUNDRED,
            &w.ethereum(),
            &BytesN::from_array(&w.env, &[0u8; 32]),
            &1
        ),
        Err(Ok(HyperionError::InvalidDestination))
    );
}

#[test]
fn zero_and_negative_amounts_never_reach_circle() {
    let w = World::new();
    for amount in [0i128, -1, -HUNDRED] {
        assert_eq!(
            w.adapter().try_dispatch(
                &w.router_id,
                &w.token_id,
                &amount,
                &w.ethereum(),
                &evm(&w, 0x11),
                &1
            ),
            Err(Ok(HyperionError::InvalidAmount))
        );
    }
    assert_eq!(w.messenger().burn_count(), 0);
}

#[test]
fn a_fast_transfer_budgets_a_fee_and_a_finalised_one_does_not() {
    let w = World::new();
    // Five basis points, and a confidence level Circle reaches in seconds rather than minutes.
    w.adapter().set_fee_policy(&5, &1000);

    w.router().bridge_out(&w.user, &request(&w, HUNDRED));
    let net = net_of(HUNDRED);
    let burn = w.messenger().last_burn();

    assert_eq!(burn.min_finality_threshold, 1000);
    assert_eq!(burn.max_fee, net * 5 / 10_000);
    // The budget is a ceiling, not a payment. Circle takes what it takes and the rest stays.
    assert!(burn.max_fee < burn.amount);
    assert_eq!(w.adapter().quote_max_fee(&net), burn.max_fee);
}

#[test]
fn the_fee_budget_is_quoted_with_the_same_arithmetic_it_is_charged_with() {
    let w = World::new();
    w.adapter().set_fee_policy(&7, &1000);
    // A quote the app can show and a budget the contract sends have to be the same number, or the
    // worst case a user was promised is not the worst case they get.
    for amount in [1i128, 999, HUNDRED, HUNDRED * 9] {
        assert_eq!(w.adapter().quote_max_fee(&amount), amount * 7 / 10_000);
    }
    assert_eq!(
        w.adapter().try_quote_max_fee(&0),
        Err(Ok(HyperionError::InvalidAmount))
    );
}

#[test]
fn a_run_of_transfers_is_counted_and_each_one_is_reported_back() {
    let w = World::new();
    for n in 1..=3u64 {
        w.router().bridge_out(&w.user, &request(&w, HUNDRED));
        assert_eq!(w.messenger().burn_count(), n as u32);
    }
    // The router's nonce travels with the burn so the indexer can tie a Circle attestation back
    // to the transfer a user is watching in the app.
    assert_eq!(w.router().last_out_nonce(), 3);
}

// ------------------------------------------------------------------------------------------
// Dust
// ------------------------------------------------------------------------------------------

#[test]
fn circles_rounding_leaves_a_remainder_behind_and_anybody_can_tidy_it_up() {
    let w = World::new();
    // Seven stroops Circle will not carry, which it leaves sitting where it found them.
    w.messenger().init(&7i128, &0i128);

    w.router().bridge_out(&w.user, &request(&w, HUNDRED));
    let net = net_of(HUNDRED);
    assert_eq!(w.messenger().last_burn().burned, net - 7);
    assert_eq!(w.token().balance(&w.adapter_id), 7);

    // A stranger pays a fee to clean up and gains nothing by it, because the destination comes
    // from the router rather than from them.
    let stranger = Address::generate(&w.env);
    assert_eq!(w.adapter().sweep(&stranger, &w.token_id), 7);
    assert_eq!(w.token().balance(&w.adapter_id), 0);
    assert_eq!(w.token().balance(&w.treasury), HUNDRED - net + 7);
    assert_eq!(w.token().balance(&stranger), 0);
}

#[test]
fn sweeping_an_empty_adapter_is_a_quiet_no_op_rather_than_a_failure() {
    let w = World::new();
    // A keeper sweeping on a schedule should not have to check first, and should not produce a
    // failed transaction every time there happens to be nothing to do.
    assert_eq!(w.adapter().sweep(&w.relayer, &w.token_id), 0);
}

#[test]
fn a_sweep_follows_the_treasury_the_router_names_today() {
    let w = World::new();
    w.fund_adapter(500);
    let new_treasury = Address::generate(&w.env);
    w.run_action(hyperion_router::AdminAction::SetTreasury(
        new_treasury.clone(),
    ));

    // Read live rather than copied at initialise time, so moving the treasury does not leave an
    // adapter quietly paying the old one.
    w.adapter().sweep(&w.relayer, &w.token_id);
    assert_eq!(w.token().balance(&new_treasury), 500);
    assert_eq!(w.token().balance(&w.treasury), 0);
}
