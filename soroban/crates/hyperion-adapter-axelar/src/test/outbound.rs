//! The way out: router to adapter to Axelar, and the refusals along the way.

use hyperion_core::{address::bytes32_to_evm, axelar::OutboundNote, HyperionError, RouteKind};
use soroban_sdk::{
    testutils::{MockAuth, MockAuthInvoke},
    Bytes, BytesN, IntoVal, String,
};

use super::setup::{gross_of, net_of, World, EVM_DECIMALS, HUNDRED};
use hyperion_router::{Destination, OutboundRequest};

/// The payload the shape that carries a note should have produced.
fn note_for(w: &World, last: u8, nonce: u64) -> Bytes {
    let recipient = bytes32_to_evm(&w.env, &w.evm(last)).unwrap();
    OutboundNote::new(recipient, nonce).encode(&w.env).unwrap()
}

// ------------------------------------------------------------------------------------------
// The happy paths
// ------------------------------------------------------------------------------------------

#[test]
fn a_transfer_walks_from_the_router_through_the_adapter_and_out_as_a_burn() {
    let w = World::new();
    let nonce = w.router().bridge_out(&w.user, &w.request(HUNDRED));

    let sent = w.its().last_transfer();
    // The adapter pays, not the user. ITS only ever sees Hyperion's own contract, which is what
    // lets the router hold the fee back before any of this happens.
    assert_eq!(sent.caller, w.adapter_id);
    assert_eq!(sent.token_id, w.burn_id());
    // Axelar's name for the chain, not Hyperion's. Sending "ethereum" to a hub that knows it as
    // "Ethereum" is a transfer that goes nowhere.
    assert_eq!(sent.destination_chain, w.axelar_ethereum());
    // Addressed to Hyperion's contract over there rather than to the person being paid, because
    // the person being paid is inside the note.
    assert_eq!(sent.destination_address, w.peer_bytes());
    assert_eq!(sent.amount, net_of(HUNDRED));
    assert_eq!(sent.data, Some(note_for(&w, 0x01, nonce)));

    // And the money actually left. The adapter is a hallway, not a vault.
    assert_eq!(w.token().balance(&w.adapter_id), 0);
    assert_eq!(w.token().balance(&w.treasury), HUNDRED - net_of(HUNDRED));
    assert_eq!(w.token().balance(&w.user), HUNDRED * 10 - gross_of(HUNDRED));
}

#[test]
fn the_bare_shape_addresses_the_recipient_itself_and_attaches_nothing() {
    let w = World::new();
    let mut request = w.request(HUNDRED);
    request.route = RouteKind::AxelarIts;
    w.router().bridge_out(&w.user, &request);

    let sent = w.its().last_transfer();
    assert_eq!(sent.caller, w.bare_id);
    // Twenty bytes naming the recipient, and nothing riding along. This is the plain ITS
    // transfer any Axelar integration would make, which is exactly the point of the shape.
    assert_eq!(
        sent.destination_address,
        Bytes::from_array(
            &w.env,
            &bytes32_to_evm(&w.env, &w.evm(0x01)).unwrap().to_array()
        )
    );
    assert_eq!(sent.data, None);
}

#[test]
fn nobody_pays_axelars_relayer_from_inside_the_contract() {
    let w = World::new();
    w.router().bridge_out(&w.user, &w.request(HUNDRED));

    // The gas service charges its spender a frame below ITS, against a message id that does not
    // exist until the gateway has emitted it. Hyperion's keeper tops the message up afterwards
    // instead, so nothing here ever signs for a relayer fee.
    assert!(!w.its().last_transfer().gas_attached);
}

#[test]
fn a_locked_asset_moves_to_its_manager_rather_than_being_burned() {
    let w = World::new();
    let mut request = w.request(HUNDRED);
    request.token = w.lock_asset.clone();
    w.router().bridge_out(&w.user, &request);

    // Three of Axelar's four manager types burn out of the sender's balance and this one does
    // not. The authorisation the adapter grants has to name the manager by address, so the funds
    // ending up there is the only proof that it named the right one.
    assert_eq!(w.manager().locked(&w.lock_asset), net_of(HUNDRED));
    assert_eq!(w.lock_token().balance(&w.adapter_id), 0);
    assert_eq!(w.its().last_transfer().token_id, w.lock_id());
}

// ------------------------------------------------------------------------------------------
// Authorisation
// ------------------------------------------------------------------------------------------

