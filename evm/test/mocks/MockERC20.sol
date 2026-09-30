// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {ERC20} from "openzeppelin/token/ERC20/ERC20.sol";

/// @title A token that can misbehave on purpose
/// @notice Stands in for USDC, including the part of USDC nobody enjoys.
/// @dev Circle can freeze an address, and a frozen recipient is the whole reason `bridgeIn`
/// parks a claim instead of reverting. A mock that always cooperates would never reach that
/// branch, so this one can be told to refuse a specific address, and separately to decline a
/// transfer by returning false rather than by reverting, because tokens in the wild do both.
contract MockERC20 is ERC20 {
    /// @dev Thrown the way a real freeze reads: the address that is not allowed to hold it.
    error Frozen(address account);

    uint8 private immutable DECIMALS;

    mapping(address account => bool frozen) public blocked;

    /// @dev When set, `transfer` returns false instead of moving anything.
    bool public silentRefusal;

    constructor(string memory name_, string memory symbol_, uint8 decimals_) ERC20(name_, symbol_) {
        DECIMALS = decimals_;
    }

    function decimals() public view override returns (uint8) {
        return DECIMALS;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }

    function setBlocked(address account, bool frozen) external {
        blocked[account] = frozen;
    }

    function setSilentRefusal(bool on) external {
        silentRefusal = on;
    }

    function transfer(address to, uint256 value) public override returns (bool) {
        if (silentRefusal) return false;
        return super.transfer(to, value);
    }

    function _update(address from, address to, uint256 value) internal override {
        if (blocked[from]) revert Frozen(from);
        if (blocked[to]) revert Frozen(to);
        super._update(from, to, value);
    }
}
