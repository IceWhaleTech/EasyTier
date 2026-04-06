import {
  buildSharedRelayConfig,
  formatConfigExample,
  RequestError,
} from "./config";
import { Hono } from "hono";
import { DEFAULT_INSTANCE, INITIAL_MESSAGE_TIMEOUT_MS } from "./constants";
import { EasyTierContainer, MyContainer } from "./easytier-container";
import {
  extractNetworkRouteIdentityFromFrame,
  type Frame,
} from "./easytier-proto";
import { generateNetworkSecretDigestHex } from "./network-secret-digest";
import { NetworkRouter } from "./network-router";
import type {
  ErrorLike,
  NetworkRouteRecord,
  ResolveNetworkRouteResult,
} from "./types/index";

type WorkerEnv = Cloudflare.Env;
type AppEnv = { Bindings: WorkerEnv };
type WaitUntilExecutionContext = {
  waitUntil(promise: Promise<unknown>): void;
};

export { EasyTierContainer, MyContainer, NetworkRouter };
const app = createApp();

export default {
  fetch(request, env, executionCtx) {
    return app.fetch(request, env, executionCtx);
  },
} satisfies ExportedHandler<WorkerEnv>;

function createApp(): Hono<AppEnv> {
  const app = new Hono<AppEnv>();

  app.onError((error) => toErrorResponse(error as ErrorLike));
  app.notFound(() => Response.json({ error: "route not found" }, { status: 404 }));

  app.use("*", async (c, next) => {
    const pathname = new URL(c.req.url).pathname;
    if (
      isWebSocketRequest(c.req.raw) &&
      (pathname === "/" || pathname === "/connect")
    ) {
      return acceptRoutedWebSocket(c.req.raw, c.env, c.executionCtx);
    }

    await next();
  });

  app.get("/", (c) => landingPage(c.req.raw));
  app.get("/healthz", (c) => c.json({ ok: true }));
  app.get("/api/config-example", (c) => c.json(formatConfigExample()));
  app.get("/api/network-route", (c) =>
    lookupNetworkRoute(c.env, new URL(c.req.url).searchParams),
  );
  app.get("/api/instance", (c) =>
    sendControlRequest(c.env, "/control/status", "GET"),
  );
  app.put("/api/instance", (c) =>
    proxyToContainer(c.env, c.req.raw, "/control/config"),
  );
  app.delete("/api/instance", (c) =>
    sendControlRequest(c.env, "/control/config", "DELETE"),
  );
  app.post("/api/instance/start", (c) =>
    sendControlRequest(c.env, "/control/start", "POST"),
  );
  app.post("/api/instance/stop", (c) =>
    sendControlRequest(c.env, "/control/stop", "POST"),
  );
  app.get("/connect", (c) =>
    c.json({ error: "websocket upgrade required" }, 426),
  );

  return app;
}

