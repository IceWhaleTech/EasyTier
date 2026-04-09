import { Container } from "@cloudflare/containers";
import { Hono } from "hono";

import { buildEasyTierArgs, parseInstanceConfig, RequestError } from "./config";
import { INTERNAL_WS_PORT } from "./constants";
import type {
  ConfigRecord,
  ErrorLike,
  InstanceConfig,
  StoredState,
  WebSocketData,
} from "./types/index";

export class EasyTierContainer extends Container {
  defaultPort = INTERNAL_WS_PORT;
  requiredPorts = [INTERNAL_WS_PORT];
  sleepAfter = "10m";
  enableInternet = false;
  private readonly controlApp = this.createControlApp();

  override async fetch(request: Request): Promise<Response> {
    try {
      if (isControlRequest(request)) {
        return this.controlApp.fetch(request);
      }

      const config = await this.requireConfig();
      await this.ensureStarted(config);
      if (isWebSocketRequest(request)) {
        return this.proxyWebSocketRequest(request);
      }
      return super.fetch(request);
    } catch (error) {
      return toErrorResponse(error as ErrorLike);
    }
  }

  private createControlApp(): Hono {
    const app = new Hono();

    app.onError((error) => toErrorResponse(error as ErrorLike));
    app.notFound(() =>
      Response.json({ error: "route not found" }, { status: 404 }),
    );

    app.put("/control/config", (c) => this.handleConfigUpdate(c.req.raw));
    app.delete("/control/config", () => this.handleConfigDelete());
    app.post("/control/observed-handshake", (c) =>
      this.handleObservedHandshake(c.req.raw),
    );
    app.get("/control/status", () => this.handleStatus());
    app.post("/control/start", () => this.handleStart());
    app.post("/control/stop", () => this.handleStop());

    return app;
  }

  private async handleConfigUpdate(request: Request): Promise<Response> {
    const config = parseInstanceConfig(
      (await request.json()) as ConfigRecord,
    );
    const previousConfig = await this.ctx.storage.get<InstanceConfig>("config");
    await this.ctx.storage.put("config", config);
    await this.ctx.storage.put("updatedAt", new Date().toISOString());
    if (JSON.stringify(previousConfig ?? null) !== JSON.stringify(config)) {
      await this.stopInstance();
    }
    return Response.json({ ok: true, config });
  }

  private async handleConfigDelete(): Promise<Response> {
    await this.stopInstance();
    await this.ctx.storage.delete("config");
    await this.ctx.storage.put("updatedAt", new Date().toISOString());
    return Response.json({ ok: true });
  }

  private async handleObservedHandshake(request: Request): Promise<Response> {
    const payload = (await request.json()) as { networkName?: string };
    if (!payload.networkName) {
      throw new RequestError(400, "networkName is required");
    }
    const now = new Date().toISOString();
    await this.ctx.storage.put("lastHandshakeAt", now);
    await this.ctx.storage.put(
      "lastHandshakeNetworkName",
      payload.networkName,
    );
    return Response.json({ ok: true, observedAt: now });
  }

  private async handleStatus(): Promise<Response> {
    return Response.json(await this.readState());
  }

  private async handleStart(): Promise<Response> {
    const config = await this.requireConfig();
    await this.ensureStarted(config);
    return Response.json(await this.readState());
  }

  private async handleStop(): Promise<Response> {
    await this.stopInstance();
    return Response.json(await this.readState());
  }

  private async proxyWebSocketRequest(request: Request): Promise<Response> {
    const response = await this.ctx.container!.getTcpPort(INTERNAL_WS_PORT).fetch(
      request.url.replace("https:", "http:"),
      request,
    );

    if (response.status !== 101 || !response.webSocket) {
      return response;
    }

    const containerSocket = response.webSocket;
    const pair = new WebSocketPair();
    const clientSocket = pair[0];
    const proxySocket = pair[1];

    setBinaryType(containerSocket);
    setBinaryType(proxySocket);
    containerSocket.accept();
    proxySocket.accept();

    proxySocket.addEventListener("message", (event) => {
      void forwardMessage(containerSocket, event.data, proxySocket);
    });
    containerSocket.addEventListener("message", (event) => {
      void forwardMessage(proxySocket, event.data, containerSocket);
    });

    proxySocket.addEventListener("close", (event) => {
      closeSocket(containerSocket, normalizeCloseCode(event.code), event.reason);
    });
    containerSocket.addEventListener("close", (event) => {
      closeSocket(proxySocket, normalizeCloseCode(event.code), event.reason);
    });

    proxySocket.addEventListener("error", () => {
      closeSocket(containerSocket, 1011, "client websocket error");
    });
    containerSocket.addEventListener("error", () => {
      closeSocket(proxySocket, 1011, "container websocket error");
    });

    return new Response(null, {
      status: 101,
      webSocket: clientSocket,
    });
  }

