import { Hono } from "hono";
import type { Context } from "hono";
import { HTTPException } from "hono/http-exception";

import { buildSharedRelayConfig, formatConfigExample } from "./config";
import { DEFAULT_INSTANCE, INITIAL_MESSAGE_TIMEOUT_MS } from "./constants";
import { EasyTierContainer } from "./easytier-container";
import {
  extractNetworkNameFromFrame,
  type Frame,
} from "./easytier-proto";
import {
  resolveInstanceCatalogView,
  syncInstanceCatalog,
  syncInstanceCatalogFromCloudflare,
} from "./instance-catalog-sync";
import { InstanceCatalog } from "./instance-catalog";
import { NetworkRouter } from "./network-router";
import type {
  NetworkRouteRecord,
  ResolveNetworkRouteResult,
  StoredState,
} from "./types";

type WorkerEnv = Cloudflare.Env & {
  API_AUTH_TOKEN?: string;
  CF_CONTAINERS_API_TOKEN?: string;
  CF_API_TOKEN?: string;
  CLOUDFLARE_API_TOKEN?: string;
};

type AppContext = {
  Bindings: WorkerEnv;
};

const app = new Hono<AppContext>();
const encoder = new TextEncoder();
const subtle = crypto.subtle as SubtleCrypto & {
  timingSafeEqual(
    left: ArrayBuffer | ArrayBufferView,
    right: ArrayBuffer | ArrayBufferView,
  ): boolean;
};

app.onError((error, c) => {
  if (error instanceof HTTPException) {
    return c.json({ error: error.message }, error.status);
  }

  const message = error instanceof Error ? error.message : String(error);
  console.error(
    JSON.stringify({
      message: "worker request failed",
      error: message,
      path: c.req.path,
      method: c.req.method,
    }),
  );
  return c.json({ error: "Internal server error" }, 500);
});

app.notFound((c) => c.json({ error: "route not found" }, 404));

app.use("/api/*", async (c, next) => {
  const expectedToken = c.env.API_AUTH_TOKEN;
  if (!expectedToken) {
    await next();
    return;
  }

  const authorization = c.req.header("authorization");
  if (!authorization?.startsWith("Bearer ")) {
    throw new HTTPException(401, { message: "missing bearer token" });
  }

  const actualToken = authorization.slice("Bearer ".length);
  if (!timingSafeEqual(actualToken, expectedToken)) {
    throw new HTTPException(403, { message: "invalid bearer token" });
  }

  await next();
});

app.get("/", landingPage);
app.get("/index.html", landingPage);

app.get("/api/config-example", (c) => c.json(formatConfigExample()));
app.put("/api/admin/instances/sync", async (c) => {
  const payload = await c.req.json();
  return syncInstanceCatalog(c.env, payload);
});
app.get("/api/instance", (c) =>
  sendControlRequest(c, "/control/status", "GET"),
);
app.put("/api/instance", (c) => proxyToContainer(c, "/control/config"));
app.delete("/api/instance", (c) =>
  sendControlRequest(c, "/control/config", "DELETE"),
);
app.post("/api/instance/start", (c) =>
  sendControlRequest(c, "/control/start", "POST"),
);
app.post("/api/instance/stop", (c) =>
  sendControlRequest(c, "/control/stop", "POST"),
);

app.get("/api/routes/:networkName", (c) =>
  forwardRouteRequest(c.env, c.req.param("networkName"), "GET"),
);
app.delete("/api/routes/:networkName", (c) =>
  forwardRouteRequest(c.env, c.req.param("networkName"), "DELETE"),
);
app.get("/api/routes/:networkName/instance", async (c) => {
  const route = await getNetworkRoute(c.env, c.req.param("networkName"));
  return sendContainerControlRequest(
    c.env,
    route.instanceName,
    route.locationHint,
    "/control/status",
    "GET",
  );
});
app.get("/api/routes/details/:networkName", async (c) => {
  const networkName = c.req.param("networkName");
  const route = await getNetworkRoute(c.env, networkName);
  const routeView = {
    networkName: route.networkName,
    instanceName: route.instanceName,
    locationHint: route.locationHint,
    createdAt: route.createdAt,
    lastResolvedAt: route.lastResolvedAt,
  };
  const [runtime, instance] = await Promise.all([
    sendContainerControlRequest(
      c.env,
      route.instanceName,
      route.locationHint,
      "/control/status",
      "GET",
    )
      .then((response) => readJsonResponse<StoredState>(response))
      .catch((error) => ({
        error: error instanceof Error ? error.message : String(error),
      })),
    resolveInstanceCatalogView(c.env, route.instanceName),
  ]);

  return c.json({
    networkName,
    client: getClientRequestInfo(c.req.raw),
    route: routeView,
    runtime,
    instance,
  });
});

