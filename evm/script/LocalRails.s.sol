// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {Script} from "forge-std/Script.sol";
import {console} from "forge-std/console.sol";

import {MockERC20} from "../test/mocks/MockERC20.sol";
import {MockGasService} from "../test/mocks/MockGasService.sol";
import {MockIts} from "../test/mocks/MockIts.sol";
import {MockTokenMessenger} from "../test/mocks/MockTokenMessenger.sol";

/// @title Stand in rails, for a chain that has none
/// @author dotmantissa
/// @notice Deploys the mocks the test suite already uses so the real deploy scripts can be run
/// end to end on a local node. Nothing here is for a public network, and the script refuses to
/// run on one.
///
/// Worth doing rather than trusting that a deploy script compiles. The thing that breaks a
/// deployment is almost never a syntax error; it is an action queued with a field the router
/// refuses, or a lane set in the wrong order, or a record whose keys do not match what phase two
/// reads back. All three of those only show up by running the whole sequence, and a local node is
/// where that costs nothing.
///
/// Usage:
///   forge script script/LocalRails.s.sol --rpc-url http://127.0.0.1:8545 --broadcast
contract LocalRails is Script {
    /// @dev Anvil and Hardhat. Anything else and this stops, because deploying a token called
    /// USDC that anybody can mint is funny on a local node and not funny anywhere else.
    function run() external {
        if (block.chainid != 31_337 && block.chainid != 1337) {
            revert("LocalRails is for a local node only");
        }

        vm.startBroadcast();

        MockERC20 usdc = new MockERC20("USD Coin", "USDC", 6);
        MockTokenMessenger messenger = new MockTokenMessenger();
        MockIts its = new MockIts();
        MockGasService gas = new MockGasService();

        // The real adapter asks ITS whether an id belongs to this token before it stores it, so
        // the stand in has to answer, or `linkToken` refuses and the deployment stops.
        bytes32 tokenId = keccak256("hyperion.local.usdc");
        its.setRegistered(tokenId, address(usdc));

        // Enough for a person to actually try a transfer afterwards.
        usdc.mint(msg.sender, 1_000_000e6);

        vm.stopBroadcast();

        console.log("export HYPERION_TOKEN=%s", address(usdc));
        console.log("export CCTP_TOKEN_MESSENGER=%s", address(messenger));
        console.log("export CCTP_MESSAGE_TRANSMITTER=%s", address(messenger));
        console.log("export AXELAR_ITS=%s", address(its));
        console.log("export AXELAR_GAS_SERVICE=%s", address(gas));
        console.log("export AXELAR_TOKEN_ID=%s", vm.toString(tokenId));

        string memory key = "localRails";
        vm.serializeAddress(key, "token", address(usdc));
        vm.serializeAddress(key, "cctpTokenMessenger", address(messenger));
        vm.serializeAddress(key, "cctpMessageTransmitter", address(messenger));
        vm.serializeAddress(key, "axelarIts", address(its));
        vm.serializeAddress(key, "axelarGasService", address(gas));
        string memory out = vm.serializeBytes32(key, "axelarTokenId", tokenId);
        vm.writeJson(out, "deployments/local-rails.json");
        console.log("");
        console.log("record              deployments/local-rails.json");
    }
}
