use soroban_sdk::contracterror;

/// Every failure mode Hyperion contracts can return.
///
/// These are stable numbers. The SDK maps them to human sentences, the indexer stores them
/// against a transfer record, and the web app shows them. Renumbering one is a breaking
/// change, so append rather than reorder.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum HyperionError {
    // Lifecycle
    AlreadyInitialized = 1,
    NotInitialized = 2,

    // Authorization
    Unauthorized = 3,
    NotRailReceiver = 4,

    // Circuit breakers
    Paused = 5,
    RouteDisabled = 6,
    AdapterNotSet = 7,
    FlowLimitExceeded = 8,

    // Amounts and decimals
    InvalidAmount = 9,
    AmountNotRepresentable = 10,
    DecimalOverflow = 11,
    InvalidDecimals = 12,
    SlippageExceeded = 13,
    FeeTooHigh = 14,

    // Destinations and address encoding
    InvalidDestination = 15,
    ZeroAddressKey = 16,
    MuxedNotSupported = 17,
    NotEvmAddress = 18,
    UnknownChain = 19,

    // Message handling
    ReplayedMessage = 20,
    UnknownNonce = 21,

    // Timelocked admin
    TimelockNotQueued = 22,
    TimelockNotReady = 23,
    TimelockExpired = 24,
    TimelockDelayOutOfRange = 25,

    // Parked deliveries
    ClaimNotFound = 26,
    ClaimAlreadySettled = 27,
    RecipientNotReady = 28,

    // Configuration
    InvalidLimit = 29,
    InvalidWindow = 30,
    TokenNotRegistered = 31,
}
