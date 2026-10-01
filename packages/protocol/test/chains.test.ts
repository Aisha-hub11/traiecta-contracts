import { describe, expect, it } from "vitest";
import {
  CCTP_DOMAIN_NAMES,
  CHAINS,
  CHAIN_KEYS,
  MAINNET_CHAINS,
  TESTNET_CHAINS,
  addressUrl,
  cctpDomainName,
  chain,
  chainByCctpDomain,
  chainByEvmId,
  chainByKey,
  chainsFor,
  evmChainsFor,
  finalitySeconds,
  isChainKey,
  isEvmChain,
  isStellarChain,
  stellarChainFor,
  txUrl,
} from "../src/chains.js";
import { C_ADDR, G_ADDR } from "./fixtures.js";

/** Narrow to an EVM chain, so a test can read a field only that family has. */
function evm(key: Parameters<typeof chain>[0]) {
  const entry = chain(key);
  if (!isEvmChain(entry)) throw new Error(`${key} is not an EVM chain`);
  return entry;
}

describe("the registry as a whole", () => {
  it("has a key for every entry and an entry for every key", () => {
    for (const key of CHAIN_KEYS) {
      expect(chain(key).key).toBe(key);
    }
    expect(CHAIN_KEYS.length).toBe(Object.keys(CHAINS).length);
  });

  it("splits cleanly into mainnet and testnet with nothing left over", () => {
    expect([...MAINNET_CHAINS, ...TESTNET_CHAINS].sort()).toEqual([...CHAIN_KEYS].sort());
    expect(MAINNET_CHAINS.some((key) => TESTNET_CHAINS.includes(key))).toBe(false);
  });

  it("gives every chain a name, an explorer and a family", () => {
    for (const key of CHAIN_KEYS) {
      const entry = chain(key);
      expect(entry.name.length).toBeGreaterThan(0);
      expect(entry.explorer.baseUrl.startsWith("https://")).toBe(true);
      expect([isStellarChain(entry), isEvmChain(entry)].filter(Boolean).length).toBe(1);
    }
  });

  it("uses https everywhere, because an rpc url over http is a downgrade attack", () => {
    for (const key of CHAIN_KEYS) {
      const entry = chain(key);
      expect(entry.defaultRpcUrl.startsWith("https://")).toBe(true);
    }
  });

  it("recognises its own keys and nothing else", () => {
    expect(isChainKey("stellar")).toBe(true);
    expect(isChainKey("solana")).toBe(false);
    expect(chainByKey("solana")).toBeNull();
    expect(chainByKey("arc")?.key).toBe("arc");
  });

  it("refuses to pretend an unknown key is a chain", () => {
    // @ts-expect-error the point of the test is the runtime guard behind the type
    expect(() => chain("solana")).toThrow();
  });
});

describe("telling the two families apart", () => {
  it("narrows a Stellar chain to the fields only it has", () => {
    const entry = chain("stellar");
    expect(isStellarChain(entry)).toBe(true);
    if (isStellarChain(entry)) {
      expect(entry.networkPassphrase).toBe("Public Global Stellar Network ; September 2015");
      expect(entry.defaultHorizonUrl.startsWith("https://")).toBe(true);
      expect(entry.decimals).toBe(7);
    }
  });

  it("has the testnet passphrase on the testnet, which is the one people paste wrong", () => {
    const entry = chain("stellar-testnet");
    if (isStellarChain(entry)) {
      expect(entry.networkPassphrase).toBe("Test SDF Network ; September 2015");
      expect(entry.friendbotUrl).toBe("https://friendbot.stellar.org");
    }
  });

  it("narrows an EVM chain to its chain id", () => {
    const entry = chain("base");
    expect(isEvmChain(entry)).toBe(true);
    if (isEvmChain(entry)) expect(entry.chainId).toBe(8453);
  });

  it("finds exactly one Stellar chain per network", () => {
    expect(stellarChainFor("mainnet").key).toBe("stellar");
    expect(stellarChainFor("testnet").key).toBe("stellar-testnet");
  });

  it("lists only the matching network", () => {
    for (const entry of chainsFor("mainnet")) expect(entry.network).toBe("mainnet");
    for (const entry of evmChainsFor("testnet")) {
      expect(entry.network).toBe("testnet");
      expect(entry.family).toBe("evm");
    }
  });
});

