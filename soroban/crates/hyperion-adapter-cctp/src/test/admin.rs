//! Setup, configuration, and the refusals that keep a misconfigured lane closed.

use hyperion_core::{cctp, HyperionError};
use soroban_sdk::{testutils::Address as _, Address, BytesN, String};

use super::setup::{World, ETHEREUM_DOMAIN, ETH_USDC};
use crate::{CctpAdapter, CctpAdapterClient};

#[test]
fn an_adapter_nobody_set_up_refuses_to_do_anything() {
    let w = World::new();
    let fresh = CctpAdapterClient::new(&w.env, &w.env.register(CctpAdapter, ()));

    // Not paused, not broken, just never told who it works for. Every path goes through the config
    // and there is no default to fall back on, which is the behaviour you want on the day somebody
    // deploys the WASM and forgets the second half of the script.
    assert_eq!(
        fresh.try_get_config(),
        Err(Ok(HyperionError::NotInitialized))
    );
    assert_eq!(
        fresh.try_sweep(&w.relayer, &w.token_id),
        Err(Ok(HyperionError::NotInitialized))
    );
    assert_eq!(
        fresh.try_receive(&w.relayer, &w.inbound(1_000, 1), &w.attestation()),
        Err(Ok(HyperionError::NotInitialized))
    );
}

#[test]
fn setting_it_up_twice_is_refused() {
    let w = World::new();
    assert_eq!(
        w.adapter()
            .try_initialize(&w.admin, &w.router_id, &w.messenger_id, &w.transmitter_id),
        Err(Ok(HyperionError::AlreadyInitialized))
    );
}

