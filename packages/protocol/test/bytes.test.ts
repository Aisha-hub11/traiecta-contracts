import { describe, expect, it } from "vitest";
import {
  asciiToBytes,
  bigIntToBytes,
  bytesToAscii,
  bytesToBigInt,
  concatBytes,
  equalBytes,
  fromHex,
  slice,
  toHex,
} from "../src/bytes.js";

describe("hex", () => {
  it("round trips every byte value", () => {
    const all = new Uint8Array(256);
    for (let i = 0; i < 256; i += 1) all[i] = i;
    expect(fromHex(toHex(all))).toEqual(all);
  });

  it("pads a single digit byte", () => {
    // The bug this catches is "0x" + 5 instead of "0x" + 05, which shifts every byte after it.
    expect(toHex(new Uint8Array([0x05, 0x00, 0x0f]))).toBe("0x05000f");
  });

  it("is empty for empty", () => {
    expect(toHex(new Uint8Array())).toBe("0x");
    expect(fromHex("0x")).toEqual(new Uint8Array());
  });

  it("accepts hex with or without the prefix, and either case", () => {
    expect(fromHex("0xAbCd")).toEqual(new Uint8Array([0xab, 0xcd]));
    expect(fromHex("abcd")).toEqual(new Uint8Array([0xab, 0xcd]));
  });

  it("refuses an odd number of digits rather than guessing which end to pad", () => {
    expect(() => fromHex("0xabc")).toThrow();
  });

  it("refuses a character that is not hex", () => {
    expect(() => fromHex("0xzz")).toThrow();
  });
});

describe("big endian integers", () => {
  it("reads and writes the same value", () => {
    const value = 0x0123456789abcdefn;
    expect(bytesToBigInt(bigIntToBytes(value, 8))).toBe(value);
  });

  it("is big endian, most significant byte first", () => {
    expect(toHex(bigIntToBytes(1n, 4))).toBe("0x00000001");
    expect(bytesToBigInt(new Uint8Array([0x01, 0x00]))).toBe(256n);
  });

  it("holds a value a double cannot", () => {
    // Two to the fifty three plus one. A muxed sub account id this size is ordinary, and a
    // double cannot hold it: converting loses the low bit and credits the wrong sub account.
    const beyondDouble = 9_007_199_254_740_993n;
    expect(bytesToBigInt(bigIntToBytes(beyondDouble, 8))).toBe(beyondDouble);
    expect(BigInt(Number(beyondDouble))).toBe(9_007_199_254_740_992n);
  });

  it("refuses a value too wide for the field rather than truncating it", () => {
    expect(() => bigIntToBytes(256n, 1)).toThrow();
    expect(() => bigIntToBytes(-1n, 8)).toThrow();
  });

  it("reads an empty slice as zero", () => {
    expect(bytesToBigInt(new Uint8Array())).toBe(0n);
  });
});

describe("plumbing", () => {
  it("concatenates in order", () => {
    const joined = concatBytes(new Uint8Array([1]), new Uint8Array(), new Uint8Array([2, 3]));
    expect(joined).toEqual(new Uint8Array([1, 2, 3]));
  });

  it("compares by content and by length", () => {
    expect(equalBytes(new Uint8Array([1, 2]), new Uint8Array([1, 2]))).toBe(true);
    expect(equalBytes(new Uint8Array([1, 2]), new Uint8Array([1, 3]))).toBe(false);
    expect(equalBytes(new Uint8Array([1, 2]), new Uint8Array([1, 2, 3]))).toBe(false);
  });

  it("slices without aliasing the source, so a later write cannot reach back", () => {
    const source = new Uint8Array([1, 2, 3, 4]);
    const taken = slice(source, 1, 3);
    expect(taken).toEqual(new Uint8Array([2, 3]));
    source[1] = 99;
    expect(taken[0]).toBe(2);
  });

  it("refuses a slice past the end instead of returning a short one", () => {
    expect(() => slice(new Uint8Array([1, 2]), 0, 5)).toThrow();
  });

  it("round trips ascii", () => {
    expect(bytesToAscii(asciiToBytes("GA5ZYIIV"))).toBe("GA5ZYIIV");
  });

  it("refuses anything outside ascii, because a strkey never contains it", () => {
    expect(() => asciiToBytes("café")).toThrow();
  });
});
