/**
 * Shared test vectors.
 *
 * Every address here is lifted verbatim from `evm/test/unit/StellarAddress.t.sol`, which passes
 * against a Solidity implementation, and most of them also appear in the Rust suite. That matters:
 * a codec checked only against its own output proves the encoder and decoder agree with each other
 * and nothing else. Using the same strings all three implementations are already held to means
 * this file disagreeing with them is a real failure rather than a new opinion.
 */

/** A classic ed25519 account. */
export const G_ADDR = "GA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RLVNR";
export const G_KEY = "0x3b9c2115c0efd344ba0a8901eb1cfe44f36cf7dbe946beb0ee4820f7ec49bf15" as const;

/** A Soroban contract. */
export const C_ADDR = "CAGR5KFYMZYI7WWQ6TWYYZ346T7GNZLKER4DOJTAG3SOB46QLR5RAPSN";
export const C_KEY = "0x0d1ea8b866708fdad0f4ed8c677cf4fe66e56a247837266036e4e0f3d05c7b10" as const;

/** The same account as `G_ADDR`, with a sub account id folded in. */
export const M_ADDR = "MA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RKABAAAAAAAAAAFLXQ";

/**
 * Two to the fifty three plus one.
 *
 * Chosen because it is the first integer a JavaScript number cannot hold. Anything in this
 * codebase that quietly turns a muxed id into a double gives back two to the fifty three, and
 * this fixture is the only reason that shows up as a failure rather than as a payment credited
 * to the wrong sub account.
 */
export const MUXED_ID = 9_007_199_254_740_993n;

/** Addresses that have to be refused, and the reason each one is wrong. */
export const BAD_ADDRESSES: readonly { value: string; why: string }[] = [
  { value: "", why: "empty" },
  { value: "GA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RLVN", why: "one character short" },
  { value: "GA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RLVNRR", why: "one character long" },
  {
    value: "GA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RLVN1",
    why: "1 is not in the alphabet",
  },
  { value: "ga5zyiivydx5grf2bkeqd2y47zcpg3hx3puunpvq5zecb57mjg7rlvnr", why: "lowercase" },
  { value: "GA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RLVNV", why: "broken checksum" },
  {
    value: "DA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RKIJG",
    why: "version byte SEP-23 does not assign",
  },
  {
    value: "MA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RKHEO",
    why: "muxed version byte, no room for an id",
  },
  {
    value: "GA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RKAAAAAAAAAAAAFTNI",
    why: "account version byte on a muxed length string",
  },
  {
    value: "MA5ZYIIVYDX5GRF2BKEQD2Y47ZCPG3HX3PUUNPVQ5ZECB57MJG7RKABAAAAAAAAAAFLXR",
    why: "the spare bit in the last character is set",
  },
];

/** Correctly formed, and still refused, because nobody holds the key. */
export const ZERO_ACCOUNT = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

/** A real published issuer, as a sanity check against something nobody in this repo invented. */
export const USDC_MAINNET_ISSUER = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

export const EVM_RECIPIENT = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8" as const;
