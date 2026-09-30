// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

/// @title A token from before the ERC20 return value was settled
/// @notice `transfer` and `transferFrom` return nothing at all.
/// @dev USDT on Ethereum mainnet is the famous one, and there are plenty of others. The router
/// reads the return data of a low level call to decide whether a payout worked, so "no return
/// data" has to mean success rather than failure. Getting that backwards would park a claim on
/// every single delivery of an asset like this, which is why it gets its own mock.
contract VoidReturnToken {
    string public name = "Void Return Token";
    string public symbol = "VOID";
    uint8 public decimals = 6;

    mapping(address holder => uint256 balance) public balanceOf;
    mapping(address holder => mapping(address spender => uint256 amount)) public allowance;

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transfer(address to, uint256 amount) external {
        _move(msg.sender, to, amount);
    }

    function transferFrom(address from, address to, uint256 amount) external {
        uint256 allowed = allowance[from][msg.sender];
        require(allowed >= amount, "allowance");
        if (allowed != type(uint256).max) allowance[from][msg.sender] = allowed - amount;
        _move(from, to, amount);
    }

    function _move(address from, address to, uint256 amount) private {
        require(balanceOf[from] >= amount, "balance");
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
    }
}
