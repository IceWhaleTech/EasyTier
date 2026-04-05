import { formatConfigExample, RequestError } from "./config";
import type { ErrorLike } from "./types/index";
import { DEFAULT_INSTANCE } from "./constants";
import { EasyTierContainer, MyContainer } from "./easytier-container";

type WorkerEnv = Cloudflare.Env;

export { EasyTierContainer, MyContainer };

export default {
  async fetch(request, env, executionCtx) {
    try {
      return await routeRequest(request, env, executionCtx);
    } catch (error) {
      return toErrorResponse(error as ErrorLike);
    }
  },
} satisfies ExportedHandler<WorkerEnv>;

async function routeRequest(
  request: Request,
  env: WorkerEnv,
  executionCtx: ExecutionContext,
): Promise<Response> {
  const url = new URL(request.url);

  if (
    isWebSocketRequest(request) &&
    (url.pathname === "/" || url.pathname === "/connect")
  ) {
    return proxyWebSocketToContainer(env, request);
  }

  if (request.method === "GET" && url.pathname === "/") {
    return landingPage(request);
  }

  if (request.method === "GET" && url.pathname === "/healthz") {
    return Response.json({ ok: true });
  }

  if (request.method === "GET" && url.pathname === "/api/config-example") {
    return Response.json(formatConfigExample());
  }

  if (request.method === "GET" && url.pathname === "/api/instance") {
    return sendControlRequest(env, "/control/status", "GET");
  }

  if (request.method === "PUT" && url.pathname === "/api/instance") {
    return proxyToContainer(env, request, "/control/config");
  }

  if (request.method === "DELETE" && url.pathname === "/api/instance") {
    return sendControlRequest(env, "/control/config", "DELETE");
  }

  if (request.method === "POST" && url.pathname === "/api/instance/start") {
    return sendControlRequest(env, "/control/start", "POST");
  }

  if (request.method === "POST" && url.pathname === "/api/instance/stop") {
    return sendControlRequest(env, "/control/stop", "POST");
  }

  if (request.method === "GET" && url.pathname === "/connect") {
    return Response.json({ error: "websocket upgrade required" }, { status: 426 });
  }

  return Response.json({ error: "route not found" }, { status: 404 });
}

function landingPage(request: Request): Response {
  const url = new URL(request.url);
  const wsProtocol = url.protocol === "http:" ? "ws:" : "wss:";
  const websocketUrl = `${wsProtocol}//${url.host}/connect`;
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
      <p>Phase 1 runs a single Cloudflare Container backed by <code>docker.io/easytier/easytier:v2.5.0</code> and forwards EasyTier WebSocket traffic to it.</p>

      <div class="card">
        <strong>Endpoints</strong>
        <ul>
          <li><code>GET /healthz</code></li>
          <li><code>GET /api/config-example</code></li>
          <li><code>GET /api/instance</code></li>
          <li><code>PUT /api/instance</code></li>
          <li><code>POST /api/instance/start</code></li>
          <li><code>POST /api/instance/stop</code></li>
          <li><code>WS /connect</code></li>
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

function isWebSocketRequest(request: Request): boolean {
  return request.headers.get("Upgrade")?.toLowerCase() === "websocket";
}

function getContainerStub(env: WorkerEnv) {
  return env.EASYTIER_CONTAINER.getByName(DEFAULT_INSTANCE);
}

function buildInternalUrl(pathname: string): string {
  return `https://internal${pathname}`;
}

function proxyToContainer(
  env: WorkerEnv,
  request: Request,
  pathname: string,
): Promise<Response> {
  return getContainerStub(env).fetch(new Request(buildInternalUrl(pathname), request));
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

function proxyWebSocketToContainer(
  env: WorkerEnv,
  request: Request,
): Promise<Response> {
  return getContainerStub(env).fetch(new Request(buildInternalUrl("/"), request));
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
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
