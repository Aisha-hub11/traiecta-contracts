import { describe, expect, it } from "vitest";
import {
  ASSETS,
  assetOn,
  assetsOn,
  isEvmAsset,
  isStellarAsset,
  pairExists,
  routesForPair,
} from "../src/registry/assets.js";
import {
  CIRCLE_FAUCET_URL,
  IRIS_API,
  UBIQUITOUS,
  railContracts,
} from "../src/registry/contracts.js";
import { CHAIN_KEYS, chain, isEvmChain, isStellarChain } from "../src/chains.js";
import { RouteKind } from "../src/routes.js";
import { isStellarAddress } from "../src/addresses.js";

describe("the asset list", () => {
  it("only names chains the registry knows", () => {
    for (const asset of ASSETS) {
      expect(CHAIN_KEYS).toContain(asset.chain);
    }
  });

  it("agrees with the chain registry about which family each one is", () => {
    for (const asset of ASSETS) {
      const entry = chain(asset.chain);
      if (isEvmAsset(asset)) expect(isEvmChain(entry)).toBe(true);
      if (isStellarAsset(asset)) expect(isStellarChain(entry)).toBe(true);
    }
  });

  it("gives every entry a source, so nothing is a number somebody remembered", () => {
    for (const asset of ASSETS) {
      expect(asset.source.length).toBeGreaterThan(0);
      expect(asset.routes.length).toBeGreaterThan(0);
    }
  });

  it("has a well formed address on every EVM entry", () => {
    for (const asset of ASSETS) {
      if (isEvmAsset(asset)) expect(asset.address).toMatch(/^0x[0-9a-fA-F]{40}$/);
    }
  });

  it("has a real issuer on every Stellar entry, and seven decimals", () => {
    for (const asset of ASSETS) {
      if (isStellarAsset(asset)) {
        expect(isStellarAddress(asset.issuer)).toBe(true);
        expect(asset.issuer.startsWith("G")).toBe(true);
        expect(asset.decimals).toBe(7);
        // Derived at deploy time from the code, the issuer and the passphrase, never typed.
        expect(asset.sacId).toBeNull();
      }
    }
  });

  it("records USDC as six decimals on Arc even though gas is eighteen", () => {
    // The single most expensive detail on that chain to get wrong.
    const arc = assetOn("arc", "USDC");
    expect(arc?.decimals).toBe(6);
    const arcChain = chain("arc");
    expect(isEvmChain(arcChain) ? arcChain.nativeCurrency.decimals : null).toBe(18);
  });

  it("never lists the same symbol twice on one chain", () => {
    const seen = new Set<string>();
    for (const asset of ASSETS) {
      const key = `${asset.chain}:${asset.symbol}`;
      expect(seen.has(key)).toBe(false);
      seen.add(key);
    }
  });

  it("looks assets up by chain and by symbol", () => {
    expect(
      assetsOn("arc")
        .map((a) => a.symbol)
        .sort(),
    ).toEqual(["EURC", "USDC"]);
    expect(assetOn("base", "USDC")?.decimals).toBe(6);
    expect(assetOn("base", "DOGE")).toBeNull();
  });
});

describe("pairing the two ends of a hop", () => {
  it("finds USDC on both ends of the route Hyperion exists for", () => {
    expect(pairExists("stellar", "arc", "USDC")).toBe(true);
    expect(pairExists("stellar", "base", "USDC")).toBe(true);
  });

  it("says no when a symbol only exists on one side", () => {
    // EURC is on Arc and not on Stellar, so there is nothing to route.
    expect(pairExists("arc", "stellar", "EURC")).toBe(false);
  });

  it("intersects the rails both ends support", () => {
    // Arc only reaches CCTP, so that is the only rail the pair has in common even though
    // Stellar supports three.
    expect(routesForPair("stellar", "arc", "USDC")).toEqual([RouteKind.Cctp]);
  });

  it("returns nothing rather than throwing for a pair that does not exist", () => {
    expect(routesForPair("arc", "stellar", "EURC")).toEqual([]);
  });

  it("offers more than one rail where both ends really do support more", () => {
    const rails = routesForPair("stellar", "base", "USDC");
    expect(rails).toContain(RouteKind.Cctp);
    expect(rails).toContain(RouteKind.AxelarIts);
  });
});

describe("the rail contracts", () => {
  it("has an entry for every EVM chain and none for Stellar", () => {
    // Stellar's CCTP addresses are not published as a fixed list, so they come from the
    // deployment record rather than from here.
    for (const key of CHAIN_KEYS) {
      const entry = railContracts(key);
      if (isEvmChain(chain(key))) {
        expect(entry).not.toBeNull();
      } else {
        expect(entry).toBeNull();
      }
    }
  });

  it("says whether each entry was read from a primary source", () => {
    for (const key of CHAIN_KEYS) {
      const entry = railContracts(key);
      if (entry === null) continue;
      expect(entry.source.length).toBeGreaterThan(0);
      expect(typeof entry.confirmed).toBe("boolean");
    }
  });

  it("marks the Arc entries confirmed and the inferred ones not", () => {
    // The distinction is the point. Shipping a plausible address as if it were verified is how
    // somebody loses money to a deploy script that looked fine.
    expect(railContracts("arc")?.confirmed).toBe(true);
    expect(railContracts("arc-testnet")?.confirmed).toBe(true);
    expect(railContracts("ethereum")?.confirmed).toBe(false);
    expect(railContracts("base")?.confirmed).toBe(false);
  });

  it("has well formed addresses throughout", () => {
    const hex = /^0x[0-9a-fA-F]{40}$/;
    for (const key of CHAIN_KEYS) {
      const entry = railContracts(key);
      if (entry === null) continue;
      if (entry.cctp !== null) {
        for (const address of Object.values(entry.cctp)) expect(address).toMatch(hex);
      }
      for (const address of Object.values(entry.common)) expect(address).toMatch(hex);
      if (entry.gateway !== null) {
        for (const address of Object.values(entry.gateway)) expect(address).toMatch(hex);
      }
    }
  });

  it("uses the same CCTP addresses on every mainnet EVM chain, which is how Circle deploys", () => {
    expect(railContracts("ethereum")?.cctp).toEqual(railContracts("base")?.cctp);
    expect(railContracts("arc")?.cctp).toEqual(railContracts("base")?.cctp);
  });

  it("keeps testnet CCTP separate from mainnet", () => {
    expect(railContracts("sepolia")?.cctp).not.toEqual(railContracts("ethereum")?.cctp);
  });

  it("only gives Arc a Gateway, because that is the only chain that has one here", () => {
    expect(railContracts("arc")?.gateway).not.toBeNull();
    expect(railContracts("base")?.gateway).toBeNull();
  });

  it("knows the three addresses that are the same on every EVM chain", () => {
    expect(UBIQUITOUS.multicall3).toBe("0xcA11bde05977b3631167028862bE2a173976CA11");
    expect(UBIQUITOUS.permit2).toBe("0x000000000022D473030F116dDEE9F6B43aC78BA3");
    expect(UBIQUITOUS.create2Factory).toBe("0x4e59b44847b379578588920cA78FbF26c0B4956C");
  });

  it("points at the right Iris for each network", () => {
    expect(IRIS_API.mainnet).toBe("https://iris-api.circle.com");
    expect(IRIS_API.testnet).toBe("https://iris-api-sandbox.circle.com");
    expect(CIRCLE_FAUCET_URL.startsWith("https://")).toBe(true);
  });
});
