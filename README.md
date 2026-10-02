# Hyperion contracts

The on-chain half of Hyperion: a router on Stellar, a router on the EVM side, and one adapter per
rail they route over.

Hyperion is not a bridge. It never decides for itself that a cross-chain message is real. It hands
transfers to rails that already made that decision and were audited for it, and it keeps the
bookkeeping, the limits, the fee and the claims. `bridge_in` only ever acts on a call from the
underlying rail's own verifier, and there is no path in this repository by which Hyperion attests
anything.

That is a smaller promise than most bridges make, and it is the reason the code looks the way it
does. There is no validator set here, no multisig signing messages, no light client. What there is
instead is a lot of arithmetic that has to be exactly right, four rails that each behave slightly
differently, and three implementations of the same wire formats that have to agree.

## What is in the box

```
soroban/              Rust workspace, five crates, compiled to wasm32v1-none
  hyperion-core         amounts, addresses, flow windows, strkey, codecs, CCTP parsing
  hyperion-router       the router: fees, limits, claims, timelock, adapter dispatch
  hyperion-adapter-cctp      Circle CCTP V2
  hyperion-adapter-axelar    Axelar ITS and GMP
  hyperion-adapter-allbridge Allbridge Core, outbound only and honest about it

evm/                  Foundry project, solc 0.8.28, via_ir
  src/HyperionRouter.sol        the same router, in the shape this chain wants
  src/adapters/                 CCTP and Axelar ITS
  src/libraries/                AmountMath, FlowGuard, StellarAddress, HyperionNotes, RouteMeta
  test/                         unit and fuzz suites
  script/                       two phase deployment, verification, a smoke transfer

packages/protocol/    TypeScript: the one copy of what both chains agree on
script/               deployment drivers for both halves
deployments/          deployment records, written by the scripts and validated on write
docs/                 the architecture document this was built from
```

## The test suites

| Tree | Tests | Run it with |
|---|---|---|
| Soroban | 380 | `cd soroban && cargo test --workspace` |
| EVM | 365 | `cd evm && FOUNDRY_PROFILE=ci forge test` |
| Protocol package | 300 | `cd packages/protocol && npm run check` |

All 1,045 pass. The EVM suite includes fuzz tests at twenty thousand runs each, which is why it
takes a minute and a half.

The number worth more than the total is in the protocol package: `test/parity.test.ts` reads the
Rust and the Solidity off disk and fails if the error numbering, the route tags, the address kinds,
the quote blockers, the CCTP offsets, the SEP-23 version bytes, the note layout or the fee and
decimal ceilings have drifted apart. Three implementations of one wire format is the actual risk in
this design, and that test is what turns drift into a red branch rather than a transfer that
arrives somewhere unexpected.

## Things that are easy to get wrong, and what was done about them

**A Stellar address is not a 32 byte key.** It is a string with a CRC16 over its own contents, and
a bridge is the one place where an undetected flipped bit means funds delivered to an address
nobody on earth holds a secret for. So the string travels all the way to the contract and the
checksum is verified there, before anything moves, while the money is still in the sender's wallet.
Three things catch everybody implementing SEP-23 for the first time, and all three have tests:
the checksum goes on the wire little endian, a muxed address has one spare bit in its sixty ninth
character that has to be zero, and the ed25519 key comes first with the eight byte sub account id
last.

**Stellar carries seven decimals and USDC carries six.** Every hop across that boundary either
loses a digit or refuses to. This code refuses: the fee is taken on the whole amount, the remainder
is floored to something the destination can represent, and a transfer whose remainder floors to
nothing is rejected by name rather than rounded to zero. The fee is charged against the gross and
the flow limit against the net, because the fee never leaves the chain and counting it would
tighten the limit by a number nobody picked.

**A muxed address cannot be paid on every rail.** The sub account id needs somewhere to travel, so
a rail that carries no payload cannot deliver to an M address. Allbridge is that rail. The router
refuses the combination up front rather than delivering to the underlying G account, because those
are different recipients and one of them is an exchange that will not credit anybody.

**A frozen recipient must not be able to wedge a delivery.** USDC can freeze an account, and by the
time a message is attested the counterpart is already burned on the far side, so reverting would
destroy the money rather than delay it. A delivery that cannot be handed over parks as a claim that
anybody may settle later, which needs no privileges because the rail fixed the recipient when it
signed.

**Pause stops departures and not arrivals.** Deliberately absent from `bridge_in`. Once a rail has
attested, refusing the message on this side does not undo anything; it only strands somebody's
money.

**Every configuration change waits.** There is no bootstrap exemption on either chain, which makes
a fresh deployment genuinely two phase and is covered below. An exemption would be a second code
path that configures a router without waiting, and that is the path an attacker wants and the path
nobody tests after week one.

## Deploying

Both routers put every configuration change through a timelock with a one hour floor. So a
deployment is: deploy, configure the adapters, queue the router changes, wait, execute. The scripts
are built around that rather than against it, and phase two is safe to run twice so a run
interrupted by a gas spike picks up where it left off.

Linking an adapter to its counterpart on the other chain is a third, separate step. A peer cannot be
named before it exists, and `link_domain`, `map_asset`, `link_chain` and `map_token` are all once
only on purpose, because repointing a live lane would let a different contract deliver on a chain
people are already using. That is a new trust assumption rather than a configuration change. A
guessed peer address is therefore a mistake no later transaction can undo, and the only safe order
is to deploy both sides first and link second.