function landingPage(request: Request): Response {
  const url = new URL(request.url);
  const wsProtocol = url.protocol === "http:" ? "ws:" : "wss:";
  const websocketUrl = `${wsProtocol}//${url.host}`;
  const configExample = JSON.stringify(formatConfigExample(), null, 2);
  const cliExample = [
    "cargo run -p easytier --bin easytier-core -- \\",
    "  --network-name stage1-demo \\",
    "  --network-secret stage1-demo \\",
    "  --no-tun \\",
    `  -p ${websocketUrl} \\`,
    "  --rpc-portal 127.0.0.1:25888 \\",
    "  --hostname cf-client",
  ].join("\n");

  return new Response(
    `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>EasyTier Cloudflare Control Plane</title>
    <style>
      :root {
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        color: #1a2533;
        background: #f4f1ea;
      }
      body {
        margin: 0;
      }
      main {
        max-width: 880px;
        margin: 0 auto;
        padding: 40px 24px 64px;
      }
      h1 {
        margin: 0 0 12px;
        font-size: 2.2rem;
      }
      p, li {
        line-height: 1.6;
      }
      .card {
        margin-top: 20px;
        padding: 20px;
        border-radius: 16px;
        background: #fffdf8;
        border: 1px solid #e1d5c1;
      }
      pre {
        overflow-x: auto;
        padding: 16px;
        border-radius: 12px;
        background: #1d2430;
        color: #f5f7fa;
      }
      code {
        font-family: "SFMono-Regular", "JetBrains Mono", monospace;
      }
    </style>
  </head>
  <body>
    <main>
      <h1>EasyTier Cloudflare Control Plane</h1>
      <p>One websocket endpoint routes ordinary EasyTier handshakes by <code>network_name + secret_digest</code> and falls back to <code>network_name</code> for Noise handshakes.</p>

      <div class="card">
        <strong>Endpoints</strong>
        <ul>
          <li><code>GET /healthz</code></li>
          <li><code>GET /api/config-example</code></li>
          <li><code>GET /api/network-route?networkName=stage1-demo</code></li>
          <li><code>GET /api/instance</code></li>
          <li><code>PUT /api/instance</code></li>
          <li><code>POST /api/instance/start</code></li>
          <li><code>POST /api/instance/stop</code></li>
          <li><code>WS /</code></li>
        </ul>
      </div>

      <div class="card">
        <strong>WebSocket endpoint</strong>
        <pre><code>${escapeHtml(websocketUrl)}</code></pre>
      </div>

      <div class="card">
        <strong>Example instance config</strong>
        <pre><code>${escapeHtml(configExample)}</code></pre>
      </div>

      <div class="card">
        <strong>Example client command</strong>
        <pre><code>${escapeHtml(cliExample)}</code></pre>
      </div>
    </main>
  </body>
</html>`,
    {
      headers: {
        "content-type": "text/html; charset=utf-8",
      },
    },
  );
}

function acceptRoutedWebSocket(
  request: Request,
  env: WorkerEnv,
  executionCtx: WaitUntilExecutionContext,
): Response {
  const pair = new WebSocketPair();
  const clientSocket = pair[0];
  const workerSocket = pair[1];

  workerSocket.accept();
  executionCtx.waitUntil(handleRoutedWebSocket(workerSocket, request, env));

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
  let rejectFirstMessage!: (reason?: ErrorLike) => void;

  const firstMessage = new Promise<Frame>((resolve, reject) => {
    resolveFirstMessage = resolve;
    rejectFirstMessage = reject;
  });

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
      rejectFirstMessage(
        new Error("client closed before routing completed"),
      );
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
    const routeIdentity = await extractNetworkRouteIdentityFromFrame(firstFrame);
    const route = await resolveNetworkRoute(
      env,
      routeIdentity.networkName,
      routeIdentity.routeKey,
      routeIdentity.routeMode,
      routeIdentity.secretDigestHex,
      pickLocationHint(request),
    );

    await configureSharedRelayContainer(env, route);

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
        message: "failed to route websocket by network identity",
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
  routeKey: string,
  routeMode: NetworkRouteRecord["routeMode"],
  secretDigestHex: string | null,
  requestedLocationHint: DurableObjectLocationHint,
): Promise<ResolveNetworkRouteResult> {
  const response = await env.NETWORK_ROUTER.getByName(routeKey).fetch(
    new Request(buildInternalUrl("/route"), {
      method: "POST",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify({
        networkName,
        routeKey,
        routeMode,
        secretDigestHex,
        requestedLocationHint,
      }),
    }),
  );
  return readJsonResponse<ResolveNetworkRouteResult>(response);
}

async function configureSharedRelayContainer(
  env: WorkerEnv,
  route: NetworkRouteRecord,
): Promise<void> {
  const config = buildSharedRelayConfig(route.instanceName);
  await readJsonResponse<{ ok: boolean }>(await sendContainerControlRequest(
    env,
    route.instanceName,
    route.locationHint,
    "/control/config",
    "PUT",
    config,
  ));
  await readJsonResponse<unknown>(await sendContainerControlRequest(
    env,
    route.instanceName,
    route.locationHint,
    "/control/start",
    "POST",
  ));
}

