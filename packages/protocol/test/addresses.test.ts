import { describe, expect, it } from "vitest";
import {
  AddressKind,
  STRKEY_LEN_MUXED,
  STRKEY_LEN_PLAIN,
  TAGGED_LEN_MUXED,
  TAGGED_LEN_PLAIN,
  VERSION_ACCOUNT,
  checksum,
  encodeStellarAddress,
  isStellarAddress,
  needsTrustline,
  parseStellarAddress,
  shortenStellarAddress,
  strkeyDestination,
  taggedDestination,
  tryParseStellarAddress,
} from "../src/addresses.js";
import { concatBytes, fromHex, toHex } from "../src/bytes.js";
import {
  BAD_ADDRESSES,
  C_ADDR,
  C_KEY,
  G_ADDR,
  G_KEY,
  MUXED_ID,
  M_ADDR,
  USDC_MAINNET_ISSUER,
  ZERO_ACCOUNT,
} from "./fixtures.js";

describe("parsing", () => {
  it("reads a classic account", () => {
    const parts = parseStellarAddress(G_ADDR);
    expect(parts.kind).toBe(AddressKind.Account);
    expect(parts.key).toBe(G_KEY);
    expect(parts.muxedId).toBe(0n);
  });

  it("reads a contract", () => {
    const parts = parseStellarAddress(C_ADDR);
    expect(parts.kind).toBe(AddressKind.Contract);
    expect(parts.key).toBe(C_KEY);
    expect(parts.muxedId).toBe(0n);
  });

  it("reads a muxed account as its base account plus an integer", () => {
    const parts = parseStellarAddress(M_ADDR);
    expect(parts.kind).toBe(AddressKind.MuxedAccount);
    expect(parts.key).toBe(G_KEY);
    expect(parts.muxedId).toBe(MUXED_ID);
  });

  it("accepts a real published issuer", () => {
    // Nothing in this repository invented this address, which is the only reason it is here.
    const parts = parseStellarAddress(USDC_MAINNET_ISSUER);
    expect(parts.kind).toBe(AddressKind.Account);
    expect(isStellarAddress(USDC_MAINNET_ISSUER)).toBe(true);
  });

  it("has the lengths SEP-23 says it has", () => {
    expect(G_ADDR.length).toBe(STRKEY_LEN_PLAIN);
    expect(C_ADDR.length).toBe(STRKEY_LEN_PLAIN);
    expect(M_ADDR.length).toBe(STRKEY_LEN_MUXED);
  });
});

describe("everything a typo can do", () => {
  for (const { value, why } of BAD_ADDRESSES) {
    it(`refuses an address that is ${why}`, () => {
      expect(() => parseStellarAddress(value)).toThrow();
      expect(isStellarAddress(value)).toBe(false);
    });
  }

  it("refuses the all zero account, which is well formed and unspendable", () => {
    expect(() => parseStellarAddress(ZERO_ACCOUNT)).toThrow(/ZeroAddressKey/);
  });

  it("reports why rather than throwing when asked nicely", () => {
    const outcome = tryParseStellarAddress("not an address");
    expect(outcome.ok).toBe(false);
    if (!outcome.ok) expect(outcome.reason.length).toBeGreaterThan(0);
  });

  it("says yes nicely too", () => {
    const outcome = tryParseStellarAddress(M_ADDR);
    expect(outcome.ok).toBe(true);
    if (outcome.ok) expect(outcome.parts.muxedId).toBe(MUXED_ID);
  });
});

