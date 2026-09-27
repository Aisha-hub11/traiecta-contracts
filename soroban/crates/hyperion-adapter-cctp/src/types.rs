use soroban_sdk::{contracttype, Address, BytesN, String};

/// Everything this adapter was told once and then relies on.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// In production this is the same multisig that holds the router's admin role. It is kept
    /// separate in storage rather than read back from the router so that a router upgrade cannot
    /// silently move who controls the rail wiring.
    pub admin: Address,
    pub router: Address,
    /// `token-messenger-minter-v2`. Burns on the way out, mints on the way in, and is the
    /// `recipient` named inside every burn message that lands here.
    pub token_messenger: Address,
    /// `message-transmitter-v2`. Checks Circle's signatures.
    pub message_transmitter: Address,
    /// Ceiling on the fee Circle may take, as basis points of the amount being sent.
    ///
    /// Zero is the right answer for a finalized transfer, which is what Hyperion defaults to:
    /// Circle charges nothing for those. Raising it is how an operator opts into fast transfers,
    /// and it has to be raised at the same time as `min_finality_threshold` is lowered, because
    /// Circle refuses a fast transfer whose max fee is below its own minimum.
    pub max_fee_bps: u32,
    /// How final the source chain has to be before Circle will attest.
    ///
    /// 2000 means "wait for real finality", which is the conservative default and the one that
    /// costs nothing. Anything lower is a fast transfer and comes with a fee.
    pub min_finality_threshold: u32,
}

/// A chain Hyperion knows how to reach over CCTP, in both directions.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainLink {
    /// The name the router and the app use, for example `ethereum` or `base`.
    pub chain: String,
    /// Circle's own numbering. Stellar is 27.
    pub domain: u32,
    /// Who may broadcast the resulting message on the far side. All zeroes means anybody.
    pub destination_caller: BytesN<32>,
}
