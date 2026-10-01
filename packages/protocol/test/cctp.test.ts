import { describe, expect, it } from "vitest";
import {
  BURN_HOOK_DATA,
  BURN_MESSAGE_VERSION,
  FINALITY_THRESHOLD_FINALIZED,
  HOOK_VERSION,
  MESSAGE_VERSION,
  MSG_BODY,
  STELLAR_DOMAIN,
  assertSupportedBurn,
  assertSupportedMessage,
  cctpBody,
  cctpHookData,
  displayNonce,
  isFinalized,
  parseCctpBurnMessage,
  parseCctpMessage,
} from "../src/cctp.js";
import { concatBytes, fromHex, toHex } from "../src/bytes.js";
import { encodeCctpHook } from "../src/notes.js";
import { G_ADDR, G_KEY } from "./fixtures.js";

/** Offsets from `hyperion_core::cctp`, typed out rather than imported, so a change breaks a test. */
const OFF = {
  version: 0,
  sourceDomain: 4,
  destinationDomain: 8,
  nonce: 12,
  sender: 44,
  recipient: 76,
  destinationCaller: 108,
  minFinality: 140,
  finalityExecuted: 144,
} as const;

const BURN_OFF = {
  version: 0,
  burnToken: 4,
  mintRecipient: 36,
  amount: 68,
  messageSender: 100,
  maxFee: 132,
  feeExecuted: 164,
  expirationBlock: 196,
} as const;

function u32(value: number): Uint8Array {
  return fromHex(`0x${value.toString(16).padStart(8, "0")}`);
}

function word(hex: string): Uint8Array {
  return fromHex(`0x${hex.replace(/^0x/, "").padStart(64, "0")}`);
}

function buildBurnBody(options: {
  amount: bigint;
  mintRecipient: string;
  hook?: Uint8Array;
  maxFee?: bigint;
}): Uint8Array {
  const body = new Uint8Array(BURN_HOOK_DATA);
  body.set(u32(BURN_MESSAGE_VERSION), BURN_OFF.version);
  body.set(word("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"), BURN_OFF.burnToken);
  body.set(word(options.mintRecipient), BURN_OFF.mintRecipient);
  body.set(word(options.amount.toString(16)), BURN_OFF.amount);
  body.set(word("1111111111111111111111111111111111111111"), BURN_OFF.messageSender);
  body.set(word((options.maxFee ?? 0n).toString(16)), BURN_OFF.maxFee);
  body.set(word("0"), BURN_OFF.feeExecuted);
  body.set(word("0"), BURN_OFF.expirationBlock);
  return options.hook === undefined ? body : concatBytes(body, options.hook);
}

function buildMessage(options: {
  body: Uint8Array;
  sourceDomain?: number;
  destinationDomain?: number;
  nonce?: string;
  finalityExecuted?: number;
}): Uint8Array {
  const header = new Uint8Array(MSG_BODY);
  header.set(u32(MESSAGE_VERSION), OFF.version);
  header.set(u32(options.sourceDomain ?? 0), OFF.sourceDomain);
  header.set(u32(options.destinationDomain ?? STELLAR_DOMAIN), OFF.destinationDomain);
  header.set(word(options.nonce ?? "2a"), OFF.nonce);
  header.set(word("28b5a0e9c621a5badaa536219b3a228c8168cf5d"), OFF.sender);
  header.set(word("431871229103b780868f8c6bb820cd16ecf942bc"), OFF.recipient);
  header.set(word("0"), OFF.destinationCaller);
  header.set(u32(FINALITY_THRESHOLD_FINALIZED), OFF.minFinality);
  header.set(u32(options.finalityExecuted ?? FINALITY_THRESHOLD_FINALIZED), OFF.finalityExecuted);
  return concatBytes(header, options.body);
}

describe("the header", () => {
  it("reads every field off its documented offset", () => {
    const raw = buildMessage({
      body: buildBurnBody({ amount: 1_000_000n, mintRecipient: "beef" }),
    });
    const message = parseCctpMessage(raw);

    expect(message.version).toBe(MESSAGE_VERSION);
    expect(message.sourceDomain).toBe(0);
    expect(message.destinationDomain).toBe(STELLAR_DOMAIN);
    expect(message.minFinalityThreshold).toBe(FINALITY_THRESHOLD_FINALIZED);
    expect(message.finalityThresholdExecuted).toBe(FINALITY_THRESHOLD_FINALIZED);
  });

  it("knows Stellar is domain twenty seven", () => {
    // Not a guess and not configurable. Circle assigned it, and an adapter pointed at the wrong
    // domain burns on one chain and mints on another.
    expect(STELLAR_DOMAIN).toBe(27);
  });

  it("keeps the nonce at full width", () => {
    const raw = buildMessage({
      body: buildBurnBody({ amount: 1n, mintRecipient: "beef" }),
      nonce: "ff".repeat(32),
    });
    const message = parseCctpMessage(raw);
    expect(message.nonce).toBe(`0x${"ff".repeat(32)}`);
  });

  it("offers a short nonce for humans and says it is not a key", () => {
    // The bottom eight bytes. Fine on a screen, catastrophic as a replay key, which is why the
    // routers key their guards on the whole thirty two bytes instead.
    const raw = buildMessage({
      body: buildBurnBody({ amount: 1n, mintRecipient: "beef" }),
      nonce: `${"ab".repeat(24)}0000000000000001`,
    });
    expect(displayNonce(parseCctpMessage(raw))).toBe(1n);
  });

  it("refuses a message too short to hold a header", () => {
    expect(() => parseCctpMessage(new Uint8Array(100))).toThrow(/MalformedMessage/);
  });

  it("reports an unknown version rather than refusing to look at it", () => {
    // Same split as `hyperion_core::cctp`. A codec that threw here could not be used to inspect
    // a message nobody is acting on, which is most of what an indexer does.
    const raw = buildMessage({ body: buildBurnBody({ amount: 1n, mintRecipient: "beef" }) });
    raw.set(u32(2), OFF.version);
    expect(parseCctpMessage(raw).version).toBe(2);
  });
});