describe("encoding is the exact inverse of parsing", () => {
  it("rebuilds all three kinds character for character", () => {
    expect(encodeStellarAddress(AddressKind.Account, G_KEY)).toBe(G_ADDR);
    expect(encodeStellarAddress(AddressKind.Contract, C_KEY)).toBe(C_ADDR);
    expect(encodeStellarAddress(AddressKind.MuxedAccount, G_KEY, MUXED_ID)).toBe(M_ADDR);
  });

  it("round trips a muxed id of zero, which is a real sub account and not an absence", () => {
    const encoded = encodeStellarAddress(AddressKind.MuxedAccount, G_KEY, 0n);
    expect(encoded.length).toBe(STRKEY_LEN_MUXED);
    const parts = parseStellarAddress(encoded);
    expect(parts.kind).toBe(AddressKind.MuxedAccount);
    expect(parts.muxedId).toBe(0n);
  });

  it("round trips the widest muxed id there is", () => {
    const max = (1n << 64n) - 1n;
    const parts = parseStellarAddress(encodeStellarAddress(AddressKind.MuxedAccount, G_KEY, max));
    expect(parts.muxedId).toBe(max);
  });

  it("refuses a muxed id that does not fit in eight bytes", () => {
    expect(() => encodeStellarAddress(AddressKind.MuxedAccount, G_KEY, 1n << 64n)).toThrow();
  });

  it("refuses a key that is not thirty two bytes", () => {
    expect(() => encodeStellarAddress(AddressKind.Account, "0x1234")).toThrow();
  });
});

describe("the checksum", () => {
  it("is CRC16 XModem over the version byte and the key", () => {
    // 0xB1D5 was computed outside this package. A checksum that only agrees with itself is not
    // a checksum, so this is the one number in the suite that has to come from elsewhere.
    const body = concatBytes(new Uint8Array([VERSION_ACCOUNT]), fromHex(G_KEY));
    expect(body.length).toBe(33);
    expect(checksum(body, 33)).toBe(0xb1d5);
  });

  it("is zero over nothing", () => {
    expect(checksum(new Uint8Array())).toBe(0);
  });
});

describe("the wire forms", () => {
  it("tags a plain address with its kind and nothing else", () => {
    const account = taggedDestination(parseStellarAddress(G_ADDR));
    expect(account.length).toBe(TAGGED_LEN_PLAIN);
    expect(toHex(account)).toBe(`0x00${G_KEY.slice(2)}`);

    const contractId = taggedDestination(parseStellarAddress(C_ADDR));
    expect(contractId.length).toBe(TAGGED_LEN_PLAIN);
    expect(toHex(contractId)).toBe(`0x01${C_KEY.slice(2)}`);
  });

  it("grows a muxed address by exactly eight big endian bytes", () => {
    const muxed = taggedDestination(parseStellarAddress(M_ADDR));
    expect(muxed.length).toBe(TAGGED_LEN_MUXED);
    expect(toHex(muxed)).toBe(`0x02${G_KEY.slice(2)}0020000000000001`);
  });

  it("puts the key first and the id last, which is the order everybody gets wrong", () => {
    const muxed = taggedDestination(parseStellarAddress(M_ADDR));
    expect(toHex(muxed.slice(1, 33))).toBe(G_KEY);
  });

  it("carries its own length in the strkey form", () => {
    const wire = strkeyDestination(G_ADDR);
    expect(wire.length).toBe(2 + STRKEY_LEN_PLAIN);
    expect(wire[0]).toBe(AddressKind.Account);
    expect(wire[1]).toBe(STRKEY_LEN_PLAIN);

    const muxed = strkeyDestination(M_ADDR);
    expect(muxed.length).toBe(2 + STRKEY_LEN_MUXED);
    expect(muxed[0]).toBe(AddressKind.MuxedAccount);
    expect(muxed[1]).toBe(STRKEY_LEN_MUXED);
  });

  it("refuses to put something that is not an address on the wire", () => {
    expect(() => strkeyDestination("not an address")).toThrow();
  });
});

describe("trustlines", () => {
  it("only lets a contract be paid without one", () => {
    expect(needsTrustline(AddressKind.Account)).toBe(true);
    expect(needsTrustline(AddressKind.MuxedAccount)).toBe(true);
    expect(needsTrustline(AddressKind.Contract)).toBe(false);
  });
});

describe("shortening for display", () => {
  it("keeps both ends, because that is what people check", () => {
    const short = shortenStellarAddress(G_ADDR);
    expect(short.startsWith("GA5ZYI")).toBe(true);
    expect(short.endsWith("RLVNR")).toBe(true);
    expect(short.length).toBeLessThan(G_ADDR.length);
  });

  it("leaves a short string alone rather than padding it out", () => {
    expect(shortenStellarAddress("GABC")).toBe("GABC");
  });
});
