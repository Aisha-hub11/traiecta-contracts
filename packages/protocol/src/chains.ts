/**
 * The chains Hyperion connects, with the numbers that are easy to get wrong.
 *
 * Every value here was read off a primary source rather than remembered, and the source is named
 * in a comment beside it. The architecture plan is blunt about why: a chain id, a CCTP domain or a
 * contract address that is almost right is worse than one that is missing, because a missing value
 * fails loudly at startup and a wrong one delivers somebody's money to a place it cannot be
 * recovered from.
 *
 * Two traps are encoded rather than explained:
 *
 * - CCTP domains are not chain ids and have no relationship to them. Stellar is domain 27 and Arc
 *   is domain 26, and both numbers look like nothing in particular.
 * - Arc's native gas token is USDC, which it accounts for internally at eighteen decimals while the
 *   ERC-20 everybody actually holds reports six. Both numbers are correct about different things,
 *   and a conversion that picks the wrong one is off by a factor of a trillion.
 */

/** Which half of the bridge a chain sits on. Determines the address format, not much else. */
export type ChainFamily = "stellar" | "evm";

/** Mainnet or testnet. Never mixed inside a single deployment record. */
export type NetworkMode = "mainnet" | "testnet";

/** Hyperion's own name for a chain, which is what travels in a `Destination`. */
export type ChainKey =
  | "stellar"
  | "stellar-testnet"
  | "ethereum"
  | "sepolia"
  | "base"
  | "base-sepolia"
  | "arc"
  | "arc-testnet";

interface ChainBase {
  readonly key: ChainKey;
  readonly family: ChainFamily;
  readonly network: NetworkMode;
  /** What to call it on screen. */
  readonly name: string;
  /** A short form for a tight column. */
  readonly shortName: string;
  /**
   * Circle's domain number, or null where CCTP does not go. Not a chain id, not derived from one,
   * and not guessable.
   */
  readonly cctpDomain: number | null;
  /**
   * Axelar's own name for this chain, or null where Axelar does not list it.
   *
   * Read out of `axelar-chains-config/info` in axelarnetwork/axelar-contract-deployments, which is
   * where Axelar publishes these, rather than guessed. Guessing does not work here: the mainnet id
   * for Ethereum is "Ethereum" with a capital letter while the testnet one is "ethereum-sepolia"
   * without, and Stellar's testnet id carries a version suffix that moves when Axelar redeploys.
   *
   * Still only a default. The adapters take this string explicitly in `setPeer` and `link_chain`,
   * so the deployment record is what a live contract is actually comparing against.
   */
  readonly axelarName: string | null;
  /** Where to send somebody who wants to see a transaction with their own eyes. */
  readonly explorer: ExplorerLinks;
}

export interface ExplorerLinks {
  readonly name: string;
  readonly baseUrl: string;
  /** `{value}` is substituted. Separate templates because no two explorers agree on the paths. */
  readonly txPath: string;
  readonly addressPath: string;
  /** Soroban contracts get their own page on Stellar explorers. Null on EVM chains. */
  readonly contractPath: string | null;
}

export interface StellarChain extends ChainBase {
  readonly family: "stellar";
  /** Signed into every transaction, and the reason a testnet signature is useless on mainnet. */
  readonly networkPassphrase: string;
  readonly defaultRpcUrl: string;
  readonly defaultHorizonUrl: string;
  /** Testnet only. Mainnet XLM has to be bought like everything else. */
  readonly friendbotUrl: string | null;
  /** Stellar's native precision. Seven, and the source of the one digit mismatch with USDC. */
  readonly decimals: 7;
  /** Roughly how long a ledger takes, which is what a Stellar flow window is measured in. */
  readonly ledgerSeconds: number;
}

export interface EvmChain extends ChainBase {
  readonly family: "evm";
  readonly chainId: number;
  readonly defaultRpcUrl: string;
  readonly defaultWsUrl: string | null;
  readonly nativeCurrency: NativeCurrency;
  /** Rough block time in seconds, used to turn confirmations into an honest estimate. */
  readonly blockSeconds: number;
  /**
   * How many blocks the indexer waits before it believes a log.
   *
   * One on a chain with deterministic finality, more where a reorg is a real thing that happens.
   */
  readonly confirmations: number;
  /** Anything about this chain that will bite somebody who assumes it behaves like Ethereum. */
  readonly quirks: readonly string[];
}

