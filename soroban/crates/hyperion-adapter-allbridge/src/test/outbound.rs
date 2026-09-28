//! The way out: router to adapter to Allbridge's pool, and the refusals along the way.

use hyperion_core::{
    inbound::{Origin, Recipient},
    AddressKind, HyperionError, RouteKind,
};
use soroban_sdk::{Bytes, BytesN, String, U256};

use super::allbridge::{MockPool, MockPoolClient};
use super::setup::{
    after_pool, gross_of, net_of, World, BASE_CHAIN_ID, ETH_CHAIN_ID, FLOW_LIMIT, GAS_FLOAT,
    HUNDRED, MESSENGER_COST, POOL_FEE_BPS, RELAY_COST,
};

/// The nonce the router handed out, as Allbridge wants to see it.
fn rail_nonce(w: &World, nonce: u64) -> U256 {
    U256::from_u128(&w.env, u128::from(nonce))
}

// ------------------------------------------------------------------------------------------
// The happy paths
// ------------------------------------------------------------------------------------------

#[test]
fn a_transfer_walks_from_the_router_through_the_adapter_and_into_the_pool() {
    let w = World::new();
    let nonce = w.router().bridge_out(&w.user, &w.request(HUNDRED));

    let sent = w.bridge().last_sent().unwrap();
    // Allbridge only ever sees Hyperion's own contract, which is what lets the router take the
    // fee before any of this happens.
    assert_eq!(sent.sender, w.adapter_id);
    assert_eq!(sent.token, w.token_id);
    assert_eq!(sent.amount, net_of(HUNDRED) as u128);
    assert_eq!(sent.recipient, w.evm(0x01));
    assert_eq!(sent.destination_chain_id, ETH_CHAIN_ID);
    assert_eq!(sent.receive_token, w.receive_token());
    assert_eq!(sent.nonce, rail_nonce(&w, nonce));

    // And the money actually moved, all the way into the pool. The adapter is a hallway.
    assert_eq!(w.pool().sold(), net_of(HUNDRED) as u128);
    assert_eq!(w.token().balance(&w.pool_id), net_of(HUNDRED));
    assert_eq!(w.token().balance(&w.adapter_id), 0);
    assert_eq!(w.token().balance(&w.treasury), HUNDRED - net_of(HUNDRED));
    assert_eq!(w.token().balance(&w.user), HUNDRED * 10 - gross_of(HUNDRED));
}

#[test]
fn what_crosses_is_what_survived_the_pool_rather_than_what_went_in() {
    let w = World::new();
    w.router().bridge_out(&w.user, &w.request(HUNDRED));

    // This is the whole character of a pooled route. The router promised a net amount, the pool
    // took its cut of that, and the number Allbridge attests to is the smaller one. Anybody
    // quoting this route has to quote the second number, not the first.
    let sent = w.bridge().last_sent().unwrap();
    assert_eq!(sent.v_usd, after_pool(net_of(HUNDRED)));
    assert!(sent.v_usd < sent.amount);
    assert_eq!(
        sent.amount - sent.v_usd,
        net_of(HUNDRED) as u128 * u128::from(POOL_FEE_BPS) / 10_000
    );
}

#[test]
fn the_relay_is_paid_out_of_the_float_and_never_out_of_the_transfer() {
    let w = World::new();
    assert_eq!(w.adapter().gas_balance(), GAS_FLOAT);

    w.router().bridge_out(&w.user, &w.request(HUNDRED));

    // The float went down by exactly the bill and the transfer did not go down at all. Carving
    // the fee out of the transfer instead would mean quoting one number and delivering another.
    assert_eq!(w.adapter().gas_balance(), GAS_FLOAT - RELAY_COST as i128);
    let sent = w.bridge().last_sent().unwrap();
    assert_eq!(sent.amount, net_of(HUNDRED) as u128);
    assert_eq!(sent.gas_amount, RELAY_COST);
    assert_eq!(sent.fee_token_amount, 0);
}

