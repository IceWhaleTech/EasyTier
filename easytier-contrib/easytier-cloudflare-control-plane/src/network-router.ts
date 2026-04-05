import { DurableObject } from "cloudflare:workers";

import { SHARED_INGRESS_POOL_SIZE } from "./constants";
import type { NetworkRouteMode, NetworkRouteRecord, ResolveNetworkRouteResult } from "./types/index";

const ROUTE_KEY = "route";
const encoder = new TextEncoder();

type ResolveRouteRequest = {
  networkName: string;
  routeKey: string;
  routeMode: NetworkRouteMode;
  secretDigestHex?: string | null;
  requestedLocationHint?: DurableObjectLocationHint | null;
};

export class NetworkRouter extends DurableObject {
  override async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);

      if (url.pathname === "/route" && request.method === "POST") {
        return Response.json(await this.resolveRoute(await request.json() as ResolveRouteRequest));
      }

      if (url.pathname === "/route" && request.method === "GET") {
        const route = await this.ctx.storage.get<NetworkRouteRecord>(ROUTE_KEY);
        return route
          ? Response.json(route)
          : Response.json({ error: "route not found" }, { status: 404 });
      }

      if (url.pathname === "/route" && request.method === "DELETE") {
        await this.ctx.storage.delete(ROUTE_KEY);
        return Response.json({ ok: true });
      }

      return Response.json({ error: "route not found" }, { status: 404 });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return Response.json({ error: message }, { status: 400 });
    }
  }

  private async resolveRoute(
    body: ResolveRouteRequest,
  ): Promise<ResolveNetworkRouteResult> {
    const networkName = body.networkName.trim();
    const routeKey = body.routeKey.trim();

    if (!networkName) {
      throw new Error("networkName is required");
    }
    if (!routeKey) {
      throw new Error("routeKey is required");
    }

    const existing = await this.ctx.storage.get<NetworkRouteRecord>(ROUTE_KEY);
    const locationHint = body.requestedLocationHint ?? "enam";
    const desiredRoute: NetworkRouteRecord = {
      networkName,
      routeKey,
      routeMode: body.routeMode,
      secretDigestHex: body.secretDigestHex ?? null,
      instanceName: await buildInstanceName(routeKey, locationHint),
      locationHint,
      createdAt: existing?.createdAt ?? new Date().toISOString(),
      lastResolvedAt: new Date().toISOString(),
    };

    if (existing) {
      await this.ctx.storage.put(ROUTE_KEY, desiredRoute);
      return { ...desiredRoute, created: false };
    }

    await this.ctx.storage.put(ROUTE_KEY, desiredRoute);
    return { ...desiredRoute, created: true };
  }
}

async function buildInstanceName(
  routeKey: string,
  locationHint: DurableObjectLocationHint,
): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", encoder.encode(routeKey));
  const bytes = new Uint8Array(digest);
  const bucket = bytes[0] % SHARED_INGRESS_POOL_SIZE;
  return `ingress-${locationHint}-${bucket}`;
}
