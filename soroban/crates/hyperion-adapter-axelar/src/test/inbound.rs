//! The way in: Axelar's callback, the note it carries, and everything that can be wrong with it.

use hyperion_core::{HyperionError, RouteKind};
use soroban_sdk::{xdr::ToXdr, Bytes, BytesN, Env, String};

use super::setup::{World, HUNDRED, STRANGE_TOKEN_ID};

/// The key the router keys replay protection on, recomputed here rather than read back.
///
/// Recomputing it is the point. If the adapter's framing ever changed, an assertion that read
/// the value out of the router would happily agree with the new one.
fn replay_key(env: &Env, source_chain: &String, message_id: &String) -> BytesN<32> {
    env.crypto()
        .keccak256(&(source_chain.clone(), message_id.clone()).to_xdr(env))
        .into()
}

fn message_id(env: &Env, tail: &str) -> String {
    String::from_str(env, tail)
}

// ------------------------------------------------------------------------------------------
// The happy paths
// ------------------------------------------------------------------------------------------

#[test]
fn a_delivery_with_a_note_pays_the_person_the_note_names() {
    let w = World::new();
    let id = message_id(&w.env, "0xfeed-1");

    let claim = w.its().deliver(
        &w.adapter_id,
        &w.axelar_ethereum(),
        &id,
        &w.peer_bytes(),
        &w.note_to_recipient(42),
        &w.burn_id(),
        &HUNDRED,
    );

    // Zero means the router handed the money straight over rather than writing down a claim.
    assert_eq!(claim, 0);
    assert_eq!(w.token().balance(&w.recipient), HUNDRED);
    // Nothing sticks to the adapter or the router on the way through.
    assert_eq!(w.token().balance(&w.adapter_id), 0);
    assert_eq!(w.token().balance(&w.router_id), 0);
    // And the pair of names that identifies this message is spent for good.
    assert!(w.router().was_processed(
        &RouteKind::AxelarGmp,
        &replay_key(&w.env, &w.axelar_ethereum(), &id)
    ));
}

#[test]
fn a_delivery_to_an_account_with_no_trustline_is_parked_rather_than_lost() {
    let w = World::new();
    let claim_id = w.its().deliver(
        &w.adapter_id,
        &w.axelar_ethereum(),
        &message_id(&w.env, "0xfeed-2"),
        &w.peer_bytes(),
        &w.note_to_classic(7),
        &w.burn_id(),
        &HUNDRED,
    );

    // This is the whole argument for routing Axelar through Hyperion instead of letting ITS pay
    // the recipient directly. A bare transfer to an account with no trustline reverts the entire
    // Axelar execution and strands the message with no retry that will ever work. Here the funds
    // sit with the router under the recipient's name until they open one.
    assert!(claim_id > 0);
    let claim = w.router().get_claim(&claim_id);
    assert_eq!(claim.recipient, w.classic);
    assert_eq!(claim.amount, HUNDRED);
    assert_eq!(claim.route, RouteKind::AxelarGmp);
    assert_eq!(claim.source_chain, w.ethereum());
    assert_eq!(claim.source_nonce, 7);
    assert!(!claim.settled);
    assert_eq!(w.token().balance(&w.router_id), HUNDRED);

    // Anybody may settle it, and until the trustline exists settling politely fails instead of
    // burning the claim.
    assert_eq!(
        w.router().try_settle_claim(&w.relayer, &claim_id),
        Err(Ok(HyperionError::RecipientNotReady))
    );
    assert!(!w.router().get_claim(&claim_id).settled);
}

#[test]
fn a_muxed_destination_survives_the_route_that_can_carry_one() {
    let w = World::new();
    // CCTP has nowhere to put a muxed account and Allbridge has no payload at all, so this is
    // the only shape that can address one. Exchanges lean on them heavily, which makes this less
    // of an edge case than it looks.
    let claim_id = w.its().deliver(
        &w.adapter_id,
        &w.axelar_ethereum(),
        &message_id(&w.env, "0xfeed-3"),
        &w.peer_bytes(),
        &w.note_to_muxed(9, 9_007_199_254_740_993),
        &w.burn_id(),
        &HUNDRED,
    );

    // The claim names the classic account underneath the muxed one, which is where the asset
    // actually lands. The sub-account id lives on in the record of the delivery.
    assert!(claim_id > 0);
    assert_eq!(w.router().get_claim(&claim_id).recipient, w.classic);
}