```bash
# EVM, end to end on a throwaway chain. Deploys stand in rails, runs phase one, proves the
# timelock refuses an early execution, warps past it, runs phase two, checks it is idempotent,
# verifies the chain against the record, and sends one real transfer.
script/anvil-e2e.sh

# Stellar
script/stellar/build.sh                       # fails if any wasm exceeds the 64KB ledger limit
stellar keys generate hyperion-deploy --network testnet --fund
HYPERION_PEER_CHAIN=arc-testnet script/stellar/deploy.sh
script/stellar/execute.sh                     # once the hour is up
script/stellar/link.sh                        # once the far side exists
script/stellar/verify.sh
```

`anvil-e2e.sh` is the highest value check in the repository, and it is in CI for that reason. The
things that break a deployment are not syntax errors. They are an action queued with a field the
router refuses, a lane set in the wrong order, a record whose keys do not match what phase two
reads back, and a hook the far side cannot parse. None of those show up in a compile and all four
show up there, from a cold start, in under a minute.

The smoke step is worth singling out. It sends a real transfer and then reads the burn the rail
recorded, checking the mint recipient and the hook bytes, because those two values are exactly the
ones a misconfiguration gets wrong without reverting.

### Secrets

No signing key is ever read from the environment or written to a record. The Stellar scripts take a
named identity from `stellar keys`, held in the CLI's own config directory outside this repository.
The EVM scripts take whatever Foundry was given on the command line. The only account that appears
in any committed file is a public address, and the only private key in the whole tree is anvil's
first well known account, in the local end to end driver, on a chain that is deleted when the
script exits.

## Currently live

Stellar testnet, verified against `deployments/stellar-testnet-phase1.json`:

| | |
|---|---|
| Router | `CDMOLDF4SJDEDRWTDF7XAYMSRE6L3YEHHIRF57CFWNQYNC6ZOZ5LWCWF` |
| Axelar ITS adapter | `CBK3TPRQR5A3H2AWESOUX4MVYOP63VNQ5EYHCX26EEJUC5B5FLB4DV5O` |
| USDC asset contract | `CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA` |

There is no CCTP adapter on Stellar yet, and that is a gap rather than a bug. Circle deploys those
contracts per network from `circlefin/stellar-cctp` and does not publish a fixed address list, so
there is nothing safe to point an adapter at. The router answers a CCTP transfer with
`AdapterNotSet`, which is the correct answer. The Axelar adapter is deployed and initialized and
linked to nothing, for the reason in the section above.

The record is assembled by a script that hands it to the protocol package's own
`parseDeploymentSet` before writing. That is why it is a node script and not a heredoc: an address
with a transposed character parses fine as a string and survives a cast, so every contract id in
the record is checked against the real strkey codec and every rail name against the real route
list, and a malformed record never reaches disk.

## Contract sizes

Soroban has a hard 64KB ceiling per contract, enforced by the ledger rather than by politeness.

| Contract | Size | Of the limit |
|---|---|---|
| `hyperion_router.wasm` | 51.7 KB | 80% |
| `hyperion_adapter_cctp.wasm` | 25.2 KB | 39% |
| `hyperion_adapter_axelar.wasm` | 24.8 KB | 38% |
| `hyperion_adapter_allbridge.wasm` | 23.7 KB | 37% |

The router at eighty percent is worth watching. `script/stellar/build.sh` warns past two thirds and
fails past the limit, so this stops being a surprise.

## Working on it

```bash
cd soroban && cargo test --workspace
cd evm && forge build && FOUNDRY_PROFILE=ci forge test
cd packages/protocol && npm install && npm run check
```

Three things a newcomer gets wrong, in the order they will hit them.

`forge build` runs the linter and `deny = "warnings"` makes every compiler warning fatal, including
"state mutability can be restricted to view". A fuzz test that only calls view and pure helpers has
to be declared `view`; one that calls `expectRevert` or `warp` must not be.

The ABI files under `packages/protocol/src/abi` are generated from the Foundry build. Run
`npm run gen` after changing anything in `evm/src`. Editing them by hand is a failing test, because
the parity suite regenerates them in memory and compares byte for byte.

`packages/protocol/.npmrc` sets `legacy-peer-deps` and that is deliberate rather than lazy. viem
pulls `abitype`, whose peer range admits TypeScript 7 while the package pins the 5.x line
`typescript-eslint` supports, and npm's peer resolver walks that conflict into a null node and dies
before writing a single file. Every dependency there is a dev dependency, the lockfile is
committed, and the flag lives in the repository because an install flag that only exists on one
machine is a build that only works on one machine.

## Contributing

`.github/CONTRIBUTING.md` covers the toolchains. The pull request template asks what changed, which
chains and rails it touches, what new trust assumption it introduces if any, whether a committed
ABI or deployment record moved, and whether it touches funds in flight. The last two are the
questions whose answers are expensive to discover after a merge.

CI runs five checks behind one required status called `gate`. Every action is pinned to a full
commit SHA, because a floating tag in a workflow that can read secrets is a supply chain hole.

Security reports go through GitHub private advisories. `.github/SECURITY.md` says what is in scope
and, more usefully, what is not: the rails themselves are out, because Hyperion routes over them
and does not own them.

## License

Apache-2.0 for the contracts, MIT for the protocol package.
