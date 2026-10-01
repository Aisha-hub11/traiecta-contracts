/**
 * Other people's contracts, per chain.
 *
 * None of these are Hyperion's. They are the rails it routes over, and they are listed here so a
 * deploy script and an indexer read the same addresses from the same place instead of from two
 * `.env` files that drifted apart.
 *
 * Every entry carries its own provenance, because "where did this address come from" is the first
 * question anybody reviewing a bridge deployment asks and the hardest one to answer six months
 * later. `confirmed` means a primary source was read for that specific chain during this build.
 * Anything not confirmed is a starting point the deploy script has to check on chain before it will
 * use it, and `scripts/verify-rails` exists for exactly that.
 */
import type { ChainKey } from "../chains.js";
import type { Hex } from "../bytes.js";

/** Circle's CCTP V2 contracts on an EVM chain. */
export interface CctpContracts {
  readonly tokenMessengerV2: Hex;
  /** The variant that charges a fee for fast attestation. Optional, and null where not deployed. */
  readonly tokenMessengerWithFees: Hex | null;
  readonly messageTransmitterV2: Hex;
  readonly tokenMinterV2: Hex;
  readonly messageV2: Hex | null;
  /** Circle's own cross chain token service, present where Circle has deployed one. */
  readonly crossChainTokenService: Hex | null;
}

/** Circle's Gateway, which is a different product from CCTP and shares no addresses with it. */
export interface GatewayContracts {
  readonly gatewayWallet: Hex;
  readonly gatewayMinter: Hex;
}

/**
 * Axelar's contracts on a chain, as Axelar itself publishes them.
 *
 * Read out of `axelar-chains-config/info` in axelarnetwork/axelar-contract-deployments. The
 * interchain token service address is the same on every EVM chain Axelar has deployed to with one
 * exception in this set, and the gas service is not, so neither is inferred from the other.
 *
 * `chainName` is the string the adapters compare against, and it is the field most worth having
 * from a primary source: Axelar's mainnet id for Ethereum is capitalised and its testnet id is
 * not, and Stellar's testnet id carries a version suffix that moves when Axelar redeploys.
 */
export interface AxelarContracts {
  /** Axelar's own name for this chain. */
  readonly chainName: string;
  /** On a Stellar network this is a contract id, so it is a string rather than a hex address. */
  readonly interchainTokenService: string;
  readonly gasService: string;
  readonly gateway: string | null;
}

/** Infrastructure that happens to live at the same address nearly everywhere. */
export interface CommonContracts {
  readonly multicall3: Hex | null;
  readonly permit2: Hex | null;
  readonly create2Factory: Hex | null;
}

export interface RailContracts {
  readonly cctp: CctpContracts | null;
  readonly gateway: GatewayContracts | null;
  /** Null where Axelar has not deployed to this chain, which is not the same as not reachable. */
  readonly axelar: AxelarContracts | null;
  readonly common: CommonContracts;
  /** Where these came from, written so a reviewer does not have to take anybody's word for it. */
  readonly source: string;
  /** Whether a primary source was read for this chain rather than inferred from another. */
  readonly confirmed: boolean;
}

/**
 * The three addresses that are the same on essentially every EVM chain, because they were all
 * deployed through a deterministic factory.
 *
 * Still checked on chain before use. "It is usually there" is not the same as "it is there".
 */
export const UBIQUITOUS: CommonContracts = {
  multicall3: "0xcA11bde05977b3631167028862bE2a173976CA11",
  permit2: "0x000000000022D473030F116dDEE9F6B43aC78BA3",
  create2Factory: "0x4e59b44847b379578588920cA78FbF26c0B4956C",
};

const ARC_DOCS = "docs.arc.io, read during this build";

/**
 * Circle deploys CCTP V2 at the same addresses across the EVM chains it supports, which is why
 * these match Arc's published set exactly. Corroboration is not confirmation, so these are marked
 * unconfirmed and the deploy script calls `localMinter()` on the messenger before it will register
 * a CCTP route on this chain.
 */
