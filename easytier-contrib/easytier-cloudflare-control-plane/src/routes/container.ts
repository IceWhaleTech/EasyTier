import { Hono } from "hono";
import type { Context } from "hono";
import { getContainerStub, requireContainerStub } from "../utils/container";
import { parseInstanceConfig, formatConfigExample, RequestError } from "../config";
import type { WorkerEnv, ConfigRecord } from "../types/index";

const container = new Hono<{ Bindings: WorkerEnv }>();

// GET /api/instance - Get instance status
container.get("/", async (c: Context) => {
  const stub = getContainerStub(c.env);
  const response = await stub.fetch(new Request("http://internal/control/status"));
  const data = await readJsonResponse(response);
  return c.json(data);
});

// GET /api/instance/config-example - Get config example
container.get("/config-example", (c: Context) => {
  return c.json(formatConfigExample());
});

// PUT /api/instance - Update instance config
container.put("/", async (c: Context) => {
  const stub = requireContainerStub(c.env);
  const body = await c.req.json() as ConfigRecord;
  const config = parseInstanceConfig(body);
  
  const response = await stub.fetch(
    new Request("http://internal/control/config", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(config),
    })
  );
  
  const data = await readJsonResponse(response);
  return c.json(data);
});

// DELETE /api/instance - Delete instance
container.delete("/", async (c: Context) => {
  const stub = requireContainerStub(c.env);
  const response = await stub.fetch(
    new Request("http://internal/control/config", { method: "DELETE" })
  );
  const data = await readJsonResponse(response);
  return c.json(data);
});

// POST /api/instance/start - Start instance
container.post("/start", async (c: Context) => {
  const stub = requireContainerStub(c.env);
  const response = await stub.fetch(
    new Request("http://internal/control/start", { method: "POST" })
  );
  const data = await readJsonResponse(response);
  return c.json(data);
});

// POST /api/instance/stop - Stop instance
container.post("/stop", async (c: Context) => {
  const stub = requireContainerStub(c.env);
  const response = await stub.fetch(
    new Request("http://internal/control/stop", { method: "POST" })
  );
  const data = await readJsonResponse(response);
  return c.json(data);
});

// POST /api/instance/observed-handshake - Record handshake observation
container.post("/observed-handshake", async (c: Context) => {
  const stub = requireContainerStub(c.env);
  const body = await c.req.json() as { networkName?: string };
  
  if (!body.networkName) {
    throw new RequestError(400, "networkName is required");
  }
  
  const response = await stub.fetch(
    new Request("http://internal/control/observed-handshake", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ networkName: body.networkName }),
    })
  );
  
  const data = await readJsonResponse(response);
  return c.json(data);
});

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

export { container as containerRoutes };
