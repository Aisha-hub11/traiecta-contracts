import { describe, expect, it } from "vitest";
import { AddressKind, STRKEY_LEN_MUXED, STRKEY_LEN_PLAIN } from "../src/addresses.js";
import { asciiToBytes, concatBytes, fromHex, toHex } from "../src/bytes.js";
import {
  METADATA_CONTRACT_CALL,
  NOTE_VERSION,
  OUTBOUND_NOTE_LEN,
  decodeCctpHook,
  decodeInboundNote,
  decodeOutboundNote,
  encodeAxelarMetadata,
  encodeCctpHook,
  encodeInboundNote,
  encodeOutboundNote,
  toEvmAddress,
  toWord,
} from "../src/notes.js";
import { C_ADDR, EVM_RECIPIENT, G_ADDR, G_KEY, MUXED_ID, M_ADDR } from "./fixtures.js";

const NONCE = 0x0102030405060708n;

/**
 * Payloads are built here by hand rather than by calling the encoder.
 *
 * Same reasoning as the Solidity suite: a decoder checked against its own encoder proves the pair
 * agree and says nothing about whether either matches what the other chain writes. The layouts
 * below are typed out from the wire format, so a change on any side breaks a test here.
 */
function packedOutbound(recipient: string, nonce: bigint): Uint8Array {
  return concatBytes(
    new Uint8Array([NOTE_VERSION]),
    fromHex(recipient),
    fromHex(`0x${nonce.toString(16).padStart(16, "0")}`),
  );
}

describe("outbound notes, which arrive from Stellar", () => {
  it("is a version, an address and a counter", () => {
    const payload = packedOutbound(EVM_RECIPIENT, NONCE);
    expect(payload.length).toBe(OUTBOUND_NOTE_LEN);

    const note = decodeOutboundNote(payload);
    expect(note.recipient.toLowerCase()).toBe(EVM_RECIPIENT.toLowerCase());
    expect(note.nonce).toBe(NONCE);
  });

  it("reads the nonce big endian", () => {
    // One byte out of place turns transfer one into transfer seventy two quadrillion, and the
    // indexer loses the ability to pair the two halves of a hop.
    const note = decodeOutboundNote(packedOutbound(EVM_RECIPIENT, 1n));
    expect(note.nonce).toBe(1n);
  });

  it("refuses a payload that is not exactly twenty nine bytes", () => {
    const short = packedOutbound(EVM_RECIPIENT, NONCE).slice(0, 28);
    expect(() => decodeOutboundNote(short)).toThrow(/MalformedMessage/);

    const long = concatBytes(packedOutbound(EVM_RECIPIENT, NONCE), new Uint8Array([0]));
    expect(() => decodeOutboundNote(long)).toThrow(/MalformedMessage/);
  });

  it("refuses a version it does not know", () => {
    const payload = packedOutbound(EVM_RECIPIENT, NONCE);
    payload[0] = 2;
    expect(() => decodeOutboundNote(payload)).toThrow(/UnsupportedHookVersion/);
  });

  it("round trips through its own encoder as well", () => {
    const note = decodeOutboundNote(encodeOutboundNote(EVM_RECIPIENT, NONCE));
    expect(note.recipient.toLowerCase()).toBe(EVM_RECIPIENT.toLowerCase());
    expect(note.nonce).toBe(NONCE);
  });

  it("accepts hex as readily as bytes, because a log gives you hex", () => {
    const note = decodeOutboundNote(toHex(packedOutbound(EVM_RECIPIENT, NONCE)));
    expect(note.nonce).toBe(NONCE);
  });
});

