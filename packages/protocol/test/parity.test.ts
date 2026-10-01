/**
 * The test that makes this package worth having.
 *
 * Everything else here checks that the TypeScript does what the TypeScript meant to do. This file
 * checks it against the contracts, by reading the Rust and the Solidity and the compiled artifacts
 * off disk. Drift between the SDK and the chain becomes a failing test rather than a support
 * ticket written by somebody whose transfer went somewhere unexpected.
 *
 * Reads sources rather than importing anything, so adding a route or an error to a contract and
 * forgetting the SDK is caught here, at the only moment it is cheap.
 */
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { toFunctionSelector } from "viem";

import { EVM_ERROR_NAMES, SOROBAN_ERROR_NAMES } from "../src/errors.js";
import {
  EVM_ERROR_SELECTORS,
  GENERATED_EVM_ERROR_NAMES,
  errorNameFromRevert,
} from "../src/abi/errors.js";
import { ROUTE_KINDS, ROUTE_SLUGS, RouteKind } from "../src/routes.js";
import { QuoteBlocker } from "../src/quotes.js";
import { AddressKind } from "../src/addresses.js";
import {
  BURN_MESSAGE_VERSION,
  FINALITY_THRESHOLD_FINALIZED,
  HOOK_VERSION,
  MESSAGE_VERSION,
  STELLAR_DOMAIN,
} from "../src/cctp.js";
import { MAX_DECIMALS, MAX_FEE_BPS } from "../src/amounts.js";
import { NOTE_VERSION, OUTBOUND_NOTE_LEN } from "../src/notes.js";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");
const evmRoot = resolve(packageRoot, "..", "..", "evm");
const sorobanRoot = resolve(packageRoot, "..", "..", "soroban");
const coreSrc = join(sorobanRoot, "crates", "hyperion-core", "src");

function read(path: string): string {
  return readFileSync(path, "utf8");
}

/** Pull `Name = <n>,` out of a Rust enum body, in declaration order. */
function rustEnumVariants(source: string, enumName: string): { name: string; value: number }[] {
  const start = source.indexOf(`pub enum ${enumName}`);
  expect(start, `${enumName} not found in the Rust source`).toBeGreaterThan(-1);
  const open = source.indexOf("{", start);
  const close = source.indexOf("\n}", open);
  const body = source.slice(open + 1, close);
  const out: { name: string; value: number }[] = [];
  for (const match of body.matchAll(/^\s*([A-Z][A-Za-z0-9]*)\s*=\s*(\d+)\s*,/gm)) {
    out.push({ name: match[1]!, value: Number(match[2]) });
  }
  expect(out.length, `no variants parsed out of ${enumName}`).toBeGreaterThan(0);
  return out;
}