async function configureDefaultSharedRelayContainer(env: WorkerEnv): Promise<void> {
  const config = buildSharedRelayConfig(DEFAULT_INSTANCE);
  await readJsonResponse<{ ok: boolean }>(await getContainerStub(env).fetch(
    new Request(buildInternalUrl("/control/config"), {
      method: "PUT",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify(config),
    }),
  ));
  await readJsonResponse<unknown>(await getContainerStub(env).fetch(
    new Request(buildInternalUrl("/control/start"), {
      method: "POST",
    }),
  ));
}

async function openContainerSocket(
  request: Request,
  env: WorkerEnv,
  route: NetworkRouteRecord,
): Promise<WebSocket> {
  const upstreamRequest = new Request(buildInternalUrl("/"), request);
  const routedStub = getRoutedContainerStub(
    env,
    route.instanceName,
    route.locationHint,
  );
  let response: Response;

  try {
    response = await routedStub.fetch(upstreamRequest);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (!message.includes("The container is not running")) {
      throw error;
    }

    await readJsonResponse<unknown>(await sendContainerControlRequest(
      env,
      route.instanceName,
      route.locationHint,
      "/control/start",
      "POST",
    ));
    await delay(500);
    response = await routedStub.fetch(new Request(buildInternalUrl("/"), request));
  }

  if (response.status === 101 && response.webSocket) {
    response.webSocket.accept();
    return response.webSocket;
  }

  await configureDefaultSharedRelayContainer(env);
  const fallbackResponse = await getContainerStub(env).fetch(upstreamRequest);
  if (fallbackResponse.status !== 101 || !fallbackResponse.webSocket) {
    throw new Error(
      `container websocket upgrade failed with status ${response.status}, fallback status ${fallbackResponse.status}`,
    );
  }

  fallbackResponse.webSocket.accept();
  return fallbackResponse.webSocket;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function isWebSocketRequest(request: Request): boolean {
  return request.headers.get("Upgrade")?.toLowerCase() === "websocket";
}

function getContainerStub(env: WorkerEnv) {
  return env.EASYTIER_CONTAINER.getByName(DEFAULT_INSTANCE);
}

function getRoutedContainerStub(
  env: WorkerEnv,
  instanceName: string,
  locationHint: DurableObjectLocationHint,
) {
  return env.EASYTIER_CONTAINER.getByName(instanceName, { locationHint });
}

function buildInternalUrl(pathname: string): string {
  return `https://internal${pathname}`;
}

function proxyToContainer(
  env: WorkerEnv,
  request: Request,
  pathname: string,
): Promise<Response> {
  return getContainerStub(env).fetch(
    new Request(buildInternalUrl(pathname), request),
  );
}

function sendControlRequest(
  env: WorkerEnv,
  pathname: string,
  method: "GET" | "POST" | "DELETE",
): Promise<Response> {
  return getContainerStub(env).fetch(
    new Request(buildInternalUrl(pathname), { method }),
  );
}

async function lookupNetworkRoute(
  env: WorkerEnv,
  searchParams: URLSearchParams,
): Promise<Response> {
  const lookupStartedAt = Date.now();
  const networkName = searchParams.get("networkName")?.trim() ?? "";
  const secretDigestHex = resolveLookupSecretDigestHex(
    networkName,
    searchParams,
  );
  const networkSecret = normalizeQueryString(
    searchParams.get("networkSecret"),
  );

  if (!networkName) {
    throw new RequestError(400, "networkName is required");
  }

  const routeKey = buildLookupRouteKey(networkName, secretDigestHex);
  const response = await env.NETWORK_ROUTER.getByName(routeKey).fetch(
    new Request(buildInternalUrl("/route"), { method: "GET" }),
  );

  if (!response.ok) {
    const data = (await response.json()) as { error?: string };
    throw new RequestError(
      response.status,
      data.error
        ? `${data.error} for routeKey ${routeKey}`
        : `route lookup failed for routeKey ${routeKey}`,
    );
  }

  const route = (await response.json()) as NetworkRouteRecord;
  const assignedRegionDetail = describeLocationHint(route.locationHint);
  const lookupLatencyMs = Date.now() - lookupStartedAt;
  return Response.json({
    ...route,
    lookupMode: secretDigestHex ? "digest" : "network-name-only",
    lookupLatencyMs,
    assignedRegion: route.locationHint,
    assignedRegionName: assignedRegionDetail.name,
    assignedRegionPrecision: "region",
    derivedFromNetworkSecret: networkSecret !== null,
  });
}

function sendContainerControlRequest(
  env: WorkerEnv,
  instanceName: string,
  locationHint: DurableObjectLocationHint,
  pathname: string,
  method: "GET" | "POST" | "PUT" | "DELETE",
  body?: object,
): Promise<Response> {
  return getRoutedContainerStub(env, instanceName, locationHint).fetch(
    new Request(buildInternalUrl(pathname), {
      method,
      headers: body === undefined ? undefined : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
  );
}

async function readJsonResponse<T>(response: Response): Promise<T> {
  const data = (await response.json()) as T & { error?: string };
  if (!response.ok) {
    throw new RequestError(
      response.status,
      typeof data === "object" && data && "error" in data && data.error
        ? String(data.error)
        : `request failed with status ${response.status}`,
    );
  }
  return data;
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
  data: Frame,
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

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function buildLookupRouteKey(
  networkName: string,
  secretDigestHex: string | null,
): string {
  return secretDigestHex
    ? `digest:${networkName}:${secretDigestHex}`
    : `network:${networkName}`;
}

function resolveLookupSecretDigestHex(
  networkName: string,
  searchParams: URLSearchParams,
): string | null {
  const explicitSecretDigestHex = normalizeSecretDigestHex(
    searchParams.get("secretDigestHex"),
  );
  const networkSecret = normalizeQueryString(searchParams.get("networkSecret"));

  if (networkSecret === null) {
    return explicitSecretDigestHex;
  }

  if (!networkName) {
    throw new RequestError(400, "networkName is required");
  }

  const derivedSecretDigestHex = generateNetworkSecretDigestHex(
    networkName,
    networkSecret,
  );

  if (
    explicitSecretDigestHex !== null &&
    explicitSecretDigestHex !== derivedSecretDigestHex
  ) {
    throw new RequestError(
      400,
      "secretDigestHex does not match the digest derived from networkSecret",
    );
  }

  return derivedSecretDigestHex;
}

function normalizeSecretDigestHex(
  value: string | null,
): string | null {
  const normalized = normalizeQueryString(value)?.toLowerCase() ?? "";

  if (!normalized) {
    return null;
  }

  if (!/^[0-9a-f]+$/.test(normalized)) {
    throw new RequestError(400, "secretDigestHex must be a hex string");
  }

  return normalized;
}

function normalizeQueryString(value: string | null): string | null {
  const normalized = value?.trim() ?? "";
  return normalized ? normalized : null;
}

function describeLocationHint(locationHint: DurableObjectLocationHint): {
  name: string;
} {
  switch (locationHint) {
    case "wnam":
      return {
        name: "Western North America",
      };
    case "enam":
      return {
        name: "Eastern North America",
      };
    case "sam":
      return {
        name: "South America",
      };
    case "weur":
      return {
        name: "Western Europe",
      };
    case "eeur":
      return {
        name: "Eastern Europe",
      };
    case "apac":
      return {
        name: "Asia-Pacific",
      };
    case "oc":
      return {
        name: "Oceania",
      };
    case "afr":
      return {
        name: "Africa",
      };
    case "me":
      return {
        name: "Middle East",
      };
  }
}

function toErrorResponse(error: ErrorLike): Response {
  const status = error instanceof RequestError ? error.status : 500;
  const message = error instanceof Error ? error.message : String(error);

  if (status >= 500) {
    console.error(
      JSON.stringify({
        message: "worker request failed",
        error: message,
      }),
    );
  }

  return Response.json({ error: message }, { status });
}
