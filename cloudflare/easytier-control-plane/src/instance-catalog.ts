import { DurableObject } from "cloudflare:workers";

import type { ContainerInstanceRecord, SyncedInstanceRecord } from "./types";

const INSTANCES_KEY = "instances";

type SyncPayload =
  | {
      instances: ContainerInstanceRecord[];
      syncedAt?: string;
    }
  | ContainerInstanceRecord[];

export class InstanceCatalog extends DurableObject {
  override async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);

      if (url.pathname === "/instances/sync" && request.method === "PUT") {
        return Response.json(
          await this.replaceAll((await request.json()) as SyncPayload),
        );
      }

      if (url.pathname === "/instances" && request.method === "GET") {
        return Response.json(Object.values(await this.loadInstances()));
      }

      if (url.pathname.startsWith("/instances/") && request.method === "GET") {
        const instanceName = decodeURIComponent(
          url.pathname.slice("/instances/".length),
        );
        const record = (await this.loadInstances())[instanceName] ?? null;
        return Response.json(record);
      }

      return Response.json({ error: "route not found" }, { status: 404 });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return Response.json({ error: message }, { status: 400 });
    }
  }

  private async replaceAll(value: SyncPayload): Promise<{
    ok: true;
    count: number;
    syncedAt: string;
  }> {
    const payload = normalizeSyncPayload(value);
    const syncedAt = payload.syncedAt ?? new Date().toISOString();
    const next: Record<string, SyncedInstanceRecord> = {};

    for (const instance of payload.instances) {
      if (!instance.name) {
        continue;
      }

      next[instance.name] = {
        ...instance,
        syncedAt,
      };
    }

    await this.ctx.storage.put(INSTANCES_KEY, next);
    return {
      ok: true,
      count: Object.keys(next).length,
      syncedAt,
    };
  }

  private async loadInstances(): Promise<Record<string, SyncedInstanceRecord>> {
    return (
      (await this.ctx.storage.get<Record<string, SyncedInstanceRecord>>(
        INSTANCES_KEY,
      )) ?? {}
    );
  }
}

function normalizeSyncPayload(value: SyncPayload): {
  instances: ContainerInstanceRecord[];
  syncedAt?: string;
} {
  if (Array.isArray(value)) {
    return { instances: value };
  }

  return {
    instances: value.instances,
    syncedAt: value.syncedAt,
  };
}
