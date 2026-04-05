import { Hono } from "hono";
import { requireContainerStub } from "../utils/container";
import type { WorkerEnv } from "../types/index";

const websocket = new Hono<{ Bindings: WorkerEnv }>();

// Handle WebSocket upgrade for / and /connect
websocket.all("/", async (c) => {
  const request = c.req.raw;
  
  // Check if this is a WebSocket request
  const upgrade = request.headers.get("Upgrade")?.toLowerCase();
  if (upgrade !== "websocket") {
    return c.text("WebSocket endpoint", 426);
  }
  
  // Proxy to container
  const stub = requireContainerStub(c.env);
  return stub.fetch(request);
});

export { websocket as websocketRoutes };
