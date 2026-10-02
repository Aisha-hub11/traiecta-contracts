# Security policy

Hyperion routes stablecoin transfers between Stellar and EVM chains. It sits in front of money in
motion, so the difference between a quiet fix and a public one is measured in whatever was in
flight at the time. This page says how to tell us, what we will look at, and what we will not.

## Supported versions

| Revision | Supported | Notes |
| --- | --- | --- |
| `main` | Yes | The only supported revision today. |
| Tagged releases | Not yet | There are none. |
| Anything deployed to a public mainnet | Not applicable yet | There is no mainnet deployment. |

This is early software. `main` is the whole product, the version in `Cargo.toml` and
`package.json` is `0.1.0`, and the only deployment records in the tree describe a local chain.
When a real deployment happens, this table will list the revision each network is running and that
becomes the thing to report against. Until then, report against `main` and say which commit.

A note on what that means for you: because nothing is live, a report now is cheap for everybody
and extremely useful. The best time to find a bug in a two phase timelocked deployment is before
anybody has signed one.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting:

**https://github.com/StellarHyperion/stellarhyperion-contracts/security/advisories/new**

It is visible only to the maintainers, it needs no account beyond the GitHub one you already have,
and it gives us a private place to work on a fix and a draft advisory in the same thread.

There is deliberately no email address on this page. An address in a security policy that nobody
is actually watching is worse than no address, because it absorbs a report and returns silence,
and the reporter reasonably concludes nobody cares and moves on. GitHub advisories notify the
maintainers through a channel that is already watched for other reasons. If you cannot use GitHub
for some reason, open a public issue that says only that you have something private to share, with
no detail, and ask for a channel.

Please do not open a public issue, publish a proof of concept, or post the details in a thread
before there is a fix. The router has a timelock on its admin path, which means even a fix that is
written in an hour is not deployed in an hour.

### What makes a report easy to act on

None of this is required. All of it shortens the time to a fix.

- The commit sha you looked at.
- Which chain and which rail, if it is specific to either.
- What an attacker gets: funds, a denial of service, a bypassed limit, a replayed delivery, an
  incorrect credit. The impact decides the priority more than the cleverness does.
- The preconditions. Does it need an admin key, a specific rail to be configured a specific way, a
  particular token's decimals, a particular ordering.
- A failing test, if you have one. `cargo test` for the Stellar side, `forge test` for the EVM
  side. A reproduction we can run is worth several paragraphs of description.

## What is in scope

The code in this repository, which is to say the parts Hyperion wrote and can fix:

- **The routers.** `evm/src/HyperionRouter.sol` and `soroban/crates/hyperion-router`. Routing
  decisions, access control, the timelock, pausing, fee collection, the flow guard.
- **The rail adapters.** `evm/src/adapters/` and `soroban/crates/hyperion-adapter-*`. Everything
  about how a transfer is handed to a rail and how a delivery is accepted back, including replay
  protection and how an adapter decides a caller is who it claims to be.
- **The shared codecs and the arithmetic.** `evm/src/libraries/` and
  `soroban/crates/hyperion-core`. Strkey parsing and encoding, the note layout, CCTP message
  reading, decimal conversion between tokens of different precision, fee arithmetic, flow window
  decay. A disagreement between the two chains' implementations of any of these is in scope even
  if neither half is wrong on its own, because the disagreement is the bug.
- **The protocol package.** `packages/protocol`. It is the copy of those same facts that
  applications use, so a codec here that disagrees with the contracts can send money to a correctly
  formatted wrong address.
- **The deployment scripts.** `evm/script/` and `script/`. A script that queues the wrong action,
  writes a record phase two misreads, or leaks a key is a security problem even though it is not a
  contract.
- **These workflows.** `.github/workflows/`. A workflow that can be made to run attacker
  controlled code, or that would expose a secret if one were ever added, is in scope.

## What is out of scope

Being specific about this is not a way of dodging reports. It is the central design fact about
Hyperion, and a good fraction of reports land here.

**Hyperion is a router, not a bridge. It does not own the rails it routes over.** It never decides
on its own that a cross chain message is real. It hands transfers to rails that already made that
decision and were audited for it, and it keeps the bookkeeping, the limits and the fees. So:

- **The rails themselves.** Circle's CCTP, Axelar's Interchain Token Service, Axelar's General
  Message Passing, Allbridge Core. Their contracts, their validator sets, their attestation
  services, their relayers, their liquidity pools, their upgrade keys. If CCTP's attestation
  service signs something it should not have, Hyperion will route it and there is nothing in this
  repository that could have stopped it. Report those to the rail. We will happily help you find
  the right contact, and if a rail's behaviour means Hyperion should stop trusting it for
  something, that part is in scope.
- **The underlying chains.** Stellar, Ethereum, Base, Arc. Consensus, the VM, fee markets, reorgs
  beyond the finality each chain documents.
- **The committed dependencies.** forge-std and OpenZeppelin, pinned as submodules under
  `evm/lib`, and the crates in `soroban/Cargo.lock`. Report those upstream. Do tell us if we are
  pinned to a version with a published advisory, which is a real finding about this repository and
  is covered by the public security form rather than the private one.
- **Token behaviour we already refuse.** The suite includes mocks for tokens that return nothing
  instead of a bool and for callers that behave badly on purpose. If you find a token behaviour
  that is not handled, that is in scope. If you find that a deliberately hostile mock behaves
  hostilely, that is the test working.
- **Anvil's published test accounts**, which appear in `script/anvil-e2e.sh`. They are funded only
  on a throwaway chain and are documented by Foundry.
- **Testnet funds.** Worth reporting if it reveals a real bug. Not worth reporting as a loss.
- **Findings with no impact behind them.** A static analysis warning, a missing event, a gas
  inefficiency, a theoretical reentrancy on a function that moves nothing. Send these as a normal
  issue or a pull request. They are welcome, they are just not an advisory.
- **Social engineering, physical access, and anything about a maintainer's own machine or
  accounts.**

## What to expect

Honest numbers for a small project rather than numbers that sound reassuring:

| Stage | Expect |
| --- | --- |
| Acknowledgement that a human read it | Within 3 working days |
| A first assessment: in scope or not, and a severity | Within 10 working days |
| A fix for something that can move or freeze funds | Worked on immediately, deployed as fast as the timelock allows |
| A fix for everything else | Scheduled, and you will be told roughly when |

If you have heard nothing after 5 working days, assume the notification was missed rather than
ignored, and nudge the thread. That is a reasonable thing to do and nobody will be annoyed.

There is no bug bounty. If one is ever funded this page will say so, with the terms, before anybody
is asked to rely on it. Promising a reward with no budget behind it wastes the time of the people
most worth keeping on side.

## Disclosure

We would rather fix first and publish after, and we would rather publish than not.

- We will work with you on a date, and we will not ask you to sit on something indefinitely. If a
  fix is taking us a long time, that is our problem to explain, not a reason to extend your
  silence.
- When it is fixed, a GitHub advisory goes out with the details, the affected revisions and what to
  do about it. You are credited by whatever name or handle you ask for, or not at all if you prefer.
- If a report turns out to be in scope but not exploitable, we will say so and say why, and the
  reasoning goes into a comment next to the code so the next person does not have to work it out
  again. That has value and it gets credited too.
