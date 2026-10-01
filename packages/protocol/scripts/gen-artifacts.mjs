#!/usr/bin/env node
/**
 * Turns Foundry's build output into the ABI modules this package exports.
 *
 * Why generate rather than hand write: an ABI typed out by a person is a copy of the truth that
 * starts drifting the moment somebody adds an argument, and the drift shows up as a decode that
 * silently returns the wrong field rather than as an error. Generating it means the only way the
 * SDK can disagree with the contracts is if somebody forgets to run this, and `parity.test.ts`
 * regenerates in memory and compares, so forgetting is a failing test.
 *
 * Deliberately emits nothing that changes between runs. No timestamp, no commit hash, no build
 * id. A generated file with a clock in it is a file that shows up in every diff and makes the
 * parity check impossible to write.
 *
 * Usage: npm run gen
 */
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");
const evmRoot = resolve(packageRoot, "..", "..", "evm");
const artifactRoot = join(evmRoot, "out");
const abiDir = join(packageRoot, "src", "abi");

/**
 * The artifacts worth exporting, and the module each one lands in.
 *
 * An allow list rather than a sweep of `out/`, because that directory also holds forge-std, the
 * OpenZeppelin tree, every test contract and every mock. Shipping an SDK that exports the ABI of
 * `HostileCallers` would be funny exactly once.
 */
const TARGETS = [
  {
    contract: "HyperionRouter",
    file: "HyperionRouter.sol",
    module: "router",
    symbol: "hyperionRouterAbi",
  },
  {
    contract: "IHyperionRouter",
    file: "IHyperionRouter.sol",
    module: "routerInterface",
    symbol: "hyperionRouterInterfaceAbi",
  },
  {
    contract: "CctpAdapter",
    file: "CctpAdapter.sol",
    module: "cctpAdapter",
    symbol: "cctpAdapterAbi",
  },
  {
    contract: "AxelarItsAdapter",
    file: "AxelarItsAdapter.sol",
    module: "axelarItsAdapter",
    symbol: "axelarItsAdapterAbi",
  },
  {
    contract: "IRailAdapter",
    file: "IRailAdapter.sol",
    module: "railAdapter",
    symbol: "railAdapterAbi",
  },
  { contract: "IERC20", file: "IERC20.sol", module: "erc20", symbol: "erc20Abi" },
];

/** Where the free standing error declarations live, in the order a reviewer reads them. */
const ERRORS_SOURCE = join(evmRoot, "src", "HyperionErrors.sol");

function readArtifact(file, contract) {
  const path = join(artifactRoot, file, `${contract}.json`);
  let raw;
  try {
    raw = readFileSync(path, "utf8");
  } catch {
    throw new Error(
      `missing artifact ${path}\nRun \`forge build\` in ${evmRoot} first; this script reads what the compiler produced rather than guessing.`,
    );
  }
  const parsed = JSON.parse(raw);
  if (!Array.isArray(parsed.abi)) throw new Error(`artifact ${path} has no abi array`);
  return parsed.abi;
}

/**
 * Sort an ABI into a stable order.
 *
 * solc's ordering is stable in practice but it is not promised anywhere, and a generated file
 * that reshuffles itself produces a diff nobody can review. Sorting by kind then by signature
 * means the output only moves when the interface actually moves.
 */
function canonicalise(abi) {
  const kindRank = { constructor: 0, receive: 1, fallback: 2, function: 3, event: 4, error: 5 };
  return [...abi].sort((a, b) => {
    const byKind = (kindRank[a.type] ?? 9) - (kindRank[b.type] ?? 9);
    if (byKind !== 0) return byKind;
    return signatureOf(a).localeCompare(signatureOf(b));
  });
}

function typeOf(input) {
  if (input.type.startsWith("tuple")) {
    const inner = (input.components ?? []).map(typeOf).join(",");
    return `(${inner})${input.type.slice("tuple".length)}`;
  }
  return input.type;
}

function signatureOf(entry) {
  const name = entry.name ?? entry.type;
  const inputs = (entry.inputs ?? []).map(typeOf).join(",");
  return `${name}(${inputs})`;
}

/**
 * Error names in the order they are declared in `HyperionErrors.sol`.
 *
 * Read off the source rather than off an ABI on purpose. An ABI is a set and tells you nothing
 * about order, and the order is the thing `errors.ts` has to agree with.
 */
