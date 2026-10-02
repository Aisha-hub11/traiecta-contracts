#!/usr/bin/env bash
#
# The whole EVM deployment, start to finish, on a throwaway chain.
#
# Deploys stand in rails, runs phase one, proves the timelock refuses an early execution, warps
# past it, runs phase two, verifies the chain against the record, and sends one real transfer
# through the result. Every step fails loudly.
#
# This exists because the things that break a deployment are not syntax errors. They are an action
# queued with a field the router refuses, a lane set in the wrong order, a record whose keys do not
# match what phase two reads back, and a hook the far side cannot parse. None of those show up in a
# compile and all of them show up here, in under a minute, for free.
#
# Usage: script/anvil-e2e.sh
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="$HOME/.foundry/bin:$PATH"

RPC="${RPC:-http://127.0.0.1:8545}"
EVM_DIR="evm"

note() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
die()  { printf '\n\033[31mfailed: %s\033[0m\n' "$*" >&2; exit 1; }

for tool in forge cast anvil node; do
  command -v "$tool" >/dev/null || die "$tool is not on PATH"
done

ANVIL_PID=""
cleanup() { [ -n "$ANVIL_PID" ] && kill "$ANVIL_PID" 2>/dev/null || true; }
trap cleanup EXIT

if cast block-number --rpc-url "$RPC" >/dev/null 2>&1; then
  note "using the node already listening on $RPC"
else
  note "starting anvil"
  anvil --silent --block-time 1 > /tmp/hyperion-anvil.log 2>&1 &
  ANVIL_PID=$!
  for _ in $(seq 1 30); do
    cast block-number --rpc-url "$RPC" >/dev/null 2>&1 && break
    sleep 1
  done
  cast block-number --rpc-url "$RPC" >/dev/null 2>&1 || die "anvil did not come up"
fi

CHAIN_ID=$(cast chain-id --rpc-url "$RPC")
[ "$CHAIN_ID" = "31337" ] || [ "$CHAIN_ID" = "1337" ] || die "chain $CHAIN_ID is not a local node; this script deploys a mintable token called USDC and that belongs nowhere else"

# Anvil's first three well known accounts. Public, documented, funded only on a throwaway chain.
# They appear here and nowhere else in this repository.
SENDER=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
GUARDIAN=0x70997970C51812dc3A010C7d01b50e0d17dc79C8
TREASURY=0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC

json() { node -e 'const f=require("fs");process.stdout.write(String(JSON.parse(f.readFileSync(process.argv[1],"utf8"))[process.argv[2]]))' "$1" "$2"; }

note "deploying stand in rails"
( cd "$EVM_DIR" && mkdir -p deployments && forge script script/LocalRails.s.sol --rpc-url "$RPC" --broadcast --unlocked --sender "$SENDER" >/tmp/hyperion-rails.log 2>&1 ) \
  || { tail -30 /tmp/hyperion-rails.log; die "LocalRails"; }

RAILS="$EVM_DIR/deployments/local-rails.json"
export HYPERION_TOKEN=$(json "$RAILS" token)
export CCTP_TOKEN_MESSENGER=$(json "$RAILS" cctpTokenMessenger)
export CCTP_MESSAGE_TRANSMITTER=$(json "$RAILS" cctpMessageTransmitter)
export AXELAR_ITS=$(json "$RAILS" axelarIts)
export AXELAR_GAS_SERVICE=$(json "$RAILS" axelarGasService)
export AXELAR_TOKEN_ID=$(json "$RAILS" axelarTokenId)
echo "  token      $HYPERION_TOKEN"
echo "  messenger  $CCTP_TOKEN_MESSENGER"
echo "  its        $AXELAR_ITS"

export HYPERION_ADMIN="$SENDER"
export HYPERION_GUARDIAN="$GUARDIAN"
export HYPERION_TREASURY="$TREASURY"
export HYPERION_FEE_BPS=30
# The contract floor, so the wait below is as short as the router allows.
export HYPERION_TIMELOCK_DELAY=3600
export HYPERION_FLOW_WINDOW=3600
export HYPERION_TOKEN_DECIMALS=6
export HYPERION_TOKEN_FLOW_LIMIT=1000000000000
export HYPERION_TOKEN_SYMBOL=USDC
export HYPERION_STELLAR_CHAIN=stellar-testnet
export AXELAR_STELLAR_CHAIN=stellar
# Stands in for the Soroban router's contract id. A real run takes this from the Stellar half's
# deployment record, which is why neither side can be deployed without reference to the other.
export STELLAR_RAIL_RECIPIENT=0x0d1ea8b866708fdad0f4ed8c677cf4fe66e56a247837266036e4e0f3d05c7b10
export STELLAR_AXELAR_PEER=0x0d1ea8b866708fdad0f4ed8c677cf4fe66e56a247837266036e4e0f3d05c7b10

note "phase one: deploy and queue"
( cd "$EVM_DIR" && forge script script/Deploy.s.sol --rpc-url "$RPC" --broadcast --unlocked --sender "$SENDER" 2>&1 ) \
  | grep -E '^  (router|cctp adapter|axelar adapter|queued|Queued|record)' || die "Deploy"

note "the timelock has to refuse this"
if ( cd "$EVM_DIR" && forge script script/Execute.s.sol --rpc-url "$RPC" --unlocked --sender "$SENDER" >/tmp/hyperion-early.log 2>&1 ); then
  die "phase two succeeded before the timelock matured, which means the timelock is not doing anything"
fi
grep -E 'NOT READY|seconds remaining' /tmp/hyperion-early.log | head -2 || die "refused, but not for the reason expected"
echo "  refused, as it should"

note "warping past the timelock"
cast rpc evm_increaseTime 3700 --rpc-url "$RPC" >/dev/null
cast rpc evm_mine --rpc-url "$RPC" >/dev/null
sleep 2

note "phase two: execute"
( cd "$EVM_DIR" && forge script script/Execute.s.sol --rpc-url "$RPC" --broadcast --unlocked --sender "$SENDER" 2>&1 ) \
  | grep -E '^  (executed|already done|CCTP)' || die "Execute"

note "phase two again, to prove it is idempotent"
( cd "$EVM_DIR" && forge script script/Execute.s.sol --rpc-url "$RPC" --unlocked --sender "$SENDER" 2>&1 ) \
  | grep -E '^  (executed this run|already done)' || die "Execute is not idempotent"

note "verifying the chain against the record"
( cd "$EVM_DIR" && forge script script/Verify.s.sol --rpc-url "$RPC" --unlocked --sender "$SENDER" 2>&1 ) \
  | grep -E '^  (Everything|BAD|WARN|Mismatches)' || die "Verify"

note "sending one real transfer"
( cd "$EVM_DIR" && forge script script/Smoke.s.sol --rpc-url "$RPC" --broadcast --unlocked --sender "$SENDER" 2>&1 ) \
  | grep -E '^  (quote available|sent|fee to treasury|What the rail|The hook)|^    (amount burned|destination domain|mint recipient|hook)' || die "Smoke"

printf '\n\033[32mThe whole EVM deployment works on a local chain.\033[0m\n'