export interface NativeCurrency {
  readonly name: string;
  readonly symbol: string;
  /** What the EVM charges gas in. */
  readonly decimals: number;
}

export type Chain = StellarChain | EvmChain;

const STELLAR_MAINNET: StellarChain = {
  key: "stellar",
  family: "stellar",
  network: "mainnet",
  name: "Stellar",
  shortName: "Stellar",
  // Circle publishes Stellar as CCTP V2 domain 27.
  cctpDomain: 27,
  axelarName: "stellar",
  networkPassphrase: "Public Global Stellar Network ; September 2015",
  defaultRpcUrl: "https://mainnet.sorobanrpc.com",
  defaultHorizonUrl: "https://horizon.stellar.org",
  friendbotUrl: null,
  decimals: 7,
  ledgerSeconds: 5,
  explorer: {
    name: "Stellar Expert",
    baseUrl: "https://stellar.expert/explorer/public",
    txPath: "/tx/{value}",
    addressPath: "/account/{value}",
    contractPath: "/contract/{value}",
  },
};

const STELLAR_TESTNET: StellarChain = {
  key: "stellar-testnet",
  family: "stellar",
  network: "testnet",
  name: "Stellar Testnet",
  shortName: "Stellar test",
  cctpDomain: 27,
  axelarName: "stellar-2026-q1-2",
  networkPassphrase: "Test SDF Network ; September 2015",
  defaultRpcUrl: "https://soroban-testnet.stellar.org",
  defaultHorizonUrl: "https://horizon-testnet.stellar.org",
  friendbotUrl: "https://friendbot.stellar.org",
  decimals: 7,
  ledgerSeconds: 5,
  explorer: {
    name: "Stellar Expert",
    baseUrl: "https://stellar.expert/explorer/testnet",
    txPath: "/tx/{value}",
    addressPath: "/account/{value}",
    contractPath: "/contract/{value}",
  },
};

const ETHEREUM: EvmChain = {
  key: "ethereum",
  family: "evm",
  network: "mainnet",
  name: "Ethereum",
  shortName: "Ethereum",
  chainId: 1,
  cctpDomain: 0,
  axelarName: "Ethereum",
  defaultRpcUrl: "https://eth.llamarpc.com",
  defaultWsUrl: null,
  nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
  blockSeconds: 12,
  confirmations: 12,
  quirks: [
    "Gas here dwarfs the protocol fee on a small transfer, so the quote leads with the total cost rather than the percentage.",
  ],
  explorer: {
    name: "Etherscan",
    baseUrl: "https://etherscan.io",
    txPath: "/tx/{value}",
    addressPath: "/address/{value}",
    contractPath: null,
  },
};

const SEPOLIA: EvmChain = {
  key: "sepolia",
  family: "evm",
  network: "testnet",
  name: "Ethereum Sepolia",
  shortName: "Sepolia",
  chainId: 11_155_111,
  cctpDomain: 0,
  axelarName: "ethereum-sepolia",
  defaultRpcUrl: "https://ethereum-sepolia-rpc.publicnode.com",
  defaultWsUrl: null,
  nativeCurrency: { name: "Sepolia Ether", symbol: "ETH", decimals: 18 },
  blockSeconds: 12,
  confirmations: 3,
  quirks: [
    "Public Sepolia endpoints rate limit hard, so the indexer backs off rather than retries.",
  ],
  explorer: {
    name: "Etherscan",
    baseUrl: "https://sepolia.etherscan.io",
    txPath: "/tx/{value}",
    addressPath: "/address/{value}",
    contractPath: null,
  },
};

const BASE: EvmChain = {
  key: "base",
  family: "evm",
  network: "mainnet",
  name: "Base",
  shortName: "Base",
  chainId: 8453,
  cctpDomain: 6,
  axelarName: "base",
  defaultRpcUrl: "https://mainnet.base.org",
  defaultWsUrl: null,
  nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
  blockSeconds: 2,
  confirmations: 6,
  quirks: [
    "Cheap enough that a fee quoted in the bridged asset is the number people actually care about.",
  ],
  explorer: {
    name: "Basescan",
    baseUrl: "https://basescan.org",
    txPath: "/tx/{value}",
    addressPath: "/address/{value}",
    contractPath: null,
  },
};