function readErrorNames() {
  const source = readFileSync(ERRORS_SOURCE, "utf8");
  const names = [];
  for (const match of source.matchAll(/^error\s+([A-Za-z0-9_]+)\s*\(/gm)) {
    names.push(match[1]);
  }
  if (names.length === 0) throw new Error(`found no error declarations in ${ERRORS_SOURCE}`);
  const duplicates = names.filter((name, index) => names.indexOf(name) !== index);
  if (duplicates.length > 0) {
    throw new Error(`duplicate error declarations: ${duplicates.join(", ")}`);
  }
  return names;
}

/**
 * Every error signature the compiled contracts carry, with its selector.
 *
 * Selectors come from `cast`-free arithmetic: keccak256 over the canonical signature, first four
 * bytes. viem supplies the hash so this script does not ship its own keccak, and the parity test
 * checks the result against the same artifacts, so a wrong selector cannot survive both.
 */
async function collectErrorSelectors(abis) {
  const { toFunctionSelector } = await import("viem");
  const bySelector = new Map();
  for (const abi of abis) {
    for (const entry of abi) {
      if (entry.type !== "error") continue;
      const signature = signatureOf(entry);
      const selector = toFunctionSelector(signature);
      const existing = bySelector.get(selector);
      if (existing !== undefined && existing.signature !== signature) {
        throw new Error(
          `selector collision on ${selector}: ${existing.signature} and ${signature}. One of them has to be renamed.`,
        );
      }
      bySelector.set(selector, { signature, name: entry.name });
    }
  }
  return [...bySelector.entries()].sort((a, b) => a[0].localeCompare(b[0]));
}

const BANNER = `// Generated by scripts/gen-artifacts.mjs from the Foundry build. Do not edit by hand.
//
// Run \`npm run gen\` after changing anything in evm/src. The parity test regenerates this in
// memory and fails if the committed copy has drifted, so an edit here is a test failure rather
// than a quiet disagreement between the SDK and the chain.
`;

function emitAbiModule(target, abi) {
  const body = JSON.stringify(canonicalise(abi), null, 2);
  return `${BANNER}
/** The compiled ABI of \`${target.contract}\`. */
export const ${target.symbol} = ${body} as const;
`;
}

function emitErrorsModule(errorNames, selectors) {
  const nameLines = errorNames.map((name) => `  "${name}",`).join("\n");
  const selectorLines = selectors
    .map(
      ([selector, { signature, name }]) =>
        `  "${selector}": { name: "${name}", signature: "${signature}" },`,
    )
    .join("\n");

  return `${BANNER}
/**
 * Every error \`HyperionErrors.sol\` declares, in declaration order.
 *
 * The order is load bearing: \`errors.ts\` keeps the same list by hand so the human readable help
 * can live next to it, and the parity test compares the two.
 */
export const GENERATED_EVM_ERROR_NAMES = [
${nameLines}
] as const;

/** Four byte selector to error, for every error the compiled contracts can actually revert with. */
export const EVM_ERROR_SELECTORS = {
${selectorLines}
} as const;

export type EvmErrorSelector = keyof typeof EVM_ERROR_SELECTORS;

/**
 * Name an error from the first four bytes of revert data.
 *
 * Returns null rather than throwing for anything it does not recognise, because unrecognised
 * revert data is the normal case when a call lands in a token or a rail rather than in Hyperion.
 */
export function errorNameFromSelector(selector: string): string | null {
  const lowered = selector.toLowerCase();
  const hit = (EVM_ERROR_SELECTORS as Record<string, { name: string } | undefined>)[lowered];
  return hit?.name ?? null;
}

/** Pull the selector off revert data and name it, when there is enough data to have one. */
export function errorNameFromRevert(data: string): string | null {
  if (!data.startsWith("0x") || data.length < 10) return null;
  return errorNameFromSelector(data.slice(0, 10));
}
`;
}

function emitIndexModule(targets) {
  const lines = targets.map((target) => `export * from "./${target.module}.js";`).join("\n");
  return `${BANNER}
${lines}
export * from "./errors.js";
`;
}

async function main() {
  mkdirSync(abiDir, { recursive: true });

  const loaded = TARGETS.map((target) => ({
    target,
    abi: readArtifact(target.file, target.contract),
  }));

  const written = [];
  for (const { target, abi } of loaded) {
    const path = join(abiDir, `${target.module}.ts`);
    writeFileSync(path, emitAbiModule(target, abi));
    written.push(`${target.module}.ts`);
  }

  const errorNames = readErrorNames();
  const selectors = await collectErrorSelectors(loaded.map(({ abi }) => abi));
  writeFileSync(join(abiDir, "errors.ts"), emitErrorsModule(errorNames, selectors));
  written.push("errors.ts");

  writeFileSync(join(abiDir, "index.ts"), emitIndexModule(TARGETS));
  written.push("index.ts");

  // Anything left over is a module from an earlier run whose target has since been removed. Say
  // so rather than deleting it, because a stray file is a two second fix and a script that
  // deletes things in src is a bad habit to install.
  const expected = new Set(written);
  const stale = readdirSync(abiDir).filter((name) => name.endsWith(".ts") && !expected.has(name));

  process.stdout.write(
    `wrote ${String(written.length)} modules to src/abi: ${written.join(", ")}\n`,
  );
  process.stdout.write(
    `${String(errorNames.length)} declared errors, ${String(selectors.length)} distinct selectors across ${String(loaded.length)} artifacts\n`,
  );
  if (stale.length > 0) {
    process.stdout.write(`note: src/abi holds files this run did not write: ${stale.join(", ")}\n`);
  }
}

await main();