  private async stopInstance(): Promise<void> {
    await this.stop().catch((error) => {
      console.warn("failed to stop container", error);
    });
    await this.ctx.storage.put("lastStopAt", new Date().toISOString());
  }

  private async ensureStarted(config: InstanceConfig): Promise<void> {
    const runtime = await this.getState().catch(() => null);
    if (runtime?.status === "running") {
      return;
    }

    const startOptions = {
      entrypoint: ["/usr/local/bin/easytier-core", ...buildEasyTierArgs(config)],
      envVars: {
        ...(config.env ?? {}),
        ET_INSTANCE_NAME: config.instanceName ?? "",
        ET_NETWORK_NAME: config.networkName ?? "",
      },
    };

    try {
      await this.startAndWaitForPorts({
        ports: [INTERNAL_WS_PORT],
        startOptions,
      });
    } catch (error) {
      // Local `wrangler dev` on OrbStack currently misreports the port-ready check
      // for this image, even when the process has already entered `running`.
      if (
        error instanceof Error &&
        error.message.includes("Network connection lost")
      ) {
        await this.start(startOptions);
      } else {
        throw error;
      }
    }
    await this.ctx.storage.put("lastStartAt", new Date().toISOString());
  }

  private async requireConfig(): Promise<InstanceConfig> {
    const config = await this.ctx.storage.get<InstanceConfig>("config");
    if (!config) {
      throw new RequestError(
        404,
        "instance config not found",
      );
    }
    return config;
  }

  private async readState(): Promise<StoredState> {
    const [
      config,
      updatedAt,
      lastStartAt,
      lastStopAt,
      lastHandshakeAt,
      lastHandshakeNetworkName,
      runtime,
    ] = await Promise.all([
      this.ctx.storage.get<InstanceConfig>("config"),
      this.ctx.storage.get<string>("updatedAt"),
      this.ctx.storage.get<string>("lastStartAt"),
      this.ctx.storage.get<string>("lastStopAt"),
      this.ctx.storage.get<string>("lastHandshakeAt"),
      this.ctx.storage.get<string>("lastHandshakeNetworkName"),
      this.getState().catch(() => null),
    ]);

    return {
      config: config ?? null,
      updatedAt: updatedAt ?? null,
      lastStartAt: lastStartAt ?? null,
      lastStopAt: lastStopAt ?? null,
      lastHandshakeAt: lastHandshakeAt ?? null,
      lastHandshakeNetworkName: lastHandshakeNetworkName ?? null,
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

function isControlRequest(request: Request): boolean {
  return new URL(request.url).pathname.startsWith("/control/");
}

function isWebSocketRequest(request: Request): boolean {
  return request.headers.get("Upgrade")?.toLowerCase() === "websocket";
}

async function forwardMessage(
  target: WebSocket,
  data: WebSocketData,
  peer: WebSocket,
): Promise<void> {
  try {
    if (isBlobLike(data)) {
      target.send(new Uint8Array(await data.arrayBuffer()));
      return;
    }

    if (data instanceof ArrayBuffer) {
      target.send(new Uint8Array(data));
      return;
    }

    if (ArrayBuffer.isView(data) || typeof data === "string") {
      target.send(data);
      return;
    }

    throw new Error(`unsupported websocket payload: ${typeof data}`);
  } catch (error) {
    console.error("failed to proxy container websocket message", error);
    closeSocket(target, 1011, "websocket proxy send failed");
    closeSocket(peer, 1011, "websocket proxy send failed");
  }
}

function setBinaryType(socket: WebSocket): void {
  try {
    (
      socket as WebSocket & {
        binaryType?: "blob" | "arraybuffer";
      }
    ).binaryType = "arraybuffer";
  } catch {
    // Ignore runtimes that don't expose binaryType.
  }
}

function isBlobLike(value: WebSocketData): value is Blob {
  return value instanceof Blob;
}

function normalizeCloseCode(code: number): number {
  return code === 1005 || code === 1006 ? 1000 : code;
}

function closeSocket(socket: WebSocket, code = 1000, reason = "closed"): void {
  try {
    socket.close(code, reason.slice(0, 123));
  } catch {
    // Ignore invalid-state close attempts during teardown.
  }
}

function toErrorResponse(error: ErrorLike): Response {
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
