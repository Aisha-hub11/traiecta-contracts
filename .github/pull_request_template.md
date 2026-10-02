<!--
Every question below is here because getting it wrong once is expensive, and most of them are
cheap to answer while the change is fresh in your head and nearly impossible to reconstruct three
weeks later.

Do not delete sections. If one does not apply, write "not applicable" and one line saying why.
That is a real answer and a reviewer can check it. A deleted section just looks like an oversight.
-->

## What changed

<!--
Plain prose. What is different in the code after this change than before it, at the level a
reviewer who has not read the diff can follow. File names are not an answer; "the router now
rejects a muxed destination on Allbridge instead of dropping the sub account id" is.
-->

## Why

<!--
What was wrong, or what became possible. If this fixes something, say how it was found: a failing
test, a fuzz counterexample, a review comment, something observed on testnet. If it is new
behaviour, say who asked for it and what they could not do before.

If the honest answer is "cleanup", that is fine. Say so and keep the diff to cleanup.
-->

## Chains and rails

<!--
Tick what this change can affect, not what it was aimed at. A change to a shared library affects
every rail that calls it, and a change to the amount maths affects every chain.
-->

Chains:

- [ ] Stellar mainnet
- [ ] Stellar testnet
- [ ] Ethereum
- [ ] Ethereum Sepolia
- [ ] Base
- [ ] Base Sepolia
- [ ] Arc
- [ ] Arc testnet
- [ ] None. This touches no chain specific behaviour.

Rails:

- [ ] Circle CCTP V2
- [ ] Axelar Interchain Token Service
- [ ] Axelar General Message Passing
- [ ] Allbridge Core
- [ ] None. This is rail agnostic.

<!--
If you ticked more than one rail, say in a sentence what they have in common here, because that is
usually the shared code path a reviewer needs to look at hardest.
-->

## New trust assumptions

<!--
The single most important question in this template, and the one a diff cannot answer on its own.

Hyperion is a router, not a bridge. It never decides that a cross chain message is real. It hands
transfers to rails that already made that decision and were audited for it, and it keeps the
bookkeeping, the limits and the fees. Every line that widens that remit is a line that moves
Hyperion from "routes over trusted things" towards "is a trusted thing", and that is a decision,
not an implementation detail.

So: after this change, is there anybody or anything new that has to behave correctly for a user's
funds to be safe? A new privileged role, a new address read from configuration rather than
derived, a new external call, a rail peer that is now believed about something it was not believed
about before, an admin function that can now do something it could not, a new assumption about
message ordering or about a timestamp.

If the answer is no, write "none" and say what makes you confident. If the answer is yes, name the
party, say what they can do, and say what happens to a transfer in flight if they do it.
-->

## Funds in flight

<!--
A transfer that has left one chain and has not arrived on the other is in a state no single chain
can see. Changes that alter how that state is keyed, recognised or accounted for can orphan money
that is already moving, and the people affected are the ones who sent before the deploy.

Does this change touch any of:

- [ ] A replay key, a message id, or anything used to recognise a delivery as already done
- [ ] The note layout, or anything about how a payload is written or parsed
- [ ] Decimal conversion, fee arithmetic, or rounding
- [ ] The flow guard, a flow window, or a limit
- [ ] An inbound handler, or the order in which a delivery is accepted and credited
- [ ] The timelock, or the two phase deployment
- [ ] None of the above

If you ticked anything other than "none of the above", describe what happens to a transfer that
was already in flight at the moment this change is deployed. "Nothing, because the key derivation
is unchanged for existing messages" is a good answer. "I had not thought about it" is a useful
answer too, and better said now than found later.
-->

## Generated and committed artifacts

<!--
Two things in this repository are generated and committed, which means they can disagree with
their source and the disagreement is silent.
-->

- [ ] `packages/protocol/src/abi` changed, and it changed because I ran `npm run gen` after a
      `forge build`, not because I edited it
- [ ] A deployment record under `deployments/` or `evm/deployments/` changed, and I have said below
      which network and why
- [ ] Neither changed

<!--
If the ABI moved, say which contract's interface moved and whether anything consuming it needs to
change. If a deployment record moved, say which network, which addresses, and whether the record
now describes something that exists on chain.
-->

## How this was tested

<!--
Not "tests pass". Which tests, and what would have failed before.

Say what you added. A change to the contracts with no new test is a change that nothing stops
somebody reverting by accident next month. If you did not add one, say why not.

Worth stating explicitly if you ran any of these, because CI runs all of them and a local run is
how you find out faster:

    cd soroban          && cargo test --workspace
    cd evm              && FOUNDRY_PROFILE=ci forge test
    cd packages/protocol && npm run check
    script/anvil-e2e.sh
    script/stellar/build.sh

If you tested on a live testnet, give the network, the contract ids or addresses, and a
transaction hash somebody else can look up.
-->

## What reviewers will look at

A reviewer on this repository is going to go through roughly this list, so it is worth reading it
before you ask for review rather than after.

- **The trust section above, first.** If it says "none" and the diff adds an external call, the
  review stops there.
- **Whether the two chains still agree.** The Rust and the Solidity encode the same route tags,
  the same error numbers, the same address kinds and the same note layout, and
  `packages/protocol/test/parity.test.ts` is what checks it. A change on one side with no change
  on the other is either a bug or a deliberate break that needs saying out loud.
- **Error numbering and route tags.** Appending is fine. Reordering relabels every failure ever
  emitted, plausibly, which is the worst kind of wrong.
- **Arithmetic, in detail.** Fees, decimal conversion between a six decimal token and an
  eighteen decimal one, flow window decay. Reviewers will look for the rounding direction and ask
  who it favours.
- **What happens on the unhappy path.** A rail that reverts, a token that returns nothing instead
  of a bool, a delivery that arrives twice, a payload that is shorter than it claims. The mocks
  under `evm/test/mocks` exist because each of these happened to somebody.
- **Admin surface.** Anything a privileged role can newly do, and whether the timelock covers it.
- **Gas, but last.** The gas table in CI is for reading, not for passing. A reviewer will ask about
  a large movement and will not ask about a small one.

## Anything else

<!--
Open questions, things you are unsure about, parts of the diff you would like a second opinion on.
A pull request that names its own weak spot gets a better review than one that does not.
-->