#[test]
fn both_halves_of_the_bill_are_paid_and_the_messenger_gets_its_own() {
    let w = World::new();
    w.router().bridge_out(&w.user, &w.request(HUNDRED));

    // Allbridge bills for the relay in two pieces and checks the sum. An adapter that asked
    // only the bridge what it charged would come up short by the messenger's half, every time,
    // and find out about it two contracts deep.
    assert_eq!(w.adapter().quote_gas(&w.ethereum()), RELAY_COST);
    assert_eq!(w.native().balance(&w.messenger_id), MESSENGER_COST as i128);
    assert_eq!(w.messenger().sent_count(), 1);
}

#[test]
fn the_message_allbridge_sends_carries_both_chain_ids_in_its_first_two_bytes() {
    let w = World::new();
    w.router().bridge_out(&w.user, &w.request(HUNDRED));

    // Not a quirk to be tidied away. The messenger reads the destination back out of byte one
    // rather than being told it, so a digest built without the stamp routes nowhere.
    let message = w.messenger().last_message().unwrap().to_array();
    assert_eq!(message[0], 7);
    assert_eq!(message[1], ETH_CHAIN_ID as u8);
    assert_eq!(w.bridge().last_sent().unwrap().message.to_array(), message);
}

#[test]
fn when_allbridge_calls_this_adapter_its_rebalancer_the_pool_takes_nothing() {
    let w = World::new();
    // Allbridge waives the pool fee for its own rebalancer, and it passes that decision down to
    // the pool as an argument. The adapter has to authorise the same boolean the bridge will
    // send, so this test is really about an authorisation matching rather than about a discount.
    assert_eq!(w.bridge().get_config().rebalancer, w.rebalancer);
    w.bridge().set_rebalancer(&w.adapter_id);
    w.router().bridge_out(&w.user, &w.request(HUNDRED));

    let sent = w.bridge().last_sent().unwrap();
    assert_eq!(sent.v_usd, net_of(HUNDRED) as u128);
}

#[test]
fn the_pool_is_looked_up_fresh_on_every_transfer() {
    let w = World::new();
    let moved = w.env.register(MockPool, ());
    MockPoolClient::new(&w.env, &moved).initialize(&w.bridge_id, &w.token_id, &POOL_FEE_BPS);
    // Allbridge's admin can repoint an asset at a different pool whenever they like. A copy of
    // the old address kept here would authorise a transfer into a contract that is no longer
    // part of the rail.
    w.bridge().add_pool(&moved, &w.token_key(&w.token_id));

    assert_eq!(w.adapter().pool_for(&w.token_id), moved);
    w.router().bridge_out(&w.user, &w.request(HUNDRED));
    assert_eq!(
        MockPoolClient::new(&w.env, &moved).sold(),
        net_of(HUNDRED) as u128
    );
    assert_eq!(w.pool().sold(), 0);
}

#[test]
fn two_lanes_stay_two_lanes() {
    let w = World::new();
    w.adapter().link_chain(&w.base(), &BASE_CHAIN_ID);
    w.adapter()
        .link_asset(&w.token_id, &w.base(), &w.receive_token());

    let mut request = w.request(HUNDRED);
    request.destination.chain = w.base();
    w.router().bridge_out(&w.user, &request);

    assert_eq!(
        w.bridge().last_sent().unwrap().destination_chain_id,
        BASE_CHAIN_ID
    );
    assert_eq!(
        w.messenger().last_message().unwrap().to_array()[1],
        BASE_CHAIN_ID as u8
    );
}

#[test]
fn the_nonce_on_the_wire_is_the_routers_own_and_it_moves_every_time() {
    let w = World::new();
    let first = w.router().bridge_out(&w.user, &w.request(HUNDRED));
    // Allbridge hashes the nonce into the message it attests to, so two identical transfers
    // with the same nonce would collide and the second would be refused as already sent. The
    // router is the only thing handing them out, and it never repeats one.
    let first_message = w.messenger().last_message().unwrap();
    let second = w.router().bridge_out(&w.user, &w.request(HUNDRED));
    assert_eq!(second, first + 1);
    assert_eq!(
        w.bridge().last_sent().unwrap().nonce,
        rail_nonce(&w, second)
    );
    assert_eq!(w.bridge().sent_count(), 2);
    assert_ne!(w.messenger().last_message().unwrap(), first_message);
}

