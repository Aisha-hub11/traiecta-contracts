// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {IAxelarGasService} from "../../src/interfaces/IAxelar.sol";

/// @title Axelar's gas service, reduced to a receipt
/// @dev The only thing worth asserting about a gas top up is that the money arrived and the
/// delivery it was meant for was named correctly. Both are here.
contract MockGasService is IAxelarGasService {
    struct TopUp {
        bytes32 txHash;
        uint256 logIndex;
        address refundAddress;
        uint256 value;
    }

    TopUp private _last;
    uint256 public topUpCount;

    function addNativeGas(bytes32 txHash, uint256 logIndex, address refundAddress) external payable override {
        _last = TopUp({txHash: txHash, logIndex: logIndex, refundAddress: refundAddress, value: msg.value});
        ++topUpCount;
    }

    function lastTopUp() external view returns (TopUp memory) {
        return _last;
    }
}