/** Pull `error Name(` out of Solidity, in declaration order. */
function solidityErrors(source: string): string[] {
  return [...source.matchAll(/^error\s+([A-Za-z0-9_]+)\s*\(/gm)].map((match) => match[1]!);
}

/** Pull a Solidity enum's members, in declaration order. */
function solidityEnum(source: string, enumName: string): string[] {
  const start = source.indexOf(`enum ${enumName}`);
  expect(start, `${enumName} not found in the Solidity source`).toBeGreaterThan(-1);
  const open = source.indexOf("{", start);
  const close = source.indexOf("}", open);
  return source
    .slice(open + 1, close)
    .split(",")
    .map((line) => line.replace(/\/\/.*$/gm, "").trim())
    .filter((line) => line.length > 0);
}

describe("the error vocabularies", () => {
  it("matches the Soroban enum name for name and number for number", () => {
    // Index aligned and append only. Reordering `HyperionError` would relabel every failure the
    // Stellar side has ever emitted, each one plausibly, which is the worst kind of wrong.
    const variants = rustEnumVariants(read(join(coreSrc, "error.rs")), "HyperionError");
    expect(SOROBAN_ERROR_NAMES.length).toBe(variants.length);
    variants.forEach((variant, index) => {
      expect(SOROBAN_ERROR_NAMES[index]).toBe(variant.name);
      expect(variant.value).toBe(index + 1);
    });
  });

  it("matches the Solidity declaration order", () => {
    const declared = solidityErrors(read(join(evmRoot, "src", "HyperionErrors.sol")));
    expect([...EVM_ERROR_NAMES]).toEqual(declared);
  });

  it("matches what the generator last wrote, so the committed ABI is not stale", () => {
    expect([...EVM_ERROR_NAMES]).toEqual([...GENERATED_EVM_ERROR_NAMES]);
  });

  it("has a selector for every declared error that a contract can actually revert with", () => {
    // Computed from the signature rather than copied from anywhere, and checked against the
    // generated table, so a selector nobody typed cannot be a selector nobody noticed.
    const known = new Map(
      Object.entries(EVM_ERROR_SELECTORS).map(([selector, meta]) => [meta.signature, selector]),
    );
    for (const [signature, selector] of known) {
      expect(toFunctionSelector(signature)).toBe(selector);
    }
  });

  it("names an error from revert data the way a wallet hands it over", () => {
    const paused = toFunctionSelector("AdapterNotSet()");
    expect(errorNameFromRevert(paused)).toBe("AdapterNotSet");
    expect(errorNameFromRevert(`${paused}${"00".repeat(32)}`)).toBe("AdapterNotSet");
    expect(errorNameFromRevert("0x")).toBeNull();
    expect(errorNameFromRevert("0xdeadbeef")).toBeNull();
  });

  it("does not claim an EVM Paused, because OpenZeppelin supplies that one", () => {
    const declared = solidityErrors(read(join(evmRoot, "src", "HyperionErrors.sol")));
    expect(declared).not.toContain("Paused");
  });
});

describe("the route tags", () => {
  it("matches the Rust enum", () => {
    const variants = rustEnumVariants(read(join(coreSrc, "route.rs")), "RouteKind");
    expect(variants.map((v) => v.name)).toEqual(["Cctp", "AxelarIts", "AxelarGmp", "Allbridge"]);
    variants.forEach((variant, index) => {
      expect(variant.value).toBe(index);
      expect(ROUTE_KINDS[index]).toBe(variant.value);
    });
  });

  it("matches the Solidity enum, which carries the same integers", () => {
    const members = solidityEnum(read(join(evmRoot, "src", "HyperionTypes.sol")), "RouteKind");
    expect(members).toEqual(["Cctp", "AxelarIts", "AxelarGmp", "Allbridge"]);
    expect(members.length).toBe(ROUTE_KINDS.length);
    expect(members.indexOf("Allbridge")).toBe(RouteKind.Allbridge);
  });

  it("has a slug for every rail and no duplicates, because slugs end up in urls", () => {
    const slugs = ROUTE_KINDS.map((kind) => ROUTE_SLUGS[kind]);
    expect(new Set(slugs).size).toBe(slugs.length);
  });
});

describe("the address kinds", () => {
  it("matches the Solidity enum", () => {
    const members = solidityEnum(read(join(evmRoot, "src", "HyperionTypes.sol")), "AddressKind");
    expect(members).toEqual(["Account", "Contract", "MuxedAccount"]);
    expect(members.indexOf("Account")).toBe(AddressKind.Account);
    expect(members.indexOf("Contract")).toBe(AddressKind.Contract);
    expect(members.indexOf("MuxedAccount")).toBe(AddressKind.MuxedAccount);
  });

  it("matches the Rust enum", () => {
    const source = read(join(coreSrc, "address.rs"));
    const variants = rustEnumVariants(source, "AddressKind");
    expect(variants.map((v) => v.name)).toEqual(["Account", "Contract", "MuxedAccount"]);
    variants.forEach((variant, index) => {
      expect(variant.value).toBe(index);
    });
  });
});

describe("the quote blockers", () => {
  it("matches the Solidity enum, in order", () => {
    // These travel in a struct an app reads over eth_call, so a reordering would relabel every
    // refusal on the screen.
    const members = solidityEnum(read(join(evmRoot, "src", "HyperionTypes.sol")), "QuoteBlocker");
    const mine = Object.entries(QuoteBlocker)
      .sort(([, a], [, b]) => a - b)
      .map(([name]) => name);
    expect(mine).toEqual(members);
  });
});

describe("the constants both chains agree on", () => {
  it("has the same SEP-23 version bytes in the Rust", () => {
    const source = read(join(coreSrc, "codec.rs"));
    expect(source).toMatch(/VERSION_ACCOUNT[^=]*=\s*6\s*<<\s*3/);
    expect(source).toMatch(/VERSION_CONTRACT[^=]*=\s*2\s*<<\s*3/);
    expect(source).toMatch(/VERSION_MUXED[^=]*=\s*12\s*<<\s*3/);
  });

  it("has the same CCTP offsets and constants in the Rust", () => {
    const source = read(join(coreSrc, "cctp.rs"));
    const expectConst = (name: string, value: number): void => {
      const match = new RegExp(`${name}\\s*:\\s*[a-z0-9]+\\s*=\\s*(\\d+)`).exec(source);
      expect(match, `${name} not found in cctp.rs`).not.toBeNull();
      expect(Number(match?.[1])).toBe(value);
    };
    expectConst("STELLAR_DOMAIN", STELLAR_DOMAIN);
    expectConst("MESSAGE_VERSION", MESSAGE_VERSION);
    expectConst("BURN_MESSAGE_VERSION", BURN_MESSAGE_VERSION);
    expectConst("FINALITY_THRESHOLD_FINALIZED", FINALITY_THRESHOLD_FINALIZED);
    expectConst("HOOK_VERSION", HOOK_VERSION);
    expectConst("MSG_BODY", 148);
    expectConst("BURN_HOOK_DATA", 228);
  });

  it("has the same note layout in the Rust", () => {
    const source = read(join(coreSrc, "axelar.rs"));
    expect(source).toMatch(new RegExp(`NOTE_VERSION\\s*:\\s*u8\\s*=\\s*${String(NOTE_VERSION)}`));
    expect(source).toMatch(
      new RegExp(`OUTBOUND_NOTE_LEN\\s*:\\s*u32\\s*=\\s*${String(OUTBOUND_NOTE_LEN)}`),
    );
  });

  it("has the same fee and decimal ceilings in the Solidity", () => {
    const source = read(join(evmRoot, "src", "libraries", "AmountMath.sol"));
    expect(source).toMatch(new RegExp(`MAX_DECIMALS\\s*=\\s*${String(MAX_DECIMALS)}`));
    expect(source).toMatch(new RegExp(`MAX_FEE_BPS\\s*=\\s*${String(MAX_FEE_BPS)}`));
    expect(source).toMatch(/BPS_DENOMINATOR\s*=\s*10_?000/);
  });

  it("has the same ceilings in the Rust", () => {
    // `MAX_DECIMALS` lives with the arithmetic; the fee cap and the basis point denominator sit
    // in the crate root, because the router needs them and the amount module only enforces them.
    expect(read(join(coreSrc, "amount.rs"))).toMatch(
      new RegExp(`pub const MAX_DECIMALS\\s*:\\s*u32\\s*=\\s*${String(MAX_DECIMALS)}`),
    );
    const root = read(join(coreSrc, "lib.rs"));
    expect(root).toMatch(
      new RegExp(`pub const MAX_FEE_BPS\\s*:\\s*u32\\s*=\\s*${String(MAX_FEE_BPS)}`),
    );
    expect(root).toMatch(/pub const BPS_DENOMINATOR\s*:\s*i128\s*=\s*10_?000/);
  });
});

describe("the generated ABI modules", () => {
  it("is exactly what the generator produces right now", () => {
    // Regenerate and compare, rather than trusting that somebody ran it. Editing a generated
    // file by hand, or changing a contract and forgetting, both land here.
    const before = snapshotAbiDir();
    execFileSync(process.execPath, [join(packageRoot, "scripts", "gen-artifacts.mjs")], {
      cwd: packageRoot,
      stdio: "pipe",
    });
    expect(snapshotAbiDir()).toEqual(before);
  });

  it("exports an ABI for every contract the app has to talk to", () => {
    const abiDir = join(packageRoot, "src", "abi");
    for (const name of [
      "router",
      "cctpAdapter",
      "axelarItsAdapter",
      "railAdapter",
      "erc20",
      "errors",
    ]) {
      expect(existsSync(join(abiDir, `${name}.ts`))).toBe(true);
    }
  });
});

function snapshotAbiDir(): Record<string, string> {
  const abiDir = join(packageRoot, "src", "abi");
  const out: Record<string, string> = {};
  for (const name of readdirSync(abiDir).sort()) {
    if (name.endsWith(".ts")) out[name] = read(join(abiDir, name));
  }
  return out;
}
