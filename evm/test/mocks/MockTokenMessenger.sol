// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {IERC20} from "openzeppelin/token/ERC20/IERC20.sol";

import {ITokenMessengerV2} from "../../src/interfaces/ICctpV2.sol";

/// @title Circle's burner, as far as the adapter can tell
/// @notice Takes the tokens and writes down exactly what it was asked to do with them.
/// @dev The real contract burns and Circle's attestation service signs. Neither of those is
/// something a unit test can assert on, so what this checks instead is the part Hyperion is
/// responsible for: the domain, the mint recipient, the finality tier, and the hook bytes that
/// tell the far side who the money belongs to.
contract MockTokenMessenger is ITokenMessengerV2 {
    struct Burn {
        uint256 amount;
        uint32 destinationDomain;
        bytes32 mintRecipient;
        address burnToken;
        bytes32 destinationCaller;
        uint256 maxFee;
        uint32 minFinalityThreshold;
        bytes hookData;
    }

    Burn private _lastBurn;
    uint256 public burnCount;

    function depositForBurnWithHook(
        uint256 amount,
        uint32 destinationDomain,
        bytes32 mintRecipient,
        address burnToken,
        bytes32 destinationCaller,
        uint256 maxFee,
        uint32 minFinalityThreshold,
        bytes calldata hookData
    ) external override {
        // Pulled rather than assumed. If the adapter forgot to approve, this reverts and the
        // test fails here, instead of passing on a burn that could never have happened. That is
        // also why the boolean goes unchecked: the token reverts, so it is always true.
        // forge-lint: disable-next-line(erc20-unchecked-transfer)
        IERC20(burnToken).transferFrom(msg.sender, address(this), amount);
        _lastBurn = Burn({
            amount: amount,
            destinationDomain: destinationDomain,
            mintRecipient: mintRecipient,
            burnToken: burnToken,
            destinationCaller: destinationCaller,
            maxFee: maxFee,
            minFinalityThreshold: minFinalityThreshold,
            hookData: hookData
        });
        ++burnCount;
    }

    function depositForBurn(
        uint256 amount,
        uint32 destinationDomain,
        bytes32 mintRecipient,
        address burnToken,
        bytes32 destinationCaller,
        uint256 maxFee,
        uint32 minFinalityThreshold
    ) external override {
        // forge-lint: disable-next-line(erc20-unchecked-transfer)
        IERC20(burnToken).transferFrom(msg.sender, address(this), amount);
        _lastBurn = Burn({
            amount: amount,
            destinationDomain: destinationDomain,
            mintRecipient: mintRecipient,
            burnToken: burnToken,
            destinationCaller: destinationCaller,
            maxFee: maxFee,
            minFinalityThreshold: minFinalityThreshold,
            hookData: ""
        });
        ++burnCount;
    }

    function lastBurn() external view returns (Burn memory) {
        return _lastBurn;
    }
}
