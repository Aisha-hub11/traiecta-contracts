//! Circle's contracts, as this adapter needs to see them.
//!
//! Declared here rather than pulled in as a dependency on purpose. Circle's Stellar packages are
//! not published to crates.io, and vendoring a git revision of somebody else's workspace into a
//! contract build is a supply chain decision, not a convenience. Two functions on the messenger
//! and one on the transmitter is the entire surface Hyperion touches, and a mismatch would fail
//! immediately and loudly on the first call rather than silently, because Soroban resolves a
//! cross contract call by function name and argument shape at invocation time.
//!
//! Transcribed from `circlefin/stellar-cctp` on branch `master`:
//!   `packages/cctp-interfaces/src/token_messenger.rs`
//!   `packages/cctp-interfaces/src/receiver.rs`

use soroban_sdk::{contractclient, Address, Bytes, BytesN, Env};

/// The burn half of CCTP v2 on Stellar, `token-messenger-minter-v2`.
///
/// Both entry points pull funds with `transfer_from`, which means the caller has to have called
/// `approve` on the asset first. That is a genuine difference from the pattern the rest of
/// Hyperion uses, where a contract pre-authorises a transfer somebody else will invoke, and it
/// is why `dispatch` opens and closes an allowance rather than building an auth entry.
///
/// Two arguments are easy to misread. `destination_caller` of all zeroes means anybody may
/// broadcast the resulting message on the far side, and a real address means only that address
/// may, which is how you stop a third party front running the mint. `max_fee` is quoted in the
/// burn token's own decimals rather than in any canonical base.
#[contractclient(name = "TokenMessengerClient")]
pub trait TokenMessenger {
    #[allow(clippy::too_many_arguments)]
    fn deposit_for_burn(
        env: Env,
        caller: Address,
        amount: i128,
        destination_domain: u32,
        mint_recipient: BytesN<32>,
        burn_token: Address,
        destination_caller: BytesN<32>,
        max_fee: i128,
        min_finality_threshold: u32,
    );

    #[allow(clippy::too_many_arguments)]
    fn deposit_for_burn_with_hook(
        env: Env,
        caller: Address,
        amount: i128,
        destination_domain: u32,
        mint_recipient: BytesN<32>,
        burn_token: Address,
        destination_caller: BytesN<32>,
        max_fee: i128,
        min_finality_threshold: u32,
        hook_data: Bytes,
    );
}

/// The mint half, `message-transmitter-v2`.
///
/// This is the contract that actually checks Circle's attestation signatures. Hyperion never
/// verifies an attestation itself and has no opinion about what a valid one looks like, which is
/// the whole point of routing over a rail instead of building one.
#[contractclient(name = "MessageTransmitterClient")]
pub trait Receiver {
    fn receive_message(env: Env, caller: Address, message: Bytes, signature: Bytes) -> bool;
}