const CCTP_V2_MAINNET: CctpContracts = {
  tokenMessengerV2: "0x28b5a0e9C621a5BadaA536219b3a228C8168cf5d",
  tokenMessengerWithFees: "0x71f54F818671cD0D7ea140Da213e5C8b5C92a408",
  messageTransmitterV2: "0x81D40F21F12A8F0E3252Bccb954D722d4c464B64",
  tokenMinterV2: "0xfd78EE919681417d192449715b2594ab58f5D002",
  messageV2: "0xec546b6B005471ECf012e5aF77FBeC07e0FD8f78",
  crossChainTokenService: "0x431871229103b780868f8C6BB820cd16ECf942BC",
};

const CCTP_V2_TESTNET: CctpContracts = {
  tokenMessengerV2: "0x8FE6B999Dc680CcFDD5Bf7EB0974218be2542DAA",
  tokenMessengerWithFees: "0x8745D906D67C346E5eb1aEEED38Eb87F34DF0C0A",
  messageTransmitterV2: "0xE737e5cEBEEBa77EFE34D4aa090756590b1CE275",
  tokenMinterV2: "0xb43db544E2c27092c107639Ad201b3dEfAbcF192",
  messageV2: "0xbaC0179bB358A8936169a63408C8481D582390C4",
  crossChainTokenService: "0x63753E722bd2C2A5DF6EE19C5106662208B81077",
};

const INFERRED_FROM_ARC =
  "Circle's CCTP V2 deployment is address identical across the EVM chains it supports, and this set matches the one Arc publishes. Confirm on the target chain before a mainnet deploy.";

const AXELAR_SOURCE =
  "axelarnetwork/axelar-contract-deployments, axelar-chains-config/info, read during this build";

/**
 * Axelar's interchain token service on every EVM chain in this set except Arc's testnet.
 *
 * Deployed through a deterministic factory, which is why it is one address rather than seven. Arc
 * is the exception and gets its own entry.
 */
const AXELAR_ITS_EVM = "0xB5FB4BE02232B1bBA4dC8f81dc24C26980dE9e3C";
const AXELAR_GAS_MAINNET = "0x2d5d7d31F671F86C782533cc367F14109a082712";
const AXELAR_GAS_TESTNET = "0xbE406F0189A0B4cf3A05C286473D23791Dd44Cc6";