// ------------------------------------------------------------------------------------------
// Refusals
// ------------------------------------------------------------------------------------------

#[test]
fn only_the_router_can_ask_for_a_dispatch() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Signed perfectly well, by somebody who is not the router. The adapter holds funds between
    // two calls in the same transaction and this is the only thing standing between a stranger
    // and them.
    assert_eq!(
        w.adapter().try_dispatch(
            &w.relayer,
            &w.token_id,
            &HUNDRED,
            &w.ethereum(),
            &w.evm(0x01),
            &1,
        ),
        Err(Ok(HyperionError::Unauthorized))
    );
    assert_eq!(w.pool().sold(), 0);
}

#[test]
fn a_dispatch_of_nothing_is_refused() {
    let w = World::new();
    for amount in [0i128, -1, -HUNDRED] {
        assert_eq!(
            w.adapter().try_dispatch(
                &w.router_id,
                &w.token_id,
                &amount,
                &w.ethereum(),
                &w.evm(0x01),
                &1,
            ),
            Err(Ok(HyperionError::InvalidAmount))
        );
    }
}

#[test]
fn a_chain_nobody_linked_is_refused() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    assert_eq!(
        w.adapter().try_dispatch(
            &w.router_id,
            &w.token_id,
            &HUNDRED,
            &String::from_str(&w.env, "polygon"),
            &w.evm(0x01),
            &1,
        ),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn an_asset_nobody_mapped_is_refused() {
    let w = World::new();
    // A perfectly real asset with no link to anything on the far side. There is nothing to name
    // as the receive token, and guessing is not an option.
    assert_eq!(
        w.adapter().try_dispatch(
            &w.router_id,
            &w.orphan_id,
            &HUNDRED,
            &w.ethereum(),
            &w.evm(0x01),
            &1,
        ),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

#[test]
fn a_destination_that_is_not_an_evm_address_is_refused() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Thirty two bytes of something. Allbridge would take them without complaint and pay
    // whatever they turned out to mean, which is a place nobody can recover funds from.
    assert_eq!(
        w.adapter().try_dispatch(
            &w.router_id,
            &w.token_id,
            &HUNDRED,
            &w.ethereum(),
            &BytesN::from_array(&w.env, &[0x33u8; 32]),
            &1,
        ),
        Err(Ok(HyperionError::NotEvmAddress))
    );
}

#[test]
fn a_destination_of_nobody_is_refused() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    assert_eq!(
        w.adapter().try_dispatch(
            &w.router_id,
            &w.token_id,
            &HUNDRED,
            &w.ethereum(),
            &w.zero_word(),
            &1,
        ),
        Err(Ok(HyperionError::ZeroAddressKey))
    );
}

#[test]
fn a_float_that_cannot_cover_the_next_hop_stops_the_transfer_before_it_starts() {
    let w = World::unlinked();
    w.link_ethereum();
    w.map_token();
    w.fund_user(HUNDRED * 10);
    // Enough to be obviously deliberate and one stroop short of the bill.
    w.fund_gas(RELAY_COST as i128 - 1);

    assert_eq!(
        w.adapter().try_dispatch(
            &w.router_id,
            &w.token_id,
            &HUNDRED,
            &w.ethereum(),
            &w.evm(0x01),
            &1,
        ),
        Err(Ok(HyperionError::GasFloatTooLow))
    );
    // Refused before anything sold, which is the point of checking first. A transfer that sold
    // into the pool and then failed to pay for its relay would leave the money on the wrong side
    // of a swap with nobody to unwind it.
    assert_eq!(w.pool().sold(), 0);
    assert_eq!(w.bridge().sent_count(), 0);

    // One more stroop and the same transfer goes.
    w.fund_gas(1);
    w.router().bridge_out(&w.user, &w.request(HUNDRED));
    assert_eq!(w.bridge().sent_count(), 1);
    assert_eq!(w.adapter().gas_balance(), 0);
}

#[test]
fn a_rail_that_stopped_swapping_takes_the_whole_transfer_down_with_it() {
    let w = World::new();
    // Allbridge's own stop authority can halt swapping long after a route was wired up, and
    // when it does the refusal arrives after the router has already moved the money.
    w.bridge().set_can_swap(&false);
    assert!(!w.adapter().rail_open());

    let before = w.token().balance(&w.user);
    assert!(w
        .router()
        .try_bridge_out(&w.user, &w.request(HUNDRED))
        .is_err());

    // Nothing partial survives. Not the fee, not the float, not a half sold position.
    assert_eq!(w.token().balance(&w.user), before);
    assert_eq!(w.token().balance(&w.treasury), 0);
    assert_eq!(w.token().balance(&w.adapter_id), 0);
    assert_eq!(w.adapter().gas_balance(), GAS_FLOAT);
    assert_eq!(w.pool().sold(), 0);
}

#[test]
fn the_rail_being_open_is_something_anybody_can_ask_before_they_sign() {
    let w = World::new();
    assert!(w.adapter().rail_open());
    w.bridge().set_can_swap(&false);
    assert!(!w.adapter().rail_open());
    w.bridge().set_can_swap(&true);
    assert!(w.adapter().rail_open());
}

#[test]
fn a_quote_follows_allbridges_prices_rather_than_a_number_somebody_wrote_down() {
    let w = World::new();
    assert_eq!(w.adapter().quote_gas(&w.ethereum()), RELAY_COST);

    // The messenger reprices whenever the destination's gas does, which is often.
    w.messenger().set_cost(&ETH_CHAIN_ID, &(MESSENGER_COST * 4));
    assert_eq!(
        w.adapter().quote_gas(&w.ethereum()),
        RELAY_COST + MESSENGER_COST * 3
    );
    assert_eq!(
        w.adapter().try_quote_gas(&w.base()),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn a_transfer_the_router_will_not_let_through_never_reaches_allbridge() {
    let w = World::new();
    let mut request = w.request(FLOW_LIMIT * 2);
    request.route = RouteKind::Allbridge;
    // The flow limit is the router's business and not this adapter's, but it is worth one test
    // that the two are wired the right way round: the limit is checked before the money moves,
    // not after Allbridge has already been handed it.
    assert!(w.router().try_bridge_out(&w.user, &request).is_err());
    assert_eq!(w.bridge().sent_count(), 0);
    assert_eq!(w.adapter().gas_balance(), GAS_FLOAT);
}

#[test]
fn nothing_arrives_on_this_route_because_there_is_nobody_to_accept_it_from() {
    let w = World::new();
    // Allbridge's attested message has no payload field, so there is no way to carry a Hyperion
    // destination across and no callback to receive one. The router's rail receiver for this
    // route is left unset on purpose, and that is what makes an inbound transfer impossible
    // rather than merely unimplemented. Even the adapter itself cannot get a delivery accepted:
    // there is no address to compare it against, so the router never gets as far as comparing.
    assert_eq!(
        w.router().try_bridge_in(
            &w.adapter_id,
            &RouteKind::Allbridge,
            &w.token_id,
            &HUNDRED,
            &Recipient {
                address: w.user.clone(),
                kind: AddressKind::Account,
                raw: Bytes::new(&w.env),
            },
            &Origin {
                chain: w.ethereum(),
                nonce: 1,
                message_id: BytesN::from_array(&w.env, &[0x01; 32]),
                sender: w.remote_bridge(0xE1),
            },
        ),
        Err(Ok(HyperionError::AdapterNotSet))
    );
}