#[test]
fn one_signature_from_the_sender_is_the_whole_of_what_a_transfer_needs() {
    let w = World::new();
    let request = w.request(HUNDRED);

    // No blanket mock. The only signature on this transaction is the user's, covering their own
    // `bridge_out` and the one transfer that takes their money. Everything after that is
    // contracts acting on their own behalf, and `mock_auths` does not mock contract invoker
    // authorisation, so the burn two frames down only succeeds because the adapter really did
    // grant it.
    w.env.mock_auths(&[MockAuth {
        address: &w.user,
        invoke: &MockAuthInvoke {
            contract: &w.router_id,
            fn_name: "bridge_out",
            args: (w.user.clone(), request.clone()).into_val(&w.env),
            sub_invokes: &[MockAuthInvoke {
                contract: &w.token_id,
                fn_name: "transfer",
                args: (w.user.clone(), w.router_id.clone(), gross_of(HUNDRED)).into_val(&w.env),
                sub_invokes: &[],
            }],
        },
    }]);

    w.router().bridge_out(&w.user, &request);
    assert_eq!(w.its().last_transfer().amount, net_of(HUNDRED));
    assert_eq!(w.token().balance(&w.adapter_id), 0);
}

#[test]
fn the_same_holds_when_the_funds_have_to_be_handed_to_a_third_party() {
    let w = World::new();
    let mut request = w.request(HUNDRED);
    request.token = w.lock_asset.clone();

    // The lock shape is the harder of the two, because the authorisation names an address the
    // adapter looked up at runtime rather than one it was compiled with.
    w.env.mock_auths(&[MockAuth {
        address: &w.user,
        invoke: &MockAuthInvoke {
            contract: &w.router_id,
            fn_name: "bridge_out",
            args: (w.user.clone(), request.clone()).into_val(&w.env),
            sub_invokes: &[MockAuthInvoke {
                contract: &w.lock_asset,
                fn_name: "transfer",
                args: (w.user.clone(), w.router_id.clone(), gross_of(HUNDRED)).into_val(&w.env),
                sub_invokes: &[],
            }],
        },
    }]);

    w.router().bridge_out(&w.user, &request);
    assert_eq!(w.manager().locked(&w.lock_asset), net_of(HUNDRED));
}

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
}

// ------------------------------------------------------------------------------------------
// Refusals
// ------------------------------------------------------------------------------------------

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
    let stranger = w
        .env
        .register_stellar_asset_contract_v2(w.its_id.clone())
        .address();
    // A perfectly real asset that Axelar has no token id for. There is nothing to put in the
    // transfer, and guessing is not an option.
    assert_eq!(
        w.adapter().try_dispatch(
            &w.router_id,
            &stranger,
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
    // Thirty two bytes of something. Truncating it to the last twenty would produce an address
    // that looks entirely ordinary and belongs to nobody.
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
            &BytesN::from_array(&w.env, &[0u8; 32]),
            &1,
        ),
        Err(Ok(HyperionError::ZeroAddressKey))
    );
}

#[test]
fn a_chain_axelar_stopped_trusting_takes_the_whole_transfer_down_with_it() {
    let w = World::new();
    let polygon = String::from_str(&w.env, "polygon");
    let axelar_polygon = String::from_str(&w.env, "Polygon");
    w.its().set_trusted(&axelar_polygon, &true);
    w.adapter().link_chain(&polygon, &axelar_polygon, &w.peer());

    // Axelar can stop routing to a chain long after somebody linked it, and when that happens
    // ITS refuses after it has already taken the funds. Nothing partial survives: the burn, the
    // fee and the sender's balance all go back to where they started.
    w.its().set_trusted(&axelar_polygon, &false);
    let before = w.token().balance(&w.user);
    let request = OutboundRequest {
        token: w.token_id.clone(),
        amount: HUNDRED,
        route: RouteKind::AxelarGmp,
        destination: Destination {
            chain: polygon,
            address: w.evm(0x01),
        },
        destination_decimals: EVM_DECIMALS,
        min_destination_amount: 0,
    };
    assert!(w.router().try_bridge_out(&w.user, &request).is_err());
    assert_eq!(w.token().balance(&w.user), before);
    assert_eq!(w.token().balance(&w.treasury), 0);
    assert_eq!(w.token().balance(&w.adapter_id), 0);
}

#[test]
fn the_nonce_on_the_wire_is_the_routers_own_and_it_moves_every_time() {
    let w = World::new();
    let first = w.router().bridge_out(&w.user, &w.request(HUNDRED));
    // Nonces come from the router and only ever from the router, so two transfers get two, and
    // the note the far side reads carries the one the router actually recorded.
    let second = w.router().bridge_out(&w.user, &w.request(HUNDRED));
    assert_eq!(second, first + 1);
    assert_eq!(
        w.its().last_transfer().data,
        Some(note_for(&w, 0x01, second))
    );
    assert_eq!(w.its().transfer_count(), 2);
}
