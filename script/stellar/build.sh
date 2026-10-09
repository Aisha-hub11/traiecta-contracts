#!/usr/bin/env bash
#
# Build the Soroban contracts and refuse to go on if any of them is too big to deploy.
#
# The 64KB limit is a hard ledger rule, not a guideline, and a contract that clears it today can
# stop clearing it after one added branch. Checking here means that shows up as a failed build
# rather than as a failed upload after a fee has been paid.
#
# Usage: script/stellar/build.sh
source "$(dirname "$0")/lib.sh"
require_tools

# The ledger's own ceiling on a contract's wasm.
LIMIT=$((64 * 1024))
# Anything past this gets a warning. Two thirds of the ceiling is where a contract stops having
# room for the next feature, which is worth knowing before the build that fails.
WARN=$((LIMIT * 2 / 3))

note "building"
( cd "$SOROBAN_DIR" && stellar contract build >/tmp/hyperion-soroban-build.log 2>&1 ) \
  || { tail -25 /tmp/hyperion-soroban-build.log; die "build"; }

note "sizes"
problems=0
measured=0
for wasm in "$WASM_DIR"/hyperion_*.wasm; do
  [ -f "$wasm" ] || continue
  measured=$((measured + 1))
  size=$(stat -c%s "$wasm")
  name=$(basename "$wasm")
  pct=$((size * 100 / LIMIT))
  if [ "$size" -gt "$LIMIT" ]; then
    printf '   \033[31m%-40s %6.1f KB  %3d%% of the limit, too big to deploy\033[0m\n' "$name" "$(echo "$size" | awk '{print $1/1024}')" "$pct"
    problems=$((problems + 1))
  elif [ "$size" -gt "$WARN" ]; then
    printf '   \033[33m%-40s %6.1f KB  %3d%% of the limit\033[0m\n' "$name" "$(echo "$size" | awk '{print $1/1024}')" "$pct"
  else
    printf '   %-40s %6.1f KB  %3d%%\n' "$name" "$(echo "$size" | awk '{print $1/1024}')" "$pct"
  fi
done

# A glob that matches nothing leaves the loop body unentered, so without this the gate would
# report success having measured no contract at all. A crate rename, a moved target directory or
# a build that emits nothing must not turn a hard ledger rule green, so measuring nothing is
# itself a failure that names the directory it read.
if [ "$measured" -eq 0 ]; then
  die "measured no wasm in $WASM_DIR (looked for hyperion_*.wasm); refusing to pass a size gate that read nothing"
fi

[ "$problems" -eq 0 ] || die "$problems contract(s) exceed the 64KB ledger limit"
printf '\n\033[32mAll %d contracts fit.\033[0m\n' "$measured"
