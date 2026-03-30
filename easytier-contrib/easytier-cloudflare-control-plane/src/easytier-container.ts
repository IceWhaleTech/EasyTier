import { Container } from "@cloudflare/containers";

import { INTERNAL_WS_PORT } from "./constants";
import { buildEasyTierArgs, parseInstanceConfig, RequestError } from "./config";
import type { InstanceConfig, StoredState } from "./types";

export class EasyTierContainer extends Container {
  defaultPort = INTERNAL_WS_PORT;
  sleepAfter = "10m";

  override async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      const route = `${request.method} ${url.pathname}`;

      if (route === "PUT /control/config") {
        const config = parseInstanceConfig(await request.json());
        await this.ctx.storage.put("config", config);
        await this.ctx.storage.put("updatedAt", new Date().toISOString());
        return Response.json({ ok: true, config });
      }

      if (route === "DELETE /control/config") {
        await this.stopInstance();
        await this.ctx.storage.delete("config");
        await this.ctx.storage.put("updatedAt", new Date().toISOString());
        return Response.json({ ok: true });
      }

      if (route === "GET /control/status") {
        return Response.json(await this.readState());
      }

      if (route === "POST /control/start") {
        const config = await this.requireConfig();
        await this.ensureStarted(config);
        return Response.json(await this.readState());
      }

      if (route === "POST /control/stop") {
        await this.stopInstance();
        return Response.json(await this.readState());
      }

      const config = await this.requireConfig();
      await this.ensureStarted(config);
      return super.fetch(request);
    } catch (error) {
      return toErrorResponse(error);
    }
  }

  private async stopInstance(): Promise<void> {
    await this.stop().catch((error) => {
      console.warn("failed to stop container", error);
    });
    await this.ctx.storage.put("lastStopAt", new Date().toISOString());
  }

  private async ensureStarted(config: InstanceConfig): Promise<void> {
    const args = buildEasyTierArgs(config);
    await this.startAndWaitForPorts({
      startOptions: {
        entrypoint: ["/usr/local/bin/easytier-core", ...args],
        envVars: {
          ...(config.env ?? {}),
          ET_INSTANCE_NAME: config.instanceName ?? "",
          ET_NETWORK_NAME: config.networkName ?? "",
        },
      },
    });
    await this.ctx.storage.put("lastStartAt", new Date().toISOString());
  }

  private async requireConfig(): Promise<InstanceConfig> {
    const config = await this.ctx.storage.get<InstanceConfig>("config");
    if (!config) {
      throw new RequestError(
        404,
        "instance config not found; configure the instance first",
      );
    }
    return config;
  }

  private async readState(): Promise<StoredState> {
    const [config, updatedAt, lastStartAt, lastStopAt, runtime] =
      await Promise.all([
        this.ctx.storage.get<InstanceConfig>("config"),
        this.ctx.storage.get<string>("updatedAt"),
        this.ctx.storage.get<string>("lastStartAt"),
        this.ctx.storage.get<string>("lastStopAt"),
        this.getState().catch(() => null),
      ]);

    return {
      config: config ?? null,
      updatedAt: updatedAt ?? null,
      lastStartAt: lastStartAt ?? null,
      lastStopAt: lastStopAt ?? null,
      runtime: runtime
        ? {
            status: runtime.status,
            lastChange: runtime.lastChange,
            exitCode: "exitCode" in runtime ? runtime.exitCode : undefined,
          }
        : null,
    };
  }
}

function toErrorResponse(error: unknown): Response {
  const status = error instanceof RequestError ? error.status : 500;
  const message = error instanceof Error ? error.message : String(error);

  if (status >= 500) {
    console.error(
      JSON.stringify({
        message: "container request failed",
        error: message,
      }),
    );
  }

  return Response.json({ error: message }, { status });
}
