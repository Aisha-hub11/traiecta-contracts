// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

/// @title The parts of Axelar's Interchain Token Service Hyperion actually calls
/// @notice Same reasoning as the CCTP interface: only what is used, declared here rather than
/// vendored wholesale, so a change in somebody else's repository cannot quietly change what this
/// one compiles against.
interface IInterchainTokenService {
    /// @notice Move a registered token to another chain, optionally with a payload.
    /// @dev The payload is what turns a transfer into a delivery instruction. Axelar delivers to a
    /// contract address, and Hyperion's contract on the far side has to learn who the tokens are
    /// for, so it travels in `data`.
    ///
    /// `metadata` is Axelar's own envelope: four bytes of version followed by the payload, and
    /// completely empty when there is no payload. Empty and "version zero plus nothing" are not
    /// the same thing to the receiving side, which is a distinction worth stating once here rather
    /// than rediscovering from a failed transfer.
    /// @param tokenId ITS's identifier for the token, which is the same value on every chain.
    /// @param destinationChain Axelar's name for the far side, for example "stellar".
    /// @param destinationAddress The receiving contract, as raw bytes, because chains disagree
    /// about how long an address is.
    /// @param amount How much to move, in this chain's decimals for the token.
    /// @param metadata Version and payload, or empty for a bare transfer.
    /// @param gasValue What to spend on destination gas, taken from `msg.value`.
    function interchainTransfer(
        bytes32 tokenId,
        string calldata destinationChain,
        bytes calldata destinationAddress,
        uint256 amount,
        bytes calldata metadata,
        uint256 gasValue
    ) external payable;

    /// @notice The ERC20 this chain uses for a token id, or zero if it does not know the id.
    /// @param tokenId ITS's identifier for the token.
    /// @return The local ERC20, or the zero address when ITS has no record of that id here.
    function registeredTokenAddress(bytes32 tokenId) external view returns (address);
}

/// @title What ITS calls on the receiving contract
/// @notice Implemented by Hyperion's Axelar adapter. ITS transfers the tokens in first and calls
/// this second, so the amount is already here by the time the function body runs.
interface IInterchainTokenExecutable {
    /// @notice Accept a transfer that arrived with a payload attached.
    /// @dev Must return `keccak256("its-execute-success")` or ITS treats the delivery as failed.
    /// Returning a value rather than simply not reverting is Axelar's way of making sure the
    /// receiving contract meant to accept this, rather than having a fallback that swallows it.
    /// @param commandId Axelar's identifier for this delivery. Unique, and the replay key.
    /// @param sourceChain Axelar's name for where it came from.
    /// @param sourceAddress The sending contract as raw bytes. Twenty bytes for an EVM chain,
    /// thirty two for Stellar, which is why this is bytes and not an address.
    /// @param data The payload the sender attached.
    /// @param tokenId ITS's identifier for the token.
    /// @param token The ERC20 on this chain.
    /// @param amount How much arrived.
    function executeWithInterchainToken(
        bytes32 commandId,
        string calldata sourceChain,
        bytes calldata sourceAddress,
        bytes calldata data,
        bytes32 tokenId,
        address token,
        uint256 amount
    ) external returns (bytes32);
}

/// @title Axelar's gas service
/// @notice Used to top up a delivery that ran short.
/// @dev Worth having even though ITS bills gas at send time, because a destination chain whose
/// gas price moved between quote and execution leaves a transfer sitting in Axelar's queue with
/// no way forward except more gas. Anybody can pay it, which is the point.
interface IAxelarGasService {
    /// @notice Add native gas to a delivery that has already been paid for once.
    /// @param txHash The transaction on this chain that started the delivery.
    /// @param logIndex Which log within that transaction, since one transaction can start several.
    /// @param refundAddress Where anything left over goes once the delivery settles.
    function addNativeGas(bytes32 txHash, uint256 logIndex, address refundAddress) external payable;
}
