import { describe, expect, it } from "vitest";
import {
  BPS_DENOMINATOR,
  MAX_DECIMALS,
  MAX_FEE_BPS,
  applyFee,
  convertDecimals,
  convertDecimalsExact,
  floorToRepresentable,
  formatUnits,
  parseAmountInput,
  parseUnits,
  pow10,
} from "../src/amounts.js";

describe("pow10", () => {
  it("is exact where a double would not be", () => {
    expect(pow10(0)).toBe(1n);
    expect(pow10(18)).toBe(1_000_000_000_000_000_000n);
    expect(pow10(38)).toBe(10n ** 38n);
  });

  it("refuses more decimal places than any asset has", () => {
    expect(() => pow10(MAX_DECIMALS + 1)).toThrow(/DecimalOverflow/);
  });
});

describe("decimal conversion", () => {
  it("is the identity when both sides agree", () => {
    const { converted, dust } = convertDecimals(123n, 6, 6);
    expect(converted).toBe(123n);
    expect(dust).toBe(0n);
  });

  it("widens six decimals into seven, which is the EVM to Stellar direction", () => {
    // One USDC on an EVM chain is 1_000_000. The same dollar on Stellar is 10_000_000.
    const { converted, dust } = convertDecimals(1_000_000n, 6, 7);
    expect(converted).toBe(10_000_000n);
    expect(dust).toBe(0n);
  });

  it("narrows seven decimals into six and reports what did not fit", () => {
    const { converted, dust } = convertDecimals(10_000_001n, 7, 6);
    expect(converted).toBe(1_000_000n);
    expect(dust).toBe(1n);
  });

  it("reports dust rather than rounding it away", () => {
    // The whole reason this returns two values. A function that silently returned 1_000_000 here
    // is a function that loses a tenth of a cent per transfer and never says so.
    const { dust } = convertDecimals(19_999_999n, 7, 6);
    expect(dust).toBe(9n);
  });

  it("refuses an impossible number of decimals", () => {
    expect(() => convertDecimals(1n, 39, 6)).toThrow(/InvalidDecimals/);
    expect(() => convertDecimals(1n, 6, 39)).toThrow(/InvalidDecimals/);
  });

  it("refuses to widen a number past a word", () => {
    expect(() => convertDecimals((1n << 256n) - 1n, 0, 38)).toThrow(/DecimalOverflow/);
  });

  it("leaves zero alone in both directions", () => {
    expect(convertDecimals(0n, 7, 6).converted).toBe(0n);
    expect(convertDecimals(0n, 6, 7).converted).toBe(0n);
  });
});

describe("exact conversion", () => {
  it("passes a value that survives the trip", () => {
    expect(convertDecimalsExact(10_000_000n, 7, 6)).toBe(1_000_000n);
  });

  it("refuses a value that would lose anything", () => {
    expect(() => convertDecimalsExact(10_000_001n, 7, 6)).toThrow(/AmountNotRepresentable/);
  });
});

describe("flooring", () => {
  it("leaves an amount alone when the destination keeps more decimals", () => {
    expect(floorToRepresentable(123n, 6, 7)).toBe(123n);
    expect(floorToRepresentable(123n, 6, 6)).toBe(123n);
  });

  it("drops only what cannot cross", () => {
    expect(floorToRepresentable(10_000_009n, 7, 6)).toBe(10_000_000n);
  });

  it("can floor an amount to nothing, which the caller has to notice", () => {
    // Nine units of a seven decimal asset is less than one unit of a six decimal one. The
    // routers check for this and refuse the transfer; here it just has to be reported honestly.
    expect(floorToRepresentable(9n, 7, 6)).toBe(0n);
  });

  it("agrees with convertDecimals about what is representable", () => {
    for (const amount of [1n, 9n, 10n, 10_000_001n, 999_999_999n]) {
      const floored = floorToRepresentable(amount, 7, 6);
      expect(convertDecimals(floored, 7, 6).dust).toBe(0n);
    }
  });
});