#[test]
fn two_message_ids_that_would_collide_if_they_were_glued_together_both_go_through() {
    let w = World::new();
    let short = String::from_str(&w.env, "Ethereu");
    w.its().set_trusted(&short, &true);
    w.adapter()
        .link_chain(&String::from_str(&w.env, "ethereu"), &short, &w.peer());

    // "Ethereum" plus "1" and "Ethereu" plus "m1" are the same eight characters in a row. Under
    // a naive join the second delivery would look like a replay of the first and be refused,
    // which is a denial of service an attacker gets to choose. XDR puts a length in front of
    // each string, so the two hash differently.
    let first = w.its().deliver(
        &w.adapter_id,
        &w.axelar_ethereum(),
        &String::from_str(&w.env, "1"),
        &w.peer_bytes(),
        &w.note_to_recipient(1),
        &w.burn_id(),
        &HUNDRED,
    );
    let second = w.its().deliver(
        &w.adapter_id,
        &short,
        &String::from_str(&w.env, "m1"),
        &w.peer_bytes(),
        &w.note_to_recipient(2),
        &w.burn_id(),
        &HUNDRED,
    );

    assert_eq!(first, 0);
    assert_eq!(second, 0);
    assert_eq!(w.token().balance(&w.recipient), HUNDRED * 2);
}

// ------------------------------------------------------------------------------------------
// Authorisation
// ------------------------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Unauthorized")]
fn nobody_but_axelar_can_call_the_callback() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Drop the blanket mock and hand over no signatures at all. The adapter's entire claim to
    // trust on the way in is that ITS is the one calling, and an account cannot sign for a
    // contract address, so there is nothing a stranger could present here.
    w.env.set_auths(&[]);
    w.adapter().execute_with_interchain_token(
        &w.axelar_ethereum(),
        &message_id(&w.env, "0xbad-1"),
        &w.peer_bytes(),
        &w.note_to_recipient(1),
        &w.burn_id(),
        &w.token_id,
        &HUNDRED,
    );
}

#[test]
fn a_payload_from_anybody_but_hyperions_own_contract_is_refused() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Axelar will deliver a payload from any contract on the far side that cares to send one.
    // Only Hyperion's own is allowed to say where money goes.
    let impostor = Bytes::from_array(&w.env, &[0xEEu8; 20]);
    assert_eq!(
        w.adapter().try_execute_with_interchain_token(
            &w.axelar_ethereum(),
            &message_id(&w.env, "0xbad-2"),
            &impostor,
            &w.note_to_recipient(1),
            &w.burn_id(),
            &w.token_id,
            &HUNDRED,
        ),
        Err(Ok(HyperionError::UnexpectedRailContract))
    );
}

// ------------------------------------------------------------------------------------------
// Refusals
// ------------------------------------------------------------------------------------------

#[test]
fn the_bare_shape_has_no_inbound_leg_at_all() {
    let w = World::new();
    // ITS delivers a plain transfer straight to the named address, so this instance is never
    // called back and has no business behaving as though it might be.
    assert_eq!(
        w.bare().try_execute_with_interchain_token(
            &w.axelar_ethereum(),
            &message_id(&w.env, "0xbad-3"),
            &w.peer_bytes(),
            &w.note_to_recipient(1),
            &w.burn_id(),
            &w.token_id,
            &HUNDRED,
        ),
        Err(Ok(HyperionError::UnsupportedRoute))
    );
}

