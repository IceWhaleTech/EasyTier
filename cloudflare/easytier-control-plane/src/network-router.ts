import { DurableObject } from "cloudflare:workers";

import type { NetworkRouteRecord, ResolveNetworkRouteResult } from "./types";

const ROUTE_KEY = "route";
const encoder = new TextEncoder();

type ResolveRouteRequest = {
  networkName: string;
  requestedLocationHint?: DurableObjectLocationHint | null;
};

export class NetworkRouter extends DurableObject {
  override async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);

      if (url.pathname === "/route" && request.method === "POST") {
        return Response.json(await this.resolveRoute(await request.json()));
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
    value: unknown,
  ): Promise<ResolveNetworkRouteResult> {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new Error("route payload must be a JSON object");
    }

    const body = value as ResolveRouteRequest;
    const networkName = body.networkName?.trim();
    if (!networkName) {
      throw new Error("networkName is required");
    }

    const existing = await this.ctx.storage.get<NetworkRouteRecord>(ROUTE_KEY);
    if (existing) {
      const updated: NetworkRouteRecord = {
        ...existing,
        lastResolvedAt: new Date().toISOString(),
      };
      await this.ctx.storage.put(ROUTE_KEY, updated);
      return { ...updated, created: false };
    }

    const locationHint = body.requestedLocationHint ?? "enam";
    const route: NetworkRouteRecord = {
      networkName,
      instanceName: await buildInstanceName(networkName, locationHint),
      locationHint,
      createdAt: new Date().toISOString(),
      lastResolvedAt: new Date().toISOString(),
    };
    await this.ctx.storage.put(ROUTE_KEY, route);
    return { ...route, created: true };
  }
}

async function buildInstanceName(
  networkName: string,
  locationHint: DurableObjectLocationHint,
): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", encoder.encode(networkName));
  const hex = [...new Uint8Array(digest)]
    .slice(0, 8)
    .map((item) => item.toString(16).padStart(2, "0"))
    .join("");
  return `net-${locationHint}-${hex}`;
}
