import { describe, expect, it } from "vitest";
import {
  EMPTY_FLOW_WINDOW,
  available,
  consume,
  effectiveConsumed,
  epochOf,
  readyAt,
  rollForward,
} from "../src/flow.js";

/** An hour, in whatever unit the chain counts. Seconds on EVM, ledgers on Soroban. */
const WINDOW = 3_600n;
const LIMIT = 1_000_000n;

describe("epochs", () => {
  it("is the timestamp divided by the window", () => {
    expect(epochOf(0n, WINDOW)).toBe(0n);
    expect(epochOf(3_599n, WINDOW)).toBe(0n);
    expect(epochOf(3_600n, WINDOW)).toBe(1n);
    expect(epochOf(7_201n, WINDOW)).toBe(2n);
  });

  it("refuses a window of zero rather than dividing by it", () => {
    expect(() => epochOf(1n, 0n)).toThrow(/InvalidWindow/);
  });
});

describe("rolling forward", () => {
  it("leaves a window alone inside its own epoch", () => {
    const self = { epoch: 1n, consumed: 500n, prevConsumed: 100n };
    expect(rollForward(self, 3_600n, WINDOW)).toEqual(self);
  });

  it("demotes the current epoch to the previous one at exactly plus one", () => {
    const self = { epoch: 1n, consumed: 500n, prevConsumed: 100n };
    expect(rollForward(self, 7_200n, WINDOW)).toEqual({
      epoch: 2n,
      consumed: 0n,
      prevConsumed: 500n,
    });
  });

  it("forgets everything after two windows of quiet", () => {
    const self = { epoch: 1n, consumed: 500n, prevConsumed: 100n };
    expect(rollForward(self, 14_400n, WINDOW)).toEqual({
      epoch: 4n,
      consumed: 0n,
      prevConsumed: 0n,
    });
  });
});

describe("the sliding part", () => {
  it("counts the whole previous window at the moment it rolls", () => {
    // At the instant the epoch ticks over, none of the previous window has slid out yet.
    const self = { epoch: 0n, consumed: 1_000n, prevConsumed: 0n };
    expect(effectiveConsumed(self, 3_600n, WINDOW)).toBe(1_000n);
  });

  it("decays the previous window linearly across the current one", () => {
    const self = { epoch: 0n, consumed: 1_000n, prevConsumed: 0n };
    // Halfway through the next window, half of the previous one still counts.
    expect(effectiveConsumed(self, 5_400n, WINDOW)).toBe(500n);
    // Three quarters through, a quarter counts.
    expect(effectiveConsumed(self, 6_300n, WINDOW)).toBe(250n);
  });

  it("is a sliding window rather than a calendar one", () => {
    // The difference that matters: a calendar limit lets somebody spend the whole cap at 23:59
    // and the whole cap again at 00:01. A sliding one does not.
    const spent = consume(EMPTY_FLOW_WINDOW, LIMIT, LIMIT, 3_599n, WINDOW);
    expect(available(spent, LIMIT, 3_601n, WINDOW)).toBeLessThan(LIMIT);
  });

  it("adds the current window's own consumption on top", () => {
    const self = { epoch: 1n, consumed: 200n, prevConsumed: 1_000n };
    // Halfway through epoch one: half of the previous thousand, plus the two hundred spent here.
    expect(effectiveConsumed(self, 5_400n, WINDOW)).toBe(700n);
  });
});

describe("headroom", () => {
  it("is the whole limit when nothing has moved", () => {
    expect(available(EMPTY_FLOW_WINDOW, LIMIT, 0n, WINDOW)).toBe(LIMIT);
  });

  it("shrinks by what has been spent", () => {
    const spent = consume(EMPTY_FLOW_WINDOW, LIMIT, 400_000n, 0n, WINDOW);
    expect(available(spent, LIMIT, 0n, WINDOW)).toBe(600_000n);
  });

  it("clamps at zero rather than going negative", () => {
    // A limit lowered after the fact leaves more consumed than allowed. Reporting a negative
    // number here would underflow on the Soroban side and read as an enormous allowance.
    const spent = consume(EMPTY_FLOW_WINDOW, LIMIT, LIMIT, 0n, WINDOW);
    expect(available(spent, LIMIT / 2n, 0n, WINDOW)).toBe(0n);
  });

  it("recovers fully after two quiet windows", () => {
    const spent = consume(EMPTY_FLOW_WINDOW, LIMIT, LIMIT, 0n, WINDOW);
    expect(available(spent, LIMIT, WINDOW * 2n, WINDOW)).toBe(LIMIT);
  });
});

describe("consuming", () => {
  it("accumulates within a window", () => {
    let state = consume(EMPTY_FLOW_WINDOW, LIMIT, 300_000n, 0n, WINDOW);
    state = consume(state, LIMIT, 300_000n, 100n, WINDOW);
    expect(state.consumed).toBe(600_000n);
  });

  it("refuses the transfer that would cross the line, and says how much is left", () => {
    const state = consume(EMPTY_FLOW_WINDOW, LIMIT, 900_000n, 0n, WINDOW);
    expect(() => consume(state, LIMIT, 200_000n, 0n, WINDOW)).toThrow(/FlowLimitExceeded/);
  });

  it("allows an amount exactly equal to the headroom", () => {
    const state = consume(EMPTY_FLOW_WINDOW, LIMIT, 900_000n, 0n, WINDOW);
    expect(consume(state, LIMIT, 100_000n, 0n, WINDOW).consumed).toBe(LIMIT);
  });

  it("rolls the window before it charges against it", () => {
    const state = consume(EMPTY_FLOW_WINDOW, LIMIT, LIMIT, 0n, WINDOW);
    // Two windows later the slate is clean, so the full limit goes through again.
    expect(consume(state, LIMIT, LIMIT, WINDOW * 2n, WINDOW).consumed).toBe(LIMIT);
  });

  it("does not mutate the window it was given", () => {
    const start = { epoch: 0n, consumed: 0n, prevConsumed: 0n };
    consume(start, LIMIT, 500n, 0n, WINDOW);
    expect(start.consumed).toBe(0n);
  });
});

describe("when an amount will fit", () => {
  it("is now when there is already room", () => {
    expect(readyAt(EMPTY_FLOW_WINDOW, LIMIT, 1n, 0n, WINDOW)).toBe(0n);
  });

  it("is some time later when the window is full", () => {
    const full = consume(EMPTY_FLOW_WINDOW, LIMIT, LIMIT, 0n, WINDOW);
    const when = readyAt(full, LIMIT, LIMIT / 2n, 0n, WINDOW);
    expect(when).not.toBeNull();
    expect(when).toBeGreaterThan(0n);
    if (when !== null) {
      expect(available(full, LIMIT, when, WINDOW)).toBeGreaterThanOrEqual(LIMIT / 2n);
    }
  });

  it("is never, for an amount larger than the whole cap", () => {
    // Waiting does not help. Saying so is more useful than a countdown that never ends.
    expect(readyAt(EMPTY_FLOW_WINDOW, LIMIT, LIMIT + 1n, 0n, WINDOW)).toBeNull();
  });
});