#[test]
fn a_delivery_of_nothing_is_refused() {
    let w = World::new();
    for amount in [0i128, -1] {
        assert_eq!(
            w.adapter().try_execute_with_interchain_token(
                &w.axelar_ethereum(),
                &message_id(&w.env, "0xbad-4"),
                &w.peer_bytes(),
                &w.note_to_recipient(1),
                &w.burn_id(),
                &w.token_id,
                &amount,
            ),
            Err(Ok(HyperionError::InvalidAmount))
        );
    }
}

#[test]
fn a_source_chain_nobody_linked_is_refused() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Axelar knows about plenty of chains Hyperion has not opened a lane to. The inbound leg
    // needs the Hyperion name to put on the claim, and there is not one.
    assert_eq!(
        w.adapter().try_execute_with_interchain_token(
            &String::from_str(&w.env, "Polygon"),
            &message_id(&w.env, "0xbad-5"),
            &w.peer_bytes(),
            &w.note_to_recipient(1),
            &w.burn_id(),
            &w.token_id,
            &HUNDRED,
        ),
        Err(Ok(HyperionError::UnknownChain))
    );
}

#[test]
fn a_token_id_nobody_mapped_is_refused_even_when_axelar_knows_it() {
    let w = World::new();
    let stranger = w
        .env
        .register_stellar_asset_contract_v2(w.its_id.clone())
        .address();
    let stranger_id = BytesN::from_array(&w.env, &STRANGE_TOKEN_ID);
    w.its().register(
        &stranger_id,
        &stranger,
        &w.manager_id,
        &crate::rail::TokenManagerType::MintBurn,
    );

    // ITS will deliver any token id it has been told about. The adapter's own mapping is the
    // allowlist, which is the behaviour you want on the day Axelar registers a token nobody on
    // this side has looked at yet.
    assert_eq!(
        w.adapter().try_execute_with_interchain_token(
            &w.axelar_ethereum(),
            &message_id(&w.env, "0xbad-6"),
            &w.peer_bytes(),
            &w.note_to_recipient(1),
            &stranger_id,
            &stranger,
            &HUNDRED,
        ),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

#[test]
fn a_token_id_naming_one_asset_and_delivering_another_is_refused() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Both halves are mapped and both are legitimate. The pairing is the lie, and taking the
    // rail's word for which asset it just handed over would pay out of the wrong balance.
    assert_eq!(
        w.adapter().try_execute_with_interchain_token(
            &w.axelar_ethereum(),
            &message_id(&w.env, "0xbad-7"),
            &w.peer_bytes(),
            &w.note_to_recipient(1),
            &w.burn_id(),
            &w.lock_asset,
            &HUNDRED,
        ),
        Err(Ok(HyperionError::TokenNotMapped))
    );
}

#[test]
fn a_note_from_a_version_this_build_does_not_speak_is_refused() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Refused rather than skipped past. A payload we cannot read is a destination we would be
    // guessing at, and guessing here means paying the wrong person.
    assert_eq!(
        w.adapter().try_execute_with_interchain_token(
            &w.axelar_ethereum(),
            &message_id(&w.env, "0xbad-8"),
            &w.peer_bytes(),
            &w.note_from_the_future(1),
            &w.burn_id(),
            &w.token_id,
            &HUNDRED,
        ),
        Err(Ok(HyperionError::UnsupportedHookVersion))
    );
}

#[test]
fn a_note_whose_strkey_disagrees_with_its_own_tag_is_refused() {
    let w = World::new();
    w.fund_adapter(HUNDRED);
    // Tagged as a contract, spelled as an account. Believing the tag would hand the funds to a
    // contract id that happens to share thirty two bytes with somebody's account key.
    assert_eq!(
        w.adapter().try_execute_with_interchain_token(
            &w.axelar_ethereum(),
            &message_id(&w.env, "0xbad-9"),
            &w.peer_bytes(),
            &w.note_that_lies_about_its_kind(1),
            &w.burn_id(),
            &w.token_id,
            &HUNDRED,
        ),
        Err(Ok(HyperionError::InvalidDestination))
    );
}

