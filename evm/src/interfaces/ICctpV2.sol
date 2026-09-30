// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

/// @title The parts of Circle's CCTP V2 Hyperion actually calls
/// @notice Trimmed to what is used. A full copy of somebody else's interface in a repository is a
/// copy that drifts, and the compiler cannot tell you it has drifted, so this declares the three
/// functions Hyperion depends on and nothing else.
///
/// Sources are the deployed contracts, not the docs: TokenMessengerV2 and MessageTransmitterV2 on
/// every domain Hyperion routes to. Stellar is domain 27.
interface ITokenMessengerV2 {
    /// @notice Burn USDC here and instruct the far side to mint it, with a payload attached.
    /// @dev The hook data is the only reason Hyperion uses this rather than the plain
    /// `depositForBurn`. CCTP's mint recipient is a bare thirty two byte slot with nowhere to say
    /// whether it names an account or a contract, and on Stellar that distinction decides whether
    /// delivery is a payment or a contract call. So the mint recipient names Hyperion's adapter
    /// over there and the real destination rides in the hook.
    /// @param amount USDC to burn, in the token's own decimals.
    /// @param destinationDomain Circle's number for the far side. Stellar is 27.
    /// @param mintRecipient Who mints on the far side, left padded into thirty two bytes.
    /// @param burnToken The USDC contract on this chain.
    /// @param destinationCaller Who is allowed to deliver, or zero for anybody. Hyperion leaves
    /// this open: restricting it would mean Hyperion has to be the one to press the button, and a
    /// transfer that only completes when its operator is awake is not a transfer people can trust.
    /// @param maxFee The most the sender will pay for fast transfer, taken from the amount.
    /// @param minFinalityThreshold 2000 for finalized, lower for fast. Hyperion asks for
    /// finalized, because a bridge that takes reorg risk to save ten minutes is trading somebody
    /// else's money for its own benchmark.
    /// @param hookData The payload the far side reads to learn where the money actually goes.
    function depositForBurnWithHook(
        uint256 amount,
        uint32 destinationDomain,
        bytes32 mintRecipient,
        address burnToken,
        bytes32 destinationCaller,
        uint256 maxFee,
        uint32 minFinalityThreshold,
        bytes calldata hookData
    ) external;

    /// @notice The plain burn, with no payload.
    /// @dev Used when the destination is an EVM chain, where the mint recipient is a plain address
    /// and there is nothing left to say. No hook means no adapter needed on the far side at all:
    /// USDC lands in the recipient's own wallet and Hyperion is not in the path.
    /// @param amount How much USDC to burn here and mint there.
    /// @param destinationDomain Circle's own number for the destination chain, which is not a
    /// chain id and has to be looked up rather than derived.
    /// @param mintRecipient Who receives the mint, left padded to a full word.
    /// @param burnToken The USDC contract on this chain.
    /// @param destinationCaller Who is allowed to deliver the attested message, or zero for
    /// anybody. Hyperion always leaves this at zero so a stuck transfer is never stuck on one key.
    /// @param maxFee The most Circle may take out of the transferred amount.
    /// @param minFinalityThreshold How settled this chain has to be before Circle will attest.
    function depositForBurn(
        uint256 amount,
        uint32 destinationDomain,
        bytes32 mintRecipient,
        address burnToken,
        bytes32 destinationCaller,
        uint256 maxFee,
        uint32 minFinalityThreshold
    ) external;
}

/// @title Circle's message transmitter
/// @notice The receiving half, used by whoever delivers an attested message.
interface IMessageTransmitterV2 {
    /// @notice Hand Circle a signed message and its attestation, and let it mint.
    /// @dev Permissionless when the burn left `destinationCaller` at zero, which Hyperion's always
    /// does. Anybody can push somebody else's transfer through, including the recipient.
    /// @param message The message Circle emitted on the source chain, byte for byte.
    /// @param attestation Circle's signature over that message.
    function receiveMessage(bytes calldata message, bytes calldata attestation) external;

    /// @notice This chain's Circle domain number.
    function localDomain() external view returns (uint32);
}
