import { Hono } from "hono";
import type { WorkerEnv } from "../types/index";

const health = new Hono<{ Bindings: WorkerEnv }>();

health.get("/", (c) => {
  return c.json({ ok: true, timestamp: new Date().toISOString() });
});

export { health as healthRoutes };
