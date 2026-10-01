/**
 * The rails Hyperion routes over, and the three things everybody needs to know about each one.
 *
 * Hyperion owns none of these. Every variant names a rail that is already live, already audited
 * by somebody else, and already carrying real volume, and the numbers below are the same integers
 * `hyperion_core::RouteKind` encodes on Stellar and `RouteKind` encodes in Solidity. An indexer
 * that reads a route off an event on one chain gets the same meaning reading it off the other.
 * Appending is fine. Reordering breaks every record ever written.
 */
export const RouteKind = {
  Cctp: 0,
  AxelarIts: 1,
  AxelarGmp: 2,
  Allbridge: 3,
} as const;

export type RouteKind = (typeof RouteKind)[keyof typeof RouteKind];

export const ROUTE_KINDS: readonly RouteKind[] = [
  RouteKind.Cctp,
  RouteKind.AxelarIts,
  RouteKind.AxelarGmp,
  RouteKind.Allbridge,
];

/** The name a route goes by in config files, URLs and log lines. */
export const ROUTE_SLUGS = {
  [RouteKind.Cctp]: "cctp",
  [RouteKind.AxelarIts]: "axelar-its",
  [RouteKind.AxelarGmp]: "axelar-gmp",
  [RouteKind.Allbridge]: "allbridge",
} as const;

export type RouteSlug = (typeof ROUTE_SLUGS)[RouteKind];

/** What a route is called on screen. */
export const ROUTE_LABELS = {
  [RouteKind.Cctp]: "Circle CCTP V2",
  [RouteKind.AxelarIts]: "Axelar Interchain Token Service",
  [RouteKind.AxelarGmp]: "Axelar General Message Passing",
  [RouteKind.Allbridge]: "Allbridge Core",
} as const;

/**
 * Everything the router and the app both ask about a rail.
 *
 * The mirror of `RouteMeta` in Solidity and the `impl RouteKind` block in Rust. Three booleans
 * rather than a free text description, because each one changes what the interface is allowed to
 * promise.
 */
export interface RouteMeta {
  readonly kind: RouteKind;
  readonly slug: RouteSlug;
  readonly label: string;
  /**
   * Whether the second leg waits on an attestation somebody has to go and fetch. The difference
   * between about fifteen minutes and about fifteen seconds, which is the first thing anybody
   * wants to know and the last thing most bridges tell them.
   */
  readonly waitsOnAttestation: boolean;
  /**
   * Whether the rail moves the asset itself rather than a pooled or wrapped stand in. A canonical
   * route cannot slip, because there is no pool to run thin.
   */
  readonly isCanonical: boolean;
  /**
   * Whether the rail can carry a payload alongside the money. Allbridge cannot, which is why a
   * muxed destination has nowhere to put its sixty four bit sub account id and why the router
   * refuses that combination outright rather than dropping the id.
   */
  readonly carriesPayload: boolean;
  /** One sentence for a tooltip, in the app's own voice. */
  readonly blurb: string;
}

export const ROUTE_META: Readonly<Record<RouteKind, RouteMeta>> = {
  [RouteKind.Cctp]: {
    kind: RouteKind.Cctp,
    slug: "cctp",
    label: ROUTE_LABELS[RouteKind.Cctp],
    waitsOnAttestation: true,
    isCanonical: true,
    carriesPayload: true,
    blurb:
      "Circle burns your USDC here and mints the real thing over there. Nothing wrapped, nothing pooled, and the amount that lands is the amount that left minus fees you saw first.",
  },
  [RouteKind.AxelarIts]: {
    kind: RouteKind.AxelarIts,
    slug: "axelar-its",
    label: ROUTE_LABELS[RouteKind.AxelarIts],
    waitsOnAttestation: true,
    isCanonical: true,
    carriesPayload: true,
    blurb:
      "Axelar's validator set signs off, then the canonical token moves. Works for assets Circle has never heard of, which is most of them.",
  },
  [RouteKind.AxelarGmp]: {
    kind: RouteKind.AxelarGmp,
    slug: "axelar-gmp",
    label: ROUTE_LABELS[RouteKind.AxelarGmp],
    waitsOnAttestation: true,
    isCanonical: false,
    carriesPayload: true,
    blurb:
      "The general purpose version. Carries an instruction rather than just a balance, which is what you want when the transfer is the beginning of something.",
  },
  [RouteKind.Allbridge]: {
    kind: RouteKind.Allbridge,
    slug: "allbridge",
    label: ROUTE_LABELS[RouteKind.Allbridge],
    waitsOnAttestation: false,
    isCanonical: false,
    carriesPayload: false,
    blurb:
      "A pool on each side, so there is nothing to wait for. You pay for the speed in slippage, and the quote tells you exactly how much before you sign.",
  },
};

const SLUG_TO_KIND: ReadonlyMap<string, RouteKind> = new Map(
  ROUTE_KINDS.map((kind) => [ROUTE_SLUGS[kind], kind]),
);

/** Read a route off the wire, refusing a tag no version of Hyperion has ever written. */
export function routeFromTag(tag: number): RouteKind {
  if (!isRouteKind(tag)) {
    throw new RangeError(`no Hyperion route is numbered ${String(tag)}`);
  }
  return tag;
}

/** Read a route out of a URL or a config file. */
export function routeFromSlug(slug: string): RouteKind {
  const kind = SLUG_TO_KIND.get(slug);
  if (kind === undefined) {
    throw new RangeError(`no Hyperion route is called ${slug}`);
  }
  return kind;
}

/**
 * The same lookup for callers reading a file somebody else wrote.
 *
 * A deployment record with a typo in a rail name should fail with the path to the field, not with
 * a RangeError thrown three frames down, so the validator needs a version that answers rather
 * than throws.
 */
export function tryRouteFromSlug(slug: string): RouteKind | null {
  return SLUG_TO_KIND.get(slug) ?? null;
}

export function isRouteKind(value: number): value is RouteKind {
  return Number.isInteger(value) && value >= RouteKind.Cctp && value <= RouteKind.Allbridge;
}

export function routeMeta(route: RouteKind): RouteMeta {
  return ROUTE_META[route];
}

export function waitsOnAttestation(route: RouteKind): boolean {
  return ROUTE_META[route].waitsOnAttestation;
}

export function isCanonical(route: RouteKind): boolean {
  return ROUTE_META[route].isCanonical;
}

export function carriesPayload(route: RouteKind): boolean {
  return ROUTE_META[route].carriesPayload;
}
