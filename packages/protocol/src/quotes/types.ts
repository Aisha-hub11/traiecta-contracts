import type { RouteKind } from "../routes.js";
import { RouteKind as RouteKindEnum } from "../routes.js";
import type { RouterSnapshot, RouteQuote } from "../quotes.js";

/**
 * Historical destination execution gas estimates per rail.
 *
 * - Circle CCTP V2: mint execution on destination (~65,000 gas)
 * - Axelar Interchain Token Service: token execution & dispatch (~150,000 gas)
 * - Axelar General Message Passing: contract call execution (~200,000 gas)
 * - Allbridge Core: swap & release from liquidity pool (~120,000 gas)
 */
export const RAIL_DESTINATION_GAS_ESTIMATES: Readonly<Record<RouteKind, bigint>> = Object.freeze({
  [RouteKindEnum.Cctp]: 65_000n,
  [RouteKindEnum.AxelarIts]: 150_000n,
  [RouteKindEnum.AxelarGmp]: 200_000n,
  [RouteKindEnum.Allbridge]: 120_000n,
});

/**
 * Inputs for evaluating multiple amount tiers across rails in a single batch.
 */
export interface QuoteParams {
  /** The destination address, exactly as entered/displayed. */
  readonly strkey: string;
  /** Decimal places of the asset on the receiving chain. */
  readonly destinationDecimals: number;
  /** Router snapshot of current on-chain state. */
  readonly snapshot: RouterSnapshot;
  /** Optional custom destination gas estimates per rail, overriding historical defaults. */
  readonly gasEstimates?: Partial<Record<RouteKind, bigint>>;
}

/**
 * Result of quoting an amount tier across all rails, including gas projections and winning rail.
 */
export interface BatchQuoteResult {
  /** The gross input tier amount evaluated. */
  readonly amount: bigint;
  /** Whether at least one rail can carry this amount right now. */
  readonly available: boolean;
  /** The winning rail selected for this tier, or null if all rails are blocked. */
  readonly winningRail: RouteKind | null;
  /** The best route quote, or null if all rails are blocked. */
  readonly bestQuote: RouteQuote | null;
  /** Total amount departing the sender's wallet for the winning route (fee + net). */
  readonly grossAmount: bigint;
  /** Protocol fee charged on this amount tier for the winning route. */
  readonly fee: bigint;
  /** Net token amount crossing the bridge. */
  readonly netAmount: bigint;
  /** Net tokens arriving in destination decimals. */
  readonly destinationAmount: bigint;
  /** Estimated destination gas in units for the winning rail, or null if none available. */
  readonly estimatedDestinationGas: bigint | null;
  /** Gas projections across all rails based on historical constants and overrides. */
  readonly gasProjections: Readonly<Record<RouteKind, bigint>>;
  /** Route quotes for all rails, in canonical route enum order. */
  readonly quotes: readonly RouteQuote[];
}