app.all("/connect", (c) =>
  isWebSocketRequest(c.req.raw)
    ? acceptRoutedWebSocket(c)
    : c.json({ error: "websocket upgrade required" }, 426),
);

export { EasyTierContainer, InstanceCatalog, NetworkRouter };
export default {
  fetch(request, env, executionCtx) {
    return app.fetch(request, env, executionCtx);
  },
  scheduled(_controller, env, executionCtx) {
    executionCtx.waitUntil(
      syncInstanceCatalogFromCloudflare(env).catch((error: unknown) => {
        console.error(
          JSON.stringify({
            message: "scheduled instance catalog sync failed",
            error: error instanceof Error ? error.message : String(error),
          }),
        );
      }),
    );
  },
} satisfies ExportedHandler<WorkerEnv>;

function landingPage(c: Context<AppContext>): Response | Promise<Response> {
  if (isWebSocketRequest(c.req.raw)) {
    return acceptRoutedWebSocket(c);
  }

  const url = new URL(c.req.url);
  const wsProtocol = url.protocol === "http:" ? "ws:" : "wss:";
  const websocketUrl = `${wsProtocol}//${url.host}/connect`;

  return c.html(`<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>EasyTier Control Plane</title>
    <style>
      :root {
        color-scheme: light;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }
      body {
        margin: 0;
        background: #f6f3ee;
        color: #1f2a37;
      }
      main {
        max-width: 720px;
        margin: 0 auto;
        padding: 48px 24px 64px;
      }
      h1 {
        margin: 0 0 16px;
        font-size: 2.25rem;
      }
      p {
        line-height: 1.6;
      }
      code {
        background: #fff;
        border-radius: 6px;
        padding: 2px 6px;
      }
      .panel {
        margin-top: 24px;
        padding: 20px;
        background: #fffdf8;
        border: 1px solid #e7dbc7;
        border-radius: 14px;
      }
      ul {
        line-height: 1.8;
        padding-left: 20px;
      }
    </style>
  </head>
  <body>
    <main>
      <h1>EasyTier Control Plane</h1>
      <p>A Hono + Cloudflare Containers worker that sticky-routes each EasyTier network to one relay instance.</p>
      <div class="panel">
        <p>WebSocket endpoint: <code>${websocketUrl}</code></p>
      </div>
      <div class="panel">
        <strong>Routing model</strong>
        <ul>
          <li><code>et.icewhale.io</code> stays as the only client endpoint</li>
          <li>The first EasyTier websocket frame is inspected for <code>network_name</code></li>
          <li>Each <code>network_name</code> is pinned to one relay container instance</li>
        </ul>
      </div>
      <div class="panel">
        <strong>Control API</strong>
        <ul>
          <li><code>GET /api/instance</code></li>
          <li><code>PUT /api/instance</code></li>
          <li><code>POST /api/instance/start</code></li>
          <li><code>POST /api/instance/stop</code></li>
          <li><code>DELETE /api/instance</code></li>
          <li><code>GET /api/routes/:networkName</code></li>
        </ul>
      </div>
    </main>
  </body>
</html>`);
}

function getContainerStub(c: Context<AppContext>) {
  const id = c.env.EASYTIER_CONTAINER.idFromName(DEFAULT_INSTANCE);
  return c.env.EASYTIER_CONTAINER.get(id);
}

function proxyToContainer(
  c: Context<AppContext>,
  pathname = "/",
): Promise<Response> {
  return getContainerStub(c).fetch(
    new Request(buildInternalUrl(pathname), c.req.raw),
  );
}

function sendControlRequest(
  c: Context<AppContext>,
  pathname: string,
  method: "GET" | "POST" | "DELETE",
): Promise<Response> {
  return getContainerStub(c).fetch(
    new Request(buildInternalUrl(pathname), { method }),
  );
}

function buildInternalUrl(pathname: string): string {
  return `https://internal${pathname}`;
}

function isWebSocketRequest(request: Request): boolean {
  return request.headers.get("Upgrade")?.toLowerCase() === "websocket";
}

function timingSafeEqual(left: string, right: string): boolean {
  const leftBytes = encoder.encode(left);
  const rightBytes = encoder.encode(right);

  if (leftBytes.byteLength !== rightBytes.byteLength) {
    return false;
  }

  return subtle.timingSafeEqual(leftBytes, rightBytes);
}

