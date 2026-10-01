/**
 * The rate limiter, mirrored so a quote can say "this is over the cap" before anybody signs.
 *
 * A sliding window rather than a calendar one. Calendar windows are simple and they hand an
 * attacker a free doubling: drain the cap at 23:59, drain it again at 00:01. This decays the
 * previous window's usage in proportion to how far into the current one the clock is, so the cap
 * means the same thing at every moment.
 *
 * The mirror of `FlowGuard.sol` and `hyperion_core::flow`, with one deliberate difference between
 * the two chains that this module inherits from whichever side it is asked about: the EVM window is
 * in seconds and the Stellar window is in ledgers. Same maths, different clock, and the deployment
 * record says which.
 */
import { fail } from "./errors.js";

/** What a token or route has consumed, and what the window before it consumed. */
export interface FlowWindow {
  readonly epoch: bigint;
  readonly consumed: bigint;
  readonly prevConsumed: bigint;
}

export const EMPTY_FLOW_WINDOW: FlowWindow = { epoch: 0n, consumed: 0n, prevConsumed: 0n };

/** Which window a moment falls in. */
export function epochOf(timestamp: bigint, window: bigint): bigint {
  if (window === 0n) fail("InvalidWindow", "a window of zero has no meaning");
  return timestamp / window;
}

/**
 * Advance a window to `timestamp` without consuming anything.
 *
 * One epoch forward keeps the old usage as the decaying tail. Two or more and the tail is gone,
 * because nothing from that long ago is still throttling anybody.
 */
export function rollForward(self: FlowWindow, timestamp: bigint, window: bigint): FlowWindow {
  const epoch = epochOf(timestamp, window);
  if (epoch === self.epoch) return self;
  if (epoch === self.epoch + 1n) {
    return { epoch, consumed: 0n, prevConsumed: self.consumed };
  }
  return { epoch, consumed: 0n, prevConsumed: 0n };
}

/**
 * How much of the cap is in use right now, previous window included at its decayed weight.
 *
 * `remaining` is how much of the current window is still ahead, so the old usage counts for the
 * full cap immediately after a rollover and for nothing just before the next one.
 */
export function effectiveConsumed(self: FlowWindow, timestamp: bigint, window: bigint): bigint {
  const rolled = rollForward(self, timestamp, window);
  const remaining = window - (timestamp % window);
  return (rolled.prevConsumed * remaining) / window + rolled.consumed;
}

/** What is left of the cap, clamped at zero rather than going negative. */
export function available(
  self: FlowWindow,
  limit: bigint,
  timestamp: bigint,
  window: bigint,
): bigint {
  const used = effectiveConsumed(self, timestamp, window);
  return used >= limit ? 0n : limit - used;
}

/**
 * Take `amount` out of the cap, or refuse.
 *
 * Refusing is the entire point, so this throws rather than clamping. A partial transfer is not a
 * smaller version of the transfer somebody asked for, it is a different amount arriving, and the
 * wrong amount arriving is worse than nothing arriving.
 */
export function consume(
  self: FlowWindow,
  limit: bigint,
  amount: bigint,
  timestamp: bigint,
  window: bigint,
): FlowWindow {
  const rolled = rollForward(self, timestamp, window);
  const used = effectiveConsumed(self, timestamp, window);
  if (used + amount > limit) {
    const free = used >= limit ? 0n : limit - used;
    fail(
      "FlowLimitExceeded",
      `asked for ${amount.toString()} with ${free.toString()} left in this window`,
    );
  }
  return { ...rolled, consumed: rolled.consumed + amount };
}

/**
 * When the cap will next have room for `amount`, as a moment on the same clock.
 *
 * Nothing on chain needs this. The interface does, because "over the limit" is a useless thing to
 * tell somebody and "room for this in about nine minutes" is not. Answered by walking forward in
 * window tenths, which is coarse on purpose: a countdown accurate to the second on a number that
 * only moves in steps would be a more precise lie.
 *
 * Returns null when the amount is over the whole cap and waiting will never help.
 */
export function readyAt(
  self: FlowWindow,
  limit: bigint,
  amount: bigint,
  timestamp: bigint,
  window: bigint,
): bigint | null {
  if (amount > limit) return null;
  if (available(self, limit, timestamp, window) >= amount) return timestamp;

  const step = window / 10n === 0n ? 1n : window / 10n;
  // Two windows is the whole memory of this guard, so if there is no room by then there never is.
  const horizon = timestamp + window * 2n + step;
  for (let at = timestamp + step; at <= horizon; at += step) {
    if (available(self, limit, at, window) >= amount) return at;
  }
  return null;
}