describe("chain ids", () => {
  it("are unique, because a duplicate silently sends to the wrong chain", () => {
    const ids = evmChainsFor("mainnet")
      .concat(evmChainsFor("testnet"))
      .map((entry) => entry.chainId);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("look up the chain a wallet says it is on", () => {
    expect(chainByEvmId(1)?.key).toBe("ethereum");
    expect(chainByEvmId(8453)?.key).toBe("base");
    expect(chainByEvmId(84_532)?.key).toBe("base-sepolia");
    expect(chainByEvmId(999_999)).toBeNull();
  });
});

describe("CCTP domains", () => {
  it("knows Stellar is twenty seven and Arc is twenty six", () => {
    const stellar = chain("stellar");
    if (isStellarChain(stellar)) expect(stellar.cctpDomain).toBe(27);
    const arc = chain("arc");
    if (isEvmChain(arc)) expect(arc.cctpDomain).toBe(26);
  });

  it("shares a domain between a chain and its testnet, which is how Circle numbers them", () => {
    // Domain is per chain, not per network. Ethereum and Sepolia are both zero, and a lookup
    // that ignored the network would answer with whichever it found first.
    expect(chainByCctpDomain(0, "mainnet")?.key).toBe("ethereum");
    expect(chainByCctpDomain(0, "testnet")?.key).toBe("sepolia");
    expect(chainByCctpDomain(6, "mainnet")?.key).toBe("base");
    expect(chainByCctpDomain(6, "testnet")?.key).toBe("base-sepolia");
  });

  it("answers null for a domain it has no chain for", () => {
    expect(chainByCctpDomain(99, "mainnet")).toBeNull();
  });

  it("names a domain even when there is no chain entry for it", () => {
    // An indexer reading a message from a chain Hyperion does not route to still has to put
    // something on the screen.
    expect(CCTP_DOMAIN_NAMES[27]).toBe("Stellar");
    expect(CCTP_DOMAIN_NAMES[26]).toBe("Arc");
    expect(cctpDomainName(0)).toBe("Ethereum");
    expect(cctpDomainName(4_242)).toContain("4242");
  });
});

describe("Arc, which is the odd one out", () => {
  it("has USDC as its native currency", () => {
    expect(evm("arc").nativeCurrency.symbol).toBe("USDC");
  });

  it("records the eighteen against six decimals split, because that is the expensive mistake", () => {
    // Gas is accounted in eighteen decimals; the ERC-20 has six. A quote that mixed them up
    // would be out by a factor of a million.
    const arc = evm("arc");
    expect(arc.nativeCurrency.decimals).toBe(18);
    expect(arc.quirks.length).toBeGreaterThan(0);
    expect(arc.quirks.join(" ")).toMatch(/decimal/i);
  });

  it("is final in one confirmation", () => {
    const arc = evm("arc");
    expect(arc.confirmations).toBe(1);
    expect(finalitySeconds(arc)).toBeGreaterThan(0);
  });
});

describe("explorer links", () => {
  it("builds a transaction url for both families", () => {
    expect(txUrl(chain("base"), "0xabc")).toContain("0xabc");
    expect(txUrl(chain("stellar"), "abc123")).toContain("abc123");
  });

  it("sends a Soroban contract to the contract page and an account to the account page", () => {
    // Stellar Expert keeps them apart, and a C address on the account path is a dead link.
    const stellar = chain("stellar");
    expect(addressUrl(stellar, C_ADDR)).toContain("contract");
    expect(addressUrl(stellar, G_ADDR)).toContain("account");
  });

  it("builds an address url on an EVM chain", () => {
    expect(addressUrl(chain("arc"), "0x0000000000000000000000000000000000000001")).toContain(
      "0x0000000000000000000000000000000000000001",
    );
  });

  it("does not produce a double slash, which breaks some explorers", () => {
    for (const key of CHAIN_KEYS) {
      const url = txUrl(chain(key), "deadbeef");
      expect(url.slice("https://".length)).not.toContain("//");
    }
  });
});

describe("finality", () => {
  it("is a positive number of seconds everywhere", () => {
    for (const key of CHAIN_KEYS) {
      expect(finalitySeconds(chain(key))).toBeGreaterThan(0);
    }
  });
});