const BASE_SEPOLIA: EvmChain = {
  key: "base-sepolia",
  family: "evm",
  network: "testnet",
  name: "Base Sepolia",
  shortName: "Base test",
  chainId: 84_532,
  cctpDomain: 6,
  axelarName: "base-sepolia",
  defaultRpcUrl: "https://sepolia.base.org",
  defaultWsUrl: null,
  nativeCurrency: { name: "Sepolia Ether", symbol: "ETH", decimals: 18 },
  blockSeconds: 2,
  confirmations: 3,
  quirks: [],
  explorer: {
    name: "Basescan",
    baseUrl: "https://sepolia.basescan.org",
    txPath: "/tx/{value}",
    addressPath: "/address/{value}",
    contractPath: null,
  },
};

/**
 * Circle's own chain, and the one Hyperion was partly built for.
 *
 * Everything below is from docs.arc.io: chain ids 5042 and 5042002, CCTP domain 26 on both, the RPC
 * and websocket endpoints, the explorers, and the USDC precompile at
 * 0x3600000000000000000000000000000000000000.
 */
const ARC: EvmChain = {
  key: "arc",
  family: "evm",
  network: "mainnet",
  name: "Arc",
  shortName: "Arc",
  chainId: 5042,
  cctpDomain: 26,
  axelarName: null,
  defaultRpcUrl: "https://rpc.mainnet.arc.io",
  defaultWsUrl: "wss://rpc.mainnet.arc.io",
  // Gas is paid in USDC here. The node accounts for it at eighteen decimals internally while the
  // ERC-20 reports six, so a fee shown to somebody has to be converted before it means anything.
  nativeCurrency: { name: "USD Coin", symbol: "USDC", decimals: 18 },
  blockSeconds: 1,
  confirmations: 1,
  quirks: [
    "Gas is paid in USDC, so a transfer and its own fee are denominated in the same asset for once.",
    "Native accounting is eighteen decimals while the USDC ERC-20 reports six. Convert before displaying anything.",
    "Malachite BFT gives deterministic finality, so one confirmation is genuinely final rather than probably final.",
    "The minimum base fee is twenty Gwei, which is a floor rather than a market rate.",
  ],
  explorer: {
    name: "Arc Explorer",
    baseUrl: "https://explorer.arc.io",
    txPath: "/tx/{value}",
    addressPath: "/address/{value}",
    contractPath: null,
  },
};

const ARC_TESTNET: EvmChain = {
  key: "arc-testnet",
  family: "evm",
  network: "testnet",
  name: "Arc Testnet",
  shortName: "Arc test",
  chainId: 5_042_002,
  cctpDomain: 26,
  axelarName: "arc-8",
  defaultRpcUrl: "https://rpc.testnet.arc.io",
  defaultWsUrl: "wss://rpc.testnet.arc.io",
  nativeCurrency: { name: "USD Coin", symbol: "USDC", decimals: 18 },
  blockSeconds: 1,
  confirmations: 1,
  quirks: [
    "Same eighteen against six decimal split as mainnet Arc.",
    "Test USDC comes from faucet.circle.com rather than from a faucet on the chain itself.",
  ],
  explorer: {
    name: "Arc Explorer",
    baseUrl: "https://explorer.testnet.arc.io",
    txPath: "/tx/{value}",
    addressPath: "/address/{value}",
    contractPath: null,
  },
};

export const CHAINS: Readonly<Record<ChainKey, Chain>> = {
  stellar: STELLAR_MAINNET,
  "stellar-testnet": STELLAR_TESTNET,
  ethereum: ETHEREUM,
  sepolia: SEPOLIA,
  base: BASE,
  "base-sepolia": BASE_SEPOLIA,
  arc: ARC,
  "arc-testnet": ARC_TESTNET,
};

export const CHAIN_KEYS: readonly ChainKey[] = Object.keys(CHAINS) as ChainKey[];

/** Every chain on one side of the fence, in the order the interface should offer them. */
export const MAINNET_CHAINS: readonly ChainKey[] = ["stellar", "ethereum", "base", "arc"];
export const TESTNET_CHAINS: readonly ChainKey[] = [
  "stellar-testnet",
  "sepolia",
  "base-sepolia",
  "arc-testnet",
];

export function isChainKey(value: string): value is ChainKey {
  return Object.prototype.hasOwnProperty.call(CHAINS, value);
}

