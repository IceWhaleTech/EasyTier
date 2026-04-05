import { RequestError } from "../config";
import type { WorkerEnv } from "../types/index";

const DEFAULT_INSTANCE = "default";

export function getContainerStub(env: WorkerEnv): DurableObjectStub {
  const id = env.EASYTIER_CONTAINER.idFromName(DEFAULT_INSTANCE);
  return env.EASYTIER_CONTAINER.get(id);
}

export function requireContainerStub(env: WorkerEnv): DurableObjectStub {
  const stub = getContainerStub(env);
  if (!stub) {
    throw new RequestError(500, "Container DO not available");
  }
  return stub;
}