#[test]
fn funds_that_never_arrived_are_not_passed_on() {
    let w = World::new();
    // The adapter reads its own balance instead of believing the amount it was handed. Nothing
    // Axelar does should produce this, which is exactly why it is worth checking: a rail that
    // reports more than it delivered would otherwise have the router write down a claim against
    // money nobody holds.
    assert_eq!(
        w.adapter().try_execute_with_interchain_token(
            &w.axelar_ethereum(),
            &message_id(&w.env, "0xbad-10"),
            &w.peer_bytes(),
            &w.note_to_recipient(1),
            &w.burn_id(),
            &w.token_id,
            &HUNDRED,
        ),
        Err(Ok(HyperionError::NothingMinted))
    );

    // And the same thing through the rail itself, with the funding step left out.
    assert!(w
        .its()
        .try_deliver_empty_handed(
            &w.adapter_id,
            &w.axelar_ethereum(),
            &message_id(&w.env, "0xbad-11"),
            &w.peer_bytes(),
            &w.note_to_recipient(1),
            &w.burn_id(),
            &HUNDRED,
        )
        .is_err());
}

#[test]
fn a_refusal_inside_the_callback_unwinds_the_whole_delivery() {
    let w = World::new();
    let id = message_id(&w.env, "0xbad-12");
    // Axelar hands the tokens over before it makes the call, so a refusal has to take the mint
    // with it. Otherwise every rejected payload leaves a pile of somebody else's money in the
    // adapter and the message marked executed on the gateway.
    assert!(w
        .its()
        .try_deliver(
            &w.adapter_id,
            &w.axelar_ethereum(),
            &id,
            &Bytes::from_array(&w.env, &[0xEEu8; 20]),
            &w.note_to_recipient(1),
            &w.burn_id(),
            &HUNDRED,
        )
        .is_err());

    assert_eq!(w.token().balance(&w.adapter_id), 0);
    assert_eq!(w.token().balance(&w.recipient), 0);
    assert!(!w.router().was_processed(
        &RouteKind::AxelarGmp,
        &replay_key(&w.env, &w.axelar_ethereum(), &id)
    ));
}

#[test]
fn the_same_message_cannot_be_delivered_twice() {
    let w = World::new();
    let id = message_id(&w.env, "0xfeed-4");
    w.its().deliver(
        &w.adapter_id,
        &w.axelar_ethereum(),
        &id,
        &w.peer_bytes(),
        &w.note_to_recipient(1),
        &w.burn_id(),
        &HUNDRED,
    );

    // The gateway marks a message executed and will not route it again, so this is a belt on top
    // of a pair of braces. It is here because the adapter is the last thing that would notice if
    // the gateway ever got that wrong, and the cost of the check is one storage read.
    w.fund_adapter(HUNDRED);
    assert_eq!(
        w.adapter().try_execute_with_interchain_token(
            &w.axelar_ethereum(),
            &id,
            &w.peer_bytes(),
            &w.note_to_recipient(1),
            &w.burn_id(),
            &w.token_id,
            &HUNDRED,
        ),
        Err(Ok(HyperionError::ReplayedMessage))
    );
    assert_eq!(w.token().balance(&w.recipient), HUNDRED);
}

#[test]
fn a_delivery_to_a_contract_lands_without_going_anywhere_near_a_claim() {
    let w = World::new();
    // Worth stating out loud next to the parking test above: the claim path is about missing
    // trustlines, not about the destination being unfamiliar. A contract that has never touched
    // this asset is paid on the spot, because a contract's balance entry springs into existence
    // the moment somebody sends it something.
    let claim_id = w.its().deliver(
        &w.adapter_id,
        &w.axelar_ethereum(),
        &message_id(&w.env, "0xfeed-5"),
        &w.peer_bytes(),
        &w.note_to_contract(&w.manager_id, 3),
        &w.burn_id(),
        &HUNDRED,
    );
    assert_eq!(claim_id, 0);
    assert_eq!(w.manager().locked(&w.token_id), HUNDRED);
}
