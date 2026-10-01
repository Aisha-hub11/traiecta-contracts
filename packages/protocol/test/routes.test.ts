import { describe, expect, it } from "vitest";
import {
  ROUTE_KINDS,
  ROUTE_LABELS,
  ROUTE_META,
  ROUTE_SLUGS,
  RouteKind,
  carriesPayload,
  isCanonical,
  isRouteKind,
  routeFromSlug,
  routeFromTag,
  routeMeta,
  tryRouteFromSlug,
  waitsOnAttestation,
} from "../src/routes.js";

describe("the tag values", () => {
  it("are the integers both chains encode", () => {
    // These travel in events. An indexer reading a route off a Stellar event and off an EVM log
    // has to get the same rail, so the numbers are fixed rather than cosmetic.
    expect(RouteKind.Cctp).toBe(0);
    expect(RouteKind.AxelarIts).toBe(1);
    expect(RouteKind.AxelarGmp).toBe(2);
    expect(RouteKind.Allbridge).toBe(3);
  });

  it("round trip through their tags", () => {
    for (const kind of ROUTE_KINDS) {
      expect(routeFromTag(kind)).toBe(kind);
    }
  });

  it("refuse a tag from a deployment newer than this build", () => {
    expect(() => routeFromTag(4)).toThrow();
    expect(() => routeFromTag(-1)).toThrow();
    expect(isRouteKind(4)).toBe(false);
  });
});

describe("slugs", () => {
  it("round trip", () => {
    for (const kind of ROUTE_KINDS) {
      expect(routeFromSlug(ROUTE_SLUGS[kind])).toBe(kind);
    }
  });

  it("are url safe, because they end up in url paths", () => {
    for (const kind of ROUTE_KINDS) {
      expect(ROUTE_SLUGS[kind]).toMatch(/^[a-z][a-z-]*[a-z]$/);
    }
  });

  it("throw on an unknown slug, or answer null when asked the other way", () => {
    expect(() => routeFromSlug("teleport")).toThrow();
    expect(tryRouteFromSlug("teleport")).toBeNull();
    expect(tryRouteFromSlug("cctp")).toBe(RouteKind.Cctp);
  });
});

describe("what is true about each rail", () => {
  it("has a label and a blurb for every one", () => {
    for (const kind of ROUTE_KINDS) {
      expect(ROUTE_LABELS[kind].length).toBeGreaterThan(0);
      expect(ROUTE_META[kind].blurb.length).toBeGreaterThan(0);
      expect(routeMeta(kind).slug).toBe(ROUTE_SLUGS[kind]);
    }
  });

  it("waits on an attestation everywhere except Allbridge", () => {
    expect(waitsOnAttestation(RouteKind.Cctp)).toBe(true);
    expect(waitsOnAttestation(RouteKind.AxelarIts)).toBe(true);
    expect(waitsOnAttestation(RouteKind.AxelarGmp)).toBe(true);
    expect(waitsOnAttestation(RouteKind.Allbridge)).toBe(false);
  });

  it("is canonical only where the asset itself moves", () => {
    // CCTP burns and mints real USDC. ITS moves the registered token. The other two hand over
    // something that represents the asset, which is a different promise.
    expect(isCanonical(RouteKind.Cctp)).toBe(true);
    expect(isCanonical(RouteKind.AxelarIts)).toBe(true);
    expect(isCanonical(RouteKind.AxelarGmp)).toBe(false);
    expect(isCanonical(RouteKind.Allbridge)).toBe(false);
  });

  it("carries a payload everywhere except Allbridge", () => {
    // This is the one that decides whether a muxed address can be paid, because the sub account
    // id has nowhere to travel without a payload.
    expect(carriesPayload(RouteKind.Cctp)).toBe(true);
    expect(carriesPayload(RouteKind.AxelarIts)).toBe(true);
    expect(carriesPayload(RouteKind.AxelarGmp)).toBe(true);
    expect(carriesPayload(RouteKind.Allbridge)).toBe(false);
  });
});
