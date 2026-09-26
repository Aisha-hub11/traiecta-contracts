use soroban_sdk::contracttype;

/// The interoperability rails Hyperion routes over.
///
/// Hyperion deliberately owns none of these. Each variant names a rail that is already
/// live on Stellar mainnet, already audited by someone else, and already carrying real
/// volume. Adding a variant means adding an adapter, never a new trust mechanism.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RouteKind {
    /// Circle CCTP V2. Native USDC burn and mint, zero slippage, no wrapped asset.
    /// Stellar is domain 27.
    Cctp = 0,
    /// Axelar Interchain Token Service. Canonical token registration with built-in
    /// flow limits.
    AxelarIts = 1,
    /// Axelar General Message Passing. Arbitrary cross-chain instructions.
    AxelarGmp = 2,
    /// Allbridge Core. Pooled liquidity, no attestation wait, pays for it in slippage.
    Allbridge = 3,
}

impl RouteKind {
    /// Stable numeric tag used in cross-chain payloads and in indexer records.
    pub fn tag(&self) -> u32 {
        match self {
            RouteKind::Cctp => 0,
            RouteKind::AxelarIts => 1,
            RouteKind::AxelarGmp => 2,
            RouteKind::Allbridge => 3,
        }
    }

    pub fn from_tag(tag: u32) -> Option<RouteKind> {
        match tag {
            0 => Some(RouteKind::Cctp),
            1 => Some(RouteKind::AxelarIts),
            2 => Some(RouteKind::AxelarGmp),
            3 => Some(RouteKind::Allbridge),
            _ => None,
        }
    }

    /// Whether the rail settles against an off-chain attestation the user has to wait on,
    /// as opposed to a liquidity pool that settles immediately.
    pub fn waits_on_attestation(&self) -> bool {
        match self {
            RouteKind::Cctp | RouteKind::AxelarIts | RouteKind::AxelarGmp => true,
            RouteKind::Allbridge => false,
        }
    }

    /// Whether the rail moves the canonical asset rather than a pooled or wrapped
    /// representation of it. Canonical routes cannot slip.
    pub fn is_canonical(&self) -> bool {
        match self {
            RouteKind::Cctp | RouteKind::AxelarIts => true,
            RouteKind::AxelarGmp | RouteKind::Allbridge => false,
        }
    }
}