function acceptRoutedWebSocket(c: Context<AppContext>): Response {
  const pair = new WebSocketPair();
  const clientSocket = pair[0];
  const workerSocket = pair[1];

  workerSocket.accept();
  c.executionCtx.waitUntil(
    handleRoutedWebSocket(workerSocket, c.req.raw, c.env),
  );

  return new Response(null, {
    status: 101,
    webSocket: clientSocket,
  });
}

async function handleRoutedWebSocket(
  clientSocket: WebSocket,
  request: Request,
  env: WorkerEnv,
): Promise<void> {
  let upstreamSocket: WebSocket | null = null;
  let clientClosed = false;
  let firstMessageResolved = false;
  const pendingMessages: Frame[] = [];

  let resolveFirstMessage!: (data: Frame) => void;
  let rejectFirstMessage!: (reason?: unknown) => void;

  const firstMessage = new Promise<Frame>(
    (resolve, reject) => {
      resolveFirstMessage = resolve;
      rejectFirstMessage = reject;
    },
  );

  const timeout = setTimeout(() => {
    if (!firstMessageResolved) {
      rejectFirstMessage(
        new Error("timed out waiting for first EasyTier frame"),
      );
    }
  }, INITIAL_MESSAGE_TIMEOUT_MS);

  clientSocket.addEventListener("message", (event) => {
    const data = event.data as Frame;

    if (!upstreamSocket) {
      pendingMessages.push(data);
      if (!firstMessageResolved) {
        firstMessageResolved = true;
        clearTimeout(timeout);
        resolveFirstMessage(data);
      }
      return;
    }

    void forwardToUpstream(upstreamSocket, data, clientSocket);
  });

  clientSocket.addEventListener("close", (event) => {
    clientClosed = true;
    clearTimeout(timeout);
    if (!firstMessageResolved) {
      rejectFirstMessage(new Error("client closed before routing completed"));
    }
    if (upstreamSocket) {
      closeSocket(upstreamSocket, event.code, event.reason);
    }
  });

  clientSocket.addEventListener("error", () => {
    clearTimeout(timeout);
    if (!firstMessageResolved) {
      rejectFirstMessage(
        new Error("client websocket failed before first frame"),
      );
    }
    if (upstreamSocket) {
      closeSocket(upstreamSocket, 1011, "client websocket error");
    }
  });

  try {
    const firstFrame = await firstMessage;
    const networkName = await extractNetworkNameFromFrame(firstFrame);
    const route = await resolveNetworkRoute(
      env,
      networkName,
      pickLocationHint(request),
    );

    if (route.created) {
      await configureSharedRelayContainer(env, route);
    }

    if (clientClosed) {
      return;
    }

    upstreamSocket = await openContainerSocket(request, env, route);
    const activeUpstream = upstreamSocket;
    activeUpstream.addEventListener("message", (event) => {
      void forwardToClient(clientSocket, event.data, activeUpstream);
    });
    activeUpstream.addEventListener("close", (event) => {
      closeSocket(clientSocket, event.code, event.reason);
    });
    activeUpstream.addEventListener("error", () => {
      closeSocket(clientSocket, 1011, "upstream websocket error");
    });

    for (const message of pendingMessages) {
      await sendNormalized(activeUpstream, message);
    }
    pendingMessages.length = 0;
  } catch (error) {
    console.error(
      JSON.stringify({
        message: "failed to route websocket by network_name",
        error: error instanceof Error ? error.message : String(error),
        path: new URL(request.url).pathname,
      }),
    );
    closeSocket(clientSocket, 1011, "routing failed");
  } finally {
    clearTimeout(timeout);
  }
}

async function resolveNetworkRoute(
  env: WorkerEnv,
  networkName: string,
  requestedLocationHint: DurableObjectLocationHint,
): Promise<ResolveNetworkRouteResult> {
  const response = await env.NETWORK_ROUTER.getByName(networkName).fetch(
    new Request(buildInternalUrl("/route"), {
      method: "POST",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify({
        networkName,
        requestedLocationHint,
      }),
    }),
  );
  return readJsonResponse<ResolveNetworkRouteResult>(response);
}

async function getNetworkRoute(
  env: WorkerEnv,
  networkName: string,
): Promise<NetworkRouteRecord> {
  const response = await forwardRouteRequest(env, networkName, "GET");
  return readJsonResponse<NetworkRouteRecord>(response);
}

function forwardRouteRequest(
  env: WorkerEnv,
  networkName: string,
  method: "GET" | "DELETE",
): Promise<Response> {
  return env.NETWORK_ROUTER.getByName(networkName).fetch(
    new Request(buildInternalUrl("/route"), { method }),
  );
}