describe("the checks the adapter makes before it acts", () => {
  it("passes a message and a burn body at the versions Hyperion speaks", () => {
    const raw = buildMessage({ body: buildBurnBody({ amount: 1n, mintRecipient: "beef" }) });
    expect(() => {
      assertSupportedMessage(parseCctpMessage(raw));
      assertSupportedBurn(parseCctpBurnMessage(cctpBody(raw)));
    }).not.toThrow();
  });

  it("refuses a header version it was not written for", () => {
    const raw = buildMessage({ body: buildBurnBody({ amount: 1n, mintRecipient: "beef" }) });
    raw.set(u32(2), OFF.version);
    expect(() => {
      assertSupportedMessage(parseCctpMessage(raw));
    }).toThrow(/UnsupportedMessageVersion/);
  });

  it("refuses a burn version it was not written for", () => {
    const body = buildBurnBody({ amount: 1n, mintRecipient: "beef" });
    body.set(u32(7), BURN_OFF.version);
    expect(() => {
      assertSupportedBurn(parseCctpBurnMessage(body));
    }).toThrow(/UnsupportedMessageVersion/);
  });
});

describe("finality", () => {
  it("is final at the threshold Circle calls final", () => {
    const raw = buildMessage({ body: buildBurnBody({ amount: 1n, mintRecipient: "beef" }) });
    expect(isFinalized(parseCctpMessage(raw))).toBe(true);
  });

  it("is not final below it, which is what a fast transfer looks like", () => {
    const raw = buildMessage({
      body: buildBurnBody({ amount: 1n, mintRecipient: "beef" }),
      finalityExecuted: 1_000,
    });
    expect(isFinalized(parseCctpMessage(raw))).toBe(false);
  });
});

describe("the burn body", () => {
  it("is everything after the header", () => {
    const body = buildBurnBody({ amount: 1_000_000n, mintRecipient: "beef" });
    const raw = buildMessage({ body });
    expect(toHex(cctpBody(raw))).toBe(toHex(body));
  });

  it("reads the amount and the mint recipient", () => {
    const raw = buildMessage({
      body: buildBurnBody({ amount: 2_500_000n, mintRecipient: G_KEY.slice(2) }),
    });
    const burn = parseCctpBurnMessage(cctpBody(raw));
    expect(burn.amount).toBe(2_500_000n);
    expect(burn.mintRecipient).toBe(G_KEY);
    expect(burn.version).toBe(BURN_MESSAGE_VERSION);
  });

  it("refuses an amount no Soroban i128 could hold", () => {
    // A number past the signed 128 bit range cannot survive the trip to the Stellar side, and
    // finding that out at the mint rather than at the parse is finding it out too late.
    const raw = buildMessage({
      body: buildBurnBody({ amount: (1n << 200n) - 1n, mintRecipient: "beef" }),
    });
    expect(() => parseCctpBurnMessage(cctpBody(raw))).toThrow(/DecimalOverflow/);
  });

  it("refuses a body too short to be a burn message", () => {
    expect(() => parseCctpBurnMessage(new Uint8Array(100))).toThrow(/MalformedMessage/);
  });
});

describe("the hook", () => {
  it("is empty when nobody attached one", () => {
    const raw = buildMessage({ body: buildBurnBody({ amount: 1n, mintRecipient: "beef" }) });
    expect(cctpHookData(cctpBody(raw)).length).toBe(0);
  });

  it("carries a Stellar destination when there is one", () => {
    const hook = encodeCctpHook(G_ADDR);
    const raw = buildMessage({
      body: buildBurnBody({ amount: 1n, mintRecipient: "beef", hook }),
    });
    expect(toHex(cctpHookData(cctpBody(raw)))).toBe(toHex(hook));
  });

  it("shares its version with the Axelar note", () => {
    expect(HOOK_VERSION).toBe(1);
  });
});