export const RAIL_CONTRACTS: Readonly<Partial<Record<ChainKey, RailContracts>>> = {
  arc: {
    cctp: CCTP_V2_MAINNET,
    gateway: {
      gatewayWallet: "0x77777777Dcc4d5A8B6E418Fd04D8997ef11000eE",
      gatewayMinter: "0x2222222d7164433c4C09B0b0D809a9b52C04C205",
    },
    // Arc mainnet is absent from Axelar's mainnet config, so there is nothing to record. Absent
    // is not the same as unreachable; it means Axelar has not published a deployment for it.
    axelar: null,
    common: UBIQUITOUS,
    source: ARC_DOCS,
    confirmed: true,
  },
  "arc-testnet": {
    cctp: CCTP_V2_TESTNET,
    gateway: {
      gatewayWallet: "0x0077777d7EBA4688BDeF3E311b846F25870A19B9",
      gatewayMinter: "0x0022222ABE238Cc2C7Bb1f21003F0a260052475B",
    },
    axelar: {
      chainName: "arc-8",
      interchainTokenService: "0x7ca8bF7326b870Eab4ef7A9B8E59fCDb47921389",
      gasService: "0xB2C575226fa1828F5101a42CaA5ba4Ce9140c108",
      gateway: "0xAfde45488D741a946E2376aDf5cf94F6a2918F48",
    },
    common: UBIQUITOUS,
    source: ARC_DOCS,
    confirmed: true,
  },
  ethereum: {
    cctp: CCTP_V2_MAINNET,
    gateway: null,
    axelar: {
      // Capitalised, which is not a typo. Axelar's mainnet id for Ethereum carries a capital
      // letter and its testnet id does not, and the adapters compare the raw string.
      chainName: "Ethereum",
      interchainTokenService: AXELAR_ITS_EVM,
      gasService: AXELAR_GAS_MAINNET,
      gateway: null,
    },
    common: UBIQUITOUS,
    source: INFERRED_FROM_ARC,
    confirmed: false,
  },
  sepolia: {
    cctp: CCTP_V2_TESTNET,
    gateway: null,
    axelar: {
      chainName: "ethereum-sepolia",
      interchainTokenService: AXELAR_ITS_EVM,
      gasService: AXELAR_GAS_TESTNET,
      gateway: "0xe432150cce91c13a887f7D836923d5597adD8E31",
    },
    common: UBIQUITOUS,
    source: INFERRED_FROM_ARC,
    confirmed: false,
  },
  base: {
    cctp: CCTP_V2_MAINNET,
    gateway: null,
    axelar: {
      chainName: "base",
      interchainTokenService: AXELAR_ITS_EVM,
      gasService: AXELAR_GAS_MAINNET,
      gateway: null,
    },
    common: UBIQUITOUS,
    source: INFERRED_FROM_ARC,
    confirmed: false,
  },
  "base-sepolia": {
    cctp: CCTP_V2_TESTNET,
    gateway: null,
    axelar: {
      chainName: "base-sepolia",
      interchainTokenService: AXELAR_ITS_EVM,
      gasService: AXELAR_GAS_TESTNET,
      gateway: "0xe432150cce91c13a887f7D836923d5597adD8E31",
    },
    common: UBIQUITOUS,
    source: INFERRED_FROM_ARC,
    confirmed: false,
  },
  // Stellar carries no CCTP entry on purpose. Those contracts are deployed from
  // circlefin/stellar-cctp rather than published as a fixed address list, so they come out of the
  // deployment record and nowhere else. An invented contract id here would be an invented contract
  // id that looks official, which is worse than a startup failure naming the missing value.
  //
  // Axelar does publish its Stellar contracts, so those are here.
  stellar: {
    cctp: null,
    gateway: null,
    axelar: {
      chainName: "stellar",
      interchainTokenService: "CBDBMIOFHGWUFRYH3D3STI2DHBOWGDDBCRKQEUB4RGQEBVG74SEED6C6",
      gasService: "CDZNIEA5FLJY2L4BWFW3P6WPFYWQNZTNP6ED2K5UHD5PNYTIMNFZDD3W",
      gateway: null,
    },
    // No deterministic factory on Soroban, and nothing here needs a multicall.
    common: { multicall3: null, permit2: null, create2Factory: null },
    source: AXELAR_SOURCE,
    confirmed: true,
  },
  "stellar-testnet": {
    cctp: null,
    gateway: null,
    axelar: {
      // Versioned, and it moves. Axelar redeploys its Stellar testnet contracts and bumps this
      // suffix, so a deployment made against one of these stops matching after the next bump.
      // `link_chain` takes the string explicitly for exactly that reason.
      chainName: "stellar-2026-q1-2",
      interchainTokenService: "CC7LAC4S7KAPIM26WKWFSOXWLNLSG7EXTC2EVY2H2UPKSTX3Z2A7M5YP",
      gasService: "CCLZOCGHHC6F6JCZHEUP53LDQHRBPPCNRYXOVFZFS3O63OGRC47CKCGV",
      gateway: "CB2JYOOZPHO43R57TC5PXV22QICKIDC5NKRF62BZG2J6JYFUIQPIAYY3",
    },
    common: { multicall3: null, permit2: null, create2Factory: null },
    source: AXELAR_SOURCE,
    confirmed: true,
  },
};

export function railContracts(key: ChainKey): RailContracts | null {
  return RAIL_CONTRACTS[key] ?? null;
}

/** Circle's attestation service, which the keeper polls and nothing else talks to. */
export const IRIS_API = {
  mainnet: "https://iris-api.circle.com",
  testnet: "https://iris-api-sandbox.circle.com",
} as const;

/** Where Circle hands out test USDC. Not an API, just the page to send somebody to. */
export const CIRCLE_FAUCET_URL = "https://faucet.circle.com";