async function configureSharedRelayContainer(
  env: WorkerEnv,
  route: NetworkRouteRecord,
): Promise<void> {
  const config = buildSharedRelayConfig(route.instanceName);
  await sendContainerControlRequest(
    env,
    route.instanceName,
    route.locationHint,
    "/control/config",
    "PUT",
    config,
  );
}

async function openContainerSocket(
  request: Request,
  env: WorkerEnv,
  route: NetworkRouteRecord,
): Promise<WebSocket> {
  const response = await getRoutedContainerStub(
    env,
    route.instanceName,
    route.locationHint,
  ).fetch(new Request(buildInternalUrl("/"), request));

  if (response.status !== 101 || !response.webSocket) {
    throw new Error(
      `container websocket upgrade failed with status ${response.status}`,
    );
  }

  response.webSocket.accept();
  return response.webSocket;
}

function getRoutedContainerStub(
  env: WorkerEnv,
  instanceName: string,
  locationHint: DurableObjectLocationHint,
) {
  return env.EASYTIER_CONTAINER.getByName(instanceName, { locationHint });
}

async function sendContainerControlRequest(
  env: WorkerEnv,
  instanceName: string,
  locationHint: DurableObjectLocationHint,
  pathname: string,
  method: "GET" | "POST" | "PUT" | "DELETE",
  body?: unknown,
): Promise<Response> {
  return getRoutedContainerStub(env, instanceName, locationHint).fetch(
    new Request(buildInternalUrl(pathname), {
      method,
      headers:
        body === undefined
          ? undefined
          : {
              "content-type": "application/json",
            },
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
  );
}

async function readJsonResponse<T>(response: Response): Promise<T> {
  const data = (await response.json()) as T & { error?: string };
  if (!response.ok) {
    throw new Error(
      typeof data === "object" && data && "error" in data && data.error
        ? String(data.error)
        : `request failed with status ${response.status}`,
    );
  }
  return data;
}

function getClientRequestInfo(request: Request) {
  const cf = request.cf as IncomingRequestCfProperties | undefined;

  return {
    ip:
      request.headers.get("cf-connecting-ip") ??
      request.headers.get("x-real-ip") ??
      null,
    colo: cf?.colo ?? null,
    country: cf?.country ?? null,
    city: cf?.city ?? null,
    region: cf?.region ?? null,
    regionCode: cf?.regionCode ?? null,
    continent: cf?.continent ?? null,
    timezone: cf?.timezone ?? null,
    latitude: cf?.latitude ?? null,
    longitude: cf?.longitude ?? null,
    asn: cf?.asn ?? null,
    asOrganization: cf?.asOrganization ?? null,
  };
}

function pickLocationHint(request: Request): DurableObjectLocationHint {
  const continent = (
    request.cf as IncomingRequestCfProperties | undefined
  )?.continent?.toUpperCase();

  switch (continent) {
    case "SA":
      return "sam";
    case "EU":
    case "AF":
      return "weur";
    case "AS":
    case "OC":
    case "ME":
      return "apac";
    case "NA":
    default:
      return "enam";
  }
}

function closeSocket(socket: WebSocket, code = 1000, reason = "closed"): void {
  try {
    socket.close(code, reason.slice(0, 123));
  } catch {
    // Ignore invalid-state close attempts during teardown.
  }
}

async function forwardToUpstream(
  upstreamSocket: WebSocket,
  data: string | ArrayBuffer | ArrayBufferView | Blob,
  clientSocket: WebSocket,
): Promise<void> {
  try {
    await sendNormalized(upstreamSocket, data);
  } catch (error) {
    console.error("failed to forward client websocket message", error);
    closeSocket(clientSocket, 1011, "upstream send failed");
    closeSocket(upstreamSocket, 1011, "upstream send failed");
  }
}

async function forwardToClient(
  clientSocket: WebSocket,
  data: unknown,
  upstreamSocket: WebSocket,
): Promise<void> {
  try {
    await sendNormalized(clientSocket, data);
  } catch (error) {
    console.error("failed to forward upstream websocket message", error);
    closeSocket(clientSocket, 1011, "client send failed");
    closeSocket(upstreamSocket, 1011, "client send failed");
  }
}

async function sendNormalized(socket: WebSocket, data: unknown): Promise<void> {
  if (data instanceof Blob) {
    socket.send(await data.arrayBuffer());
    return;
  }

  if (
    typeof data === "string" ||
    data instanceof ArrayBuffer ||
    ArrayBuffer.isView(data)
  ) {
    socket.send(data);
    return;
  }

  throw new Error("unsupported websocket payload");
}
