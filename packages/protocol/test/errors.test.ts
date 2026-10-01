import { describe, expect, it } from "vitest";
import {
  ERROR_HELP,
  EVM_ERROR_NAMES,
  HYPERION_ERROR_NAMES,
  HyperionProtocolError,
  SOROBAN_ERROR_NAMES,
  describeError,
  fail,
  isHyperionErrorName,
  sorobanErrorCode,
  sorobanErrorName,
} from "../src/errors.js";

describe("the Soroban vocabulary", () => {
  it("is index aligned, so tag one is the first name", () => {
    // The contract numbers its errors from one. Getting this off by one would relabel every
    // failure on the Stellar side, each one plausibly.
    expect(sorobanErrorName(1)).toBe("AlreadyInitialized");
    expect(SOROBAN_ERROR_NAMES[0]).toBe("AlreadyInitialized");
    expect(sorobanErrorName(SOROBAN_ERROR_NAMES.length)).toBe(
      SOROBAN_ERROR_NAMES[SOROBAN_ERROR_NAMES.length - 1],
    );
  });

  it("round trips a name to a code and back", () => {
    for (const name of SOROBAN_ERROR_NAMES) {
      const code = sorobanErrorCode(name);
      expect(code).not.toBeNull();
      if (code !== null) expect(sorobanErrorName(code)).toBe(name);
    }
  });

  it("has no duplicates, which would make one code unreachable", () => {
    expect(new Set(SOROBAN_ERROR_NAMES).size).toBe(SOROBAN_ERROR_NAMES.length);
  });

  it("answers null for a code outside the range rather than guessing", () => {
    expect(sorobanErrorName(0)).toBeNull();
    expect(sorobanErrorName(SOROBAN_ERROR_NAMES.length + 1)).toBeNull();
    expect(sorobanErrorName(-1)).toBeNull();
  });
});

describe("the EVM vocabulary", () => {
  it("has no duplicates", () => {
    expect(new Set(EVM_ERROR_NAMES).size).toBe(EVM_ERROR_NAMES.length);
  });

  it("has no Paused, because that chain gets one from OpenZeppelin", () => {
    // `Pausable` supplies `EnforcedPause()`. Declaring a second one would mean two errors for
    // one condition and a decoder that names whichever it happens to match first.
    expect(EVM_ERROR_NAMES).not.toContain("Paused");
    expect(SOROBAN_ERROR_NAMES).toContain("Paused");
  });

  it("carries arguments on the errors that need them", () => {
    // `SlippageExceeded(wanted, got)` is worth two words of calldata because "the price moved"
    // without the two numbers is a support ticket rather than an answer.
    expect(EVM_ERROR_NAMES).toContain("SlippageExceeded");
    expect(EVM_ERROR_NAMES).toContain("FlowLimitExceeded");
  });

  it("does not claim errors only the Soroban side has", () => {
    for (const sorobanOnly of [
      "InsufficientLiquidity",
      "NotTheRail",
      "GasFloatTooLow",
      "UnknownNonce",
    ]) {
      expect(EVM_ERROR_NAMES).not.toContain(sorobanOnly);
      expect(SOROBAN_ERROR_NAMES).toContain(sorobanOnly);
    }
  });
});

describe("the help", () => {
  it("covers every error on both chains", () => {
    for (const name of HYPERION_ERROR_NAMES) {
      const help = ERROR_HELP[name];
      expect(help.summary.length).toBeGreaterThan(0);
      expect(help.fault.length).toBeGreaterThan(0);
    }
  });

  it("reads like a sentence rather than a constant", () => {
    for (const name of HYPERION_ERROR_NAMES) {
      const { summary } = ERROR_HELP[name];
      expect(summary).toMatch(/^[A-Z"]/);
      expect(summary.endsWith(".")).toBe(true);
      expect(summary).not.toContain("_");
    }
  });

  it("knows which failures are worth retrying", () => {
    // The distinction an app needs to decide between a retry button and an explanation.
    expect(ERROR_HELP.FlowLimitExceeded.retryable).toBe(true);
    expect(ERROR_HELP.Paused.retryable).toBe(true);
    expect(ERROR_HELP.NotEvmAddress.retryable).toBe(false);
    expect(ERROR_HELP.ZeroAddressKey.retryable).toBe(false);
  });

  it("fills in the Soroban code where there is one", () => {
    expect(ERROR_HELP.Paused.sorobanCode).toBe(sorobanErrorCode("Paused"));
    expect(ERROR_HELP.AmountBelowMinimum.sorobanCode).toBeNull();
  });

  it("recognises its own names and nothing else", () => {
    expect(isHyperionErrorName("Paused")).toBe(true);
    expect(isHyperionErrorName("EnforcedPause")).toBe(false);
    expect(isHyperionErrorName("")).toBe(false);
  });

  it("falls back gracefully for a name from a newer deployment", () => {
    // An SDK one release behind the chain should say "something new went wrong" rather than
    // crash while trying to explain it.
    const help = describeError("SomethingNobodyHasShippedYet");
    expect(help.summary.length).toBeGreaterThan(0);
    expect(help.fault).toBe("internal");
  });
});

describe("throwing", () => {
  it("names the error in the message, so a log line is greppable", () => {
    expect(() => {
      fail("ZeroAddressKey");
    }).toThrow(/^ZeroAddressKey: /);
  });

  it("keeps the name on the object too, which is what code should branch on", () => {
    try {
      fail("SlippageExceeded", "wanted 10, got 9");
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(HyperionProtocolError);
      expect((error as HyperionProtocolError).code).toBe("SlippageExceeded");
      expect((error as HyperionProtocolError).message).toContain("wanted 10, got 9");
    }
  });
});