describe("inbound notes, which travel to Stellar", () => {
  it("is a version, a tagged strkey and a counter", () => {
    const note = encodeInboundNote(G_ADDR, NONCE);
    // version + kind + length + 56 characters + 8 bytes of nonce
    expect(note.length).toBe(1 + 1 + 1 + STRKEY_LEN_PLAIN + 8);
    expect(note[0]).toBe(NOTE_VERSION);
    expect(note[1]).toBe(AddressKind.Account);
    expect(note[2]).toBe(STRKEY_LEN_PLAIN);
    expect(toHex(note.slice(3, 3 + STRKEY_LEN_PLAIN))).toBe(toHex(asciiToBytes(G_ADDR)));
  });

  it("is eighty bytes for a muxed address", () => {
    // 1 + 1 + 1 + 69 + 8. The arithmetic is spelled out because an off by one here was a real
    // bug once: a minimum length constant had folded one strkey character into its own total.
    const note = encodeInboundNote(M_ADDR, NONCE);
    expect(note.length).toBe(80);
    expect(note[1]).toBe(AddressKind.MuxedAccount);
    expect(note[2]).toBe(STRKEY_LEN_MUXED);
  });

  it("reads back what it wrote, for all three kinds", () => {
    for (const [address, kind] of [
      [G_ADDR, AddressKind.Account],
      [C_ADDR, AddressKind.Contract],
      [M_ADDR, AddressKind.MuxedAccount],
    ] as const) {
      const decoded = decodeInboundNote(encodeInboundNote(address, NONCE));
      expect(decoded.kind).toBe(kind);
      expect(decoded.strkey).toBe(address);
      expect(decoded.nonce).toBe(NONCE);
    }
  });

  it("reads the nonce off the end, not off a fixed offset", () => {
    // The strkey in the middle is two different lengths, so the nonce cannot live at a constant
    // offset from the front. Reading it from the back is the whole trick.
    const plain = decodeInboundNote(encodeInboundNote(G_ADDR, NONCE));
    const muxed = decodeInboundNote(encodeInboundNote(M_ADDR, NONCE));
    expect(plain.nonce).toBe(NONCE);
    expect(muxed.nonce).toBe(NONCE);
  });

  it("refuses a note whose declared length does not match its contents", () => {
    const note = encodeInboundNote(G_ADDR, NONCE);
    note[2] = 55;
    expect(() => decodeInboundNote(note)).toThrow(/MalformedMessage/);
  });

  it("refuses a note whose kind byte disagrees with the address inside it", () => {
    // A note built by something other than Hyperion. Catching it here means the transfer fails
    // in the app rather than at the rail, where nobody can read the reason.
    const note = encodeInboundNote(G_ADDR, NONCE);
    note[1] = AddressKind.Contract;
    expect(() => decodeInboundNote(note)).toThrow(/MalformedMessage/);
  });

  it("refuses a version it does not know", () => {
    const note = encodeInboundNote(G_ADDR, NONCE);
    note[0] = 9;
    expect(() => decodeInboundNote(note)).toThrow(/UnsupportedHookVersion/);
  });

  it("refuses a note too short to hold anything", () => {
    expect(() => decodeInboundNote(new Uint8Array([NOTE_VERSION, 0, 0]))).toThrow(
      /MalformedMessage/,
    );
  });

  it("refuses an address it cannot parse before it builds anything", () => {
    expect(() => encodeInboundNote("not an address", NONCE)).toThrow();
  });
});

describe("the CCTP hook", () => {
  it("is a version and a tagged destination, with no room for anything else", () => {
    const hook = encodeCctpHook(G_ADDR);
    expect(hook.length).toBe(34);
    expect(hook[0]).toBe(NOTE_VERSION);
    expect(hook[1]).toBe(AddressKind.Account);
    expect(toHex(hook.slice(2))).toBe(G_KEY);
  });

  it("is forty two bytes when it carries a sub account id", () => {
    const hook = encodeCctpHook(M_ADDR);
    expect(hook.length).toBe(42);
    const decoded = decodeCctpHook(hook);
    expect(decoded.kind).toBe(AddressKind.MuxedAccount);
    expect(decoded.key).toBe(G_KEY);
    expect(decoded.muxedId).toBe(MUXED_ID);
  });

  it("round trips a contract destination", () => {
    const decoded = decodeCctpHook(encodeCctpHook(C_ADDR));
    expect(decoded.kind).toBe(AddressKind.Contract);
    expect(decoded.muxedId).toBe(0n);
  });

  it("refuses a length that is neither thirty four nor forty two", () => {
    expect(() => decodeCctpHook(new Uint8Array(35))).toThrow(/MalformedMessage/);
  });

  it("refuses a kind byte that does not match the length", () => {
    const hook = encodeCctpHook(G_ADDR);
    hook[1] = AddressKind.MuxedAccount;
    expect(() => decodeCctpHook(hook)).toThrow(/MalformedMessage/);
  });

  it("shares its version with the Axelar note on purpose", () => {
    // Both versions move together, so an indexer reading either one only has to know one number.
    expect(NOTE_VERSION).toBe(1);
  });
});

describe("the Axelar metadata envelope", () => {
  it("is four bytes of zero then the note", () => {
    const note = encodeInboundNote(G_ADDR, NONCE);
    const envelope = encodeAxelarMetadata(note);
    expect(envelope.length).toBe(4 + note.length);
    expect(toHex(envelope.slice(0, 4))).toBe("0x00000000");
    expect(toHex(envelope.slice(4))).toBe(toHex(note));
    expect(METADATA_CONTRACT_CALL).toBe(0);
  });
});

describe("words and addresses", () => {
  it("takes an address out of the low twenty bytes of a word", () => {
    expect(toEvmAddress(toWord(EVM_RECIPIENT)).toLowerCase()).toBe(EVM_RECIPIENT.toLowerCase());
  });

  it("refuses a word with anything in the top twelve bytes", () => {
    // A rail slot holding a Stellar key read as an EVM address is a transfer to a contract that
    // does not exist. The twelve zero bytes are the only thing that tells the two apart.
    const dirty = `0x01${"00".repeat(11)}${EVM_RECIPIENT.slice(2)}`;
    expect(() => toEvmAddress(dirty as `0x${string}`)).toThrow(/NotEvmAddress/);
  });

  it("refuses a word that is not thirty two bytes", () => {
    expect(() => toEvmAddress("0x1234")).toThrow();
  });
});