#[test]
fn it_starts_on_the_cautious_setting_with_no_lane_open() {
    let w = World::unlinked();
    let cfg = w.adapter().get_config();

    assert_eq!(cfg.admin, w.admin);
    assert_eq!(cfg.router, w.router_id);
    assert_eq!(cfg.token_messenger, w.messenger_id);
    assert_eq!(cfg.message_transmitter, w.transmitter_id);
    // Zero fee budget and full finality. An operator who wants fast transfers has to ask for them
    // and accept the fee, rather than inheriting one from a default nobody chose.
    assert_eq!(cfg.max_fee_bps, 0);
    assert_eq!(
        cfg.min_finality_threshold,
        cctp::FINALITY_THRESHOLD_FINALIZED
    );

    assert_eq!(
        w.adapter().try_domain_for(&w.ethereum()),
        Err(Ok(HyperionError::UnknownChain))
    );
    assert_eq!(
        w.adapter().try_get_link(&ETHEREUM_DOMAIN),
        Err(Ok(HyperionError::UnknownChain))
    );
    assert_eq!(
        w.adapter().try_asset_for(&ETHEREUM_DOMAIN, &w.eth_usdc()),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

// ------------------------------------------------------------------------------------------
// Linking chains
// ------------------------------------------------------------------------------------------

#[test]
fn a_linked_domain_reads_back_in_both_directions() {
    let w = World::new();
    let link = w.adapter().get_link(&ETHEREUM_DOMAIN);

    assert_eq!(link.domain, ETHEREUM_DOMAIN);
    assert_eq!(link.chain, w.ethereum());
    // A chain name going out, a domain number coming in, and the same pair either way. Getting
    // this backwards is how a transfer to Base ends up on Arbitrum.
    assert_eq!(w.adapter().domain_for(&w.ethereum()), ETHEREUM_DOMAIN);
    assert_eq!(
        w.adapter().asset_for(&ETHEREUM_DOMAIN, &w.eth_usdc()),
        w.token_id
    );
}

#[test]
fn a_chain_with_no_name_cannot_be_linked() {
    let w = World::unlinked();
    assert_eq!(
        w.adapter().try_link_domain(
            &String::from_str(&w.env, ""),
            &ETHEREUM_DOMAIN,
            &BytesN::from_array(&w.env, &[0u8; 32])
        ),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn stellar_cannot_be_linked_to_itself() {
    let w = World::unlinked();
    // Circle would refuse a transfer addressed to the domain it originated on anyway. Refusing it
    // here keeps the reason readable instead of surfacing as somebody else's error code.
    assert_eq!(
        w.adapter().try_link_domain(
            &String::from_str(&w.env, "stellar"),
            &cctp::STELLAR_DOMAIN,
            &BytesN::from_array(&w.env, &[0u8; 32])
        ),
        Err(Ok(HyperionError::WrongDomain))
    );
}

#[test]
fn linking_the_same_lane_again_replaces_the_caller_on_it() {
    let w = World::new();
    let tightened = BytesN::from_array(&w.env, &[0xA1u8; 32]);
    w.adapter()
        .link_domain(&w.ethereum(), &ETHEREUM_DOMAIN, &tightened);
    assert_eq!(
        w.adapter().get_link(&ETHEREUM_DOMAIN).destination_caller,
        tightened
    );
    // Opening it back up is the same call with an empty caller.
    w.adapter().link_domain(
        &w.ethereum(),
        &ETHEREUM_DOMAIN,
        &BytesN::from_array(&w.env, &[0u8; 32]),
    );
    assert_eq!(
        w.adapter().get_link(&ETHEREUM_DOMAIN).destination_caller,
        BytesN::from_array(&w.env, &[0u8; 32])
    );
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn nobody_but_the_admin_can_open_a_lane() {
    let w = World::unlinked();
    // Drop the blanket mock and hand over no signatures at all. Linking a domain decides where
    // money goes, so it is not a call anybody gets to make.
    w.env.set_auths(&[]);
    w.link_ethereum();
}

// ------------------------------------------------------------------------------------------
// Mapping assets
// ------------------------------------------------------------------------------------------

#[test]
fn an_asset_cannot_be_mapped_onto_a_domain_nobody_linked() {
    let w = World::unlinked();
    // The order matters. A mapping on an unlinked domain would be unreachable on the way in,
    // because the inbound path needs the chain name before it looks for the asset.
    assert_eq!(
        w.adapter()
            .try_map_asset(&ETHEREUM_DOMAIN, &w.eth_usdc(), &w.token_id),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn the_mapping_is_the_allowlist_and_it_is_per_domain() {
    let w = World::new();
    // The same ERC20 bytes on a domain nobody mapped is still not a recognised asset. Circle
    // numbers its domains, and a token address alone does not say which chain it came from.
    assert_eq!(
        w.adapter().try_asset_for(&7, &w.eth_usdc()),
        Err(Ok(HyperionError::TokenNotMapped))
    );
    let other = BytesN::from_array(&w.env, &[0xDEu8; 32]);
    assert_eq!(
        w.adapter().try_asset_for(&ETHEREUM_DOMAIN, &other),
        Err(Ok(HyperionError::TokenNotMapped))
    );
    assert_eq!(
        w.adapter().asset_for(&ETHEREUM_DOMAIN, &w.eth_usdc()),
        w.token_id
    );
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn nobody_but_the_admin_can_add_an_asset() {
    let w = World::new();
    w.env.set_auths(&[]);
    w.adapter().map_asset(
        &ETHEREUM_DOMAIN,
        &BytesN::from_array(&w.env, &ETH_USDC),
        &w.token_id,
    );
}

// ------------------------------------------------------------------------------------------
// Fee policy
// ------------------------------------------------------------------------------------------

#[test]
fn a_fee_budget_above_the_ceiling_is_refused() {
    let w = World::new();
    // A hundred basis points is already far above anything Circle has ever charged. Anything past
    // it is a fat fingered config change, not a policy decision.
    w.adapter().set_fee_policy(&100, &1000);
    assert_eq!(w.adapter().get_config().max_fee_bps, 100);
    assert_eq!(
        w.adapter().try_set_fee_policy(&101, &1000),
        Err(Ok(HyperionError::FeeTooHigh))
    );
    // And the refusal left the old value alone.
    assert_eq!(w.adapter().get_config().max_fee_bps, 100);
}

#[test]
fn a_finality_threshold_outside_circles_range_is_refused() {
    let w = World::new();
    for bad in [0u32, cctp::FINALITY_THRESHOLD_FINALIZED + 1, u32::MAX] {
        assert_eq!(
            w.adapter().try_set_fee_policy(&0, &bad),
            Err(Ok(HyperionError::InvalidLimit))
        );
    }
    // The two ends of the legal range both work.
    w.adapter().set_fee_policy(&0, &1);
    w.adapter()
        .set_fee_policy(&0, &cctp::FINALITY_THRESHOLD_FINALIZED);
    assert_eq!(
        w.adapter().get_config().min_finality_threshold,
        cctp::FINALITY_THRESHOLD_FINALIZED
    );
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn nobody_but_the_admin_can_change_the_fee_policy() {
    let w = World::new();
    w.env.set_auths(&[]);
    w.adapter().set_fee_policy(&100, &1000);
}

// ------------------------------------------------------------------------------------------
// The admin role
// ------------------------------------------------------------------------------------------

#[test]
fn handing_over_the_admin_role_takes_effect_at_once() {
    let w = World::new();
    let successor = Address::generate(&w.env);
    w.adapter().set_admin(&successor);
    assert_eq!(w.adapter().get_config().admin, successor);

    // No timelock on this one. The adapter's admin is the router's own timelocked multisig in
    // production, so the delay already happened on the way to making this call.
    w.env.mock_all_auths();
    w.adapter().set_fee_policy(&1, &1000);
    assert_eq!(w.adapter().get_config().max_fee_bps, 1);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn nobody_but_the_admin_can_hand_the_role_on() {
    let w = World::new();
    w.env.set_auths(&[]);
    w.adapter().set_admin(&w.relayer);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn nobody_but_the_admin_can_replace_the_code() {
    let w = World::new();
    w.env.set_auths(&[]);
    w.adapter().upgrade(&BytesN::from_array(&w.env, &[0u8; 32]));
}

// ------------------------------------------------------------------------------------------
// Housekeeping
// ------------------------------------------------------------------------------------------

#[test]
fn anybody_can_keep_a_quiet_lane_from_being_archived() {
    let w = World::new();
    let keeper = Address::generate(&w.env);
    // Soroban storage expires if nothing touches it, and a lane that sees no traffic for a few
    // months is exactly the lane whose config you do not want to have quietly evaporated. This
    // changes nothing and is open to anybody, so a keeper can run it on a cron with no privileges.
    w.adapter().keep_alive(&ETHEREUM_DOMAIN);
    let _ = keeper;
    assert_eq!(w.adapter().get_link(&ETHEREUM_DOMAIN).chain, w.ethereum());

    // Even a domain nobody linked, which is a no-op rather than an error, because a keeper
    // sweeping a list of domains should not fail on the one that was retired last week.
    w.adapter().keep_alive(&super::setup::UNLINKED_DOMAIN);
}

#[test]
fn keeping_an_unconfigured_adapter_alive_is_refused() {
    let w = World::new();
    let fresh = CctpAdapterClient::new(&w.env, &w.env.register(CctpAdapter, ()));
    assert_eq!(
        fresh.try_keep_alive(&ETHEREUM_DOMAIN),
        Err(Ok(HyperionError::NotInitialized))
    );
}