/**
 * A chain by name, throwing rather than returning undefined, because callers always need one.
 *
 * The type says the key is valid and the check says so too. The gap between them is a string that
 * came out of an environment variable, a url or a saved preference, and the type system never saw
 * it. Without the check that string returns undefined typed as a `Chain`, and the failure lands
 * somewhere far away reading a property off nothing.
 */
export function chain(key: ChainKey): Chain {
  // Read through a widened view on purpose. The type says this key is one of ours; the value may
  // have arrived as a plain string from an environment variable, a url or a saved preference,
  // and the compiler never saw it. Without the miss being expressible, an unknown key returns
  // undefined typed as a `Chain` and the failure lands somewhere far away.
  const found: Chain | undefined = (CHAINS as Record<string, Chain | undefined>)[key];
  if (found === undefined) {
    throw new RangeError(`no chain is called ${key}; known chains are ${CHAIN_KEYS.join(", ")}`);
  }
  return found;
}

export function chainByKey(value: string): Chain | null {
  return isChainKey(value) ? CHAINS[value] : null;
}

export function isStellarChain(value: Chain): value is StellarChain {
  return value.family === "stellar";
}

export function isEvmChain(value: Chain): value is EvmChain {
  return value.family === "evm";
}

/** Every chain in a network, Stellar first, because Stellar is always one end of the hop. */
export function chainsFor(network: NetworkMode): readonly Chain[] {
  const keys = network === "mainnet" ? MAINNET_CHAINS : TESTNET_CHAINS;
  return keys.map((key) => CHAINS[key]);
}

/** The Stellar end of a network. Exactly one per network, by design. */
export function stellarChainFor(network: NetworkMode): StellarChain {
  return network === "mainnet" ? STELLAR_MAINNET : STELLAR_TESTNET;
}

export function evmChainsFor(network: NetworkMode): readonly EvmChain[] {
  return chainsFor(network).filter(isEvmChain);
}

/** An EVM chain by its chain id, which is what a connected wallet reports. */
export function chainByEvmId(chainId: number): EvmChain | null {
  for (const key of CHAIN_KEYS) {
    const value = CHAINS[key];
    if (isEvmChain(value) && value.chainId === chainId) return value;
  }
  return null;
}

/** A chain by its CCTP domain, which is what an inbound rail message names it by. */
export function chainByCctpDomain(domain: number, network: NetworkMode): Chain | null {
  for (const value of chainsFor(network)) {
    if (value.cctpDomain === domain) return value;
  }
  return null;
}

/** A link to a transaction. */
export function txUrl(value: Chain, hash: string): string {
  return value.explorer.baseUrl + value.explorer.txPath.replace("{value}", hash);
}

/** A link to an account or contract, picking the right path for a Soroban contract id. */
export function addressUrl(value: Chain, address: string): string {
  const looksLikeContract = value.family === "stellar" && address.startsWith("C");
  const path =
    looksLikeContract && value.explorer.contractPath !== null
      ? value.explorer.contractPath
      : value.explorer.addressPath;
  return value.explorer.baseUrl + path.replace("{value}", address);
}

/**
 * Roughly how long a chain takes to consider a log settled.
 *
 * Only ever shown as an estimate, never used to decide anything, because a number the interface
 * treats as truth is a number that will be wrong during the one congestion event that matters.
 */
export function finalitySeconds(value: Chain): number {
  if (isStellarChain(value)) return value.ledgerSeconds;
  return value.blockSeconds * value.confirmations;
}

/**
 * Every CCTP V2 domain Circle has published, for reading an inbound message that names a chain
 * Hyperion does not route to.
 *
 * An indexer that cannot name domain 9 has to say "an unsupported domain" instead of "Aptos", and
 * the second one is the answer somebody was actually looking for.
 */
export const CCTP_DOMAIN_NAMES: Readonly<Record<number, string>> = {
  0: "Ethereum",
  1: "Avalanche",
  2: "OP Mainnet",
  3: "Arbitrum",
  4: "Noble",
  5: "Solana",
  6: "Base",
  7: "Polygon PoS",
  8: "Sui",
  9: "Aptos",
  10: "Unichain",
  11: "Linea",
  12: "Codex",
  13: "Sonic",
  14: "World Chain",
  26: "Arc",
  27: "Stellar",
};

export function cctpDomainName(domain: number): string {
  return CCTP_DOMAIN_NAMES[domain] ?? `domain ${String(domain)}`;
}