describe("fees", () => {
  it("splits an amount into net and fee", () => {
    const { net, fee } = applyFee(1_000_000n, 30);
    expect(fee).toBe(3_000n);
    expect(net).toBe(997_000n);
    expect(net + fee).toBe(1_000_000n);
  });

  it("charges nothing at zero basis points", () => {
    const { net, fee } = applyFee(1_000_000n, 0);
    expect(fee).toBe(0n);
    expect(net).toBe(1_000_000n);
  });

  it("rounds the fee down, so the fee is never more than it says", () => {
    // 1 unit at 30 bps is 0.003 units. The sender keeps the whole unit rather than being
    // charged a rounded up fee on a rounded down transfer.
    const { net, fee } = applyFee(1n, 30);
    expect(fee).toBe(0n);
    expect(net).toBe(1n);
  });

  it("refuses a zero amount before it looks at the fee", () => {
    expect(() => applyFee(0n, 30)).toThrow(/InvalidAmount/);
  });

  it("refuses a fee past the cap", () => {
    expect(() => applyFee(1_000_000n, MAX_FEE_BPS + 1)).toThrow(/FeeTooHigh/);
  });

  it("never lets the whole amount become fee at the current cap", () => {
    // The cap is one percent, so there is no amount where the fee eats everything. The guard in
    // the contract exists for the day somebody raises the cap, and so does this.
    const { net } = applyFee(1n, MAX_FEE_BPS);
    expect(net).toBeGreaterThan(0n);
  });

  it("uses ten thousand as the denominator, the same as the contracts", () => {
    expect(BPS_DENOMINATOR).toBe(10_000n);
  });
});

describe("formatting, which never touches a float", () => {
  it("writes whole units without a trailing point", () => {
    expect(formatUnits(10_000_000n, 7)).toBe("1");
    expect(formatUnits(0n, 7)).toBe("0");
  });

  it("keeps significant fraction digits and drops the rest", () => {
    expect(formatUnits(12_345_678n, 7)).toBe("1.2345678");
    expect(formatUnits(1_500_000n, 7)).toBe("0.15");
  });

  it("holds a value no double could print correctly", () => {
    const huge = 123_456_789_012_345_678_901_234_567_890n;
    expect(formatUnits(huge, 18)).toBe("123456789012.34567890123456789");
  });

  it("truncates to a digit budget rather than rounding up somebody's balance", () => {
    expect(formatUnits(12_345_678n, 7, 2)).toBe("1.23");
    expect(formatUnits(19_999_999n, 7, 2)).toBe("1.99");
  });

  it("handles a value smaller than one unit", () => {
    expect(formatUnits(1n, 7)).toBe("0.0000001");
  });

  it("has no thousands separators, so what comes out goes back in unchanged", () => {
    // The round trip is the point. A grouped string cannot be fed back to the parser, and a
    // comma means a decimal point to most of the world, so grouping is the app's problem.
    for (const text of ["0", "1", "1.2345678", "0.0000001", "123456789.123", "1000000"]) {
      expect(formatUnits(parseUnits(text, 7), 7)).toBe(text);
    }
  });
});

describe("parsing, strictly", () => {
  it("reads a plain number", () => {
    expect(parseUnits("1", 7)).toBe(10_000_000n);
    expect(parseUnits("0.5", 7)).toBe(5_000_000n);
  });

  it("accepts a bare point on either side", () => {
    expect(parseUnits("1.", 7)).toBe(10_000_000n);
    expect(parseUnits(".5", 7)).toBe(5_000_000n);
  });

  it("refuses more precision than the asset has, instead of quietly dropping it", () => {
    // Somebody typing eight decimals at a seven decimal asset meant something. Truncating it
    // would send a different amount than the one on the screen.
    expect(() => parseUnits("1.12345678", 7)).toThrow(/AmountNotRepresentable/);
  });

  it("refuses anything that is not a bare unsigned decimal", () => {
    for (const bad of ["", ".", "abc", "1.2.3", "-1", "+1", "1e9", " 1", "1 ", "1,234", "0x10"]) {
      expect(() => parseUnits(bad, 7)).toThrow(/InvalidAmount/);
    }
  });

  it("refuses a negative rather than carrying one into the fee maths", () => {
    // Amounts are unsigned everywhere in this protocol. Accepting a sign here would surface
    // three frames later as something that reads like an unrelated bug.
    expect(() => parseUnits("-1", 7)).toThrow(/InvalidAmount/);
  });
});

describe("parsing what somebody typed or pasted", () => {
  it("tidies whitespace and the separators a copied figure arrives with", () => {
    expect(parseAmountInput("  1.5  ", 7)).toBe(15_000_000n);
    expect(parseAmountInput("1 234", 7)).toBe(12_340_000_000n);
    expect(parseAmountInput("1_234", 7)).toBe(12_340_000_000n);
    expect(parseAmountInput("1'234", 7)).toBe(12_340_000_000n);
  });

  it("refuses to guess what a comma meant", () => {
    // "1,234" is one and a bit in Germany and a thousand and a bit in the States. Picking one
    // would be picking somebody's transfer size for them.
    expect(() => parseAmountInput("1,234", 7)).toThrow(/InvalidAmount/);
  });

  it("still refuses outright nonsense", () => {
    expect(() => parseAmountInput("   ", 7)).toThrow(/InvalidAmount/);
    expect(() => parseAmountInput("one", 7)).toThrow(/InvalidAmount/);
  });
});
