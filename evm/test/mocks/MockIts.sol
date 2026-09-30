// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {IERC20} from "openzeppelin/token/ERC20/IERC20.sol";

import {IInterchainTokenExecutable, IInterchainTokenService} from "../../src/interfaces/IAxelar.sol";

/// @title Axelar's Interchain Token Service, both directions
/// @notice Sends by recording, and delivers by doing what the real one does in the order it does
/// it: move the tokens first, then call the receiving contract.
/// @dev That order is the whole reason the adapter's inbound path does not pull anything. If this
/// mock called first and transferred second, the adapter would look correct while being wrong,
/// which is the most expensive kind of passing test.
contract MockIts is IInterchainTokenService {
    struct Sent {
        bytes32 tokenId;
        string destinationChain;
        bytes destinationAddress;
        uint256 amount;
        bytes metadata;
        uint256 gasValue;
        uint256 value;
    }

    Sent private _lastSent;
    uint256 public sentCount;

    mapping(bytes32 tokenId => address token) private _registered;

    /// @dev Some of the value is handed back, the way a real destination gas market refunds what
    /// it did not spend. Set to zero for the case where the rail keeps all of it.
    uint256 public refundBps;

    function setRegistered(bytes32 tokenId, address token) external {
        _registered[tokenId] = token;
    }

    function setRefundBps(uint256 bps) external {
        refundBps = bps;
    }

    function registeredTokenAddress(bytes32 tokenId) external view override returns (address) {
        return _registered[tokenId];
    }

    function interchainTransfer(
        bytes32 tokenId,
        string calldata destinationChain,
        bytes calldata destinationAddress,
        uint256 amount,
        bytes calldata metadata,
        uint256 gasValue
    ) external payable override {
        address token = _registered[tokenId];
        require(token != address(0), "unknown token id");
        // Reverts on a missing allowance, which is exactly the failure worth having.
        // forge-lint: disable-next-line(erc20-unchecked-transfer)
        IERC20(token).transferFrom(msg.sender, address(this), amount);
        _lastSent = Sent({
            tokenId: tokenId,
            destinationChain: destinationChain,
            destinationAddress: destinationAddress,
            amount: amount,
            metadata: metadata,
            gasValue: gasValue,
            value: msg.value
        });
        ++sentCount;

        if (refundBps != 0 && msg.value != 0) {
            (bool sent,) = payable(msg.sender).call{value: (msg.value * refundBps) / 10_000}("");
            require(sent, "refund");
        }
    }

    /// @notice Deliver a transfer to a receiving contract the way ITS does.
    /// @dev Holds a float of the token so a test can deliver without minting into the receiver
    /// directly, which would hide a receiver that reads its own balance instead of its arguments.
    function deliver(
        address receiver,
        bytes32 commandId,
        string calldata sourceChain,
        bytes calldata sourceAddress,
        bytes calldata data,
        bytes32 tokenId,
        address token,
        uint256 amount
    ) external returns (bytes32) {
        // Raw, unchecked, in that order, because that is what the real service does and
        // the ordering is the whole point of this helper.
        // forge-lint: disable-next-line(erc20-unchecked-transfer)
        IERC20(token).transfer(receiver, amount);
        return IInterchainTokenExecutable(receiver)
            .executeWithInterchainToken(commandId, sourceChain, sourceAddress, data, tokenId, token, amount);
    }

    function lastSent() external view returns (Sent memory) {
        return _lastSent;
    }
}
