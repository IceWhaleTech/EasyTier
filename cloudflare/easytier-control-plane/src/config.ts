import {
  DEFAULT_INSTANCE,
  DEFAULT_WS_LISTENER,
  SHARED_RELAY_NETWORK_NAME,
} from "./constants";
import type { InstanceConfig } from "./types";

export class RequestError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "RequestError";
  }
}

export function normalizeConfig(input: InstanceConfig): InstanceConfig {
  const listeners =
    input.listeners && input.listeners.length > 0
      ? [...new Set(input.listeners.map((item) => item.trim()).filter(Boolean))]
      : [DEFAULT_WS_LISTENER];

  return {
    instanceName: input.instanceName?.trim() || DEFAULT_INSTANCE,
    networkName: input.networkName?.trim() || "",
    networkSecret: input.networkSecret ?? "",
    peers: (input.peers ?? []).map((item) => item.trim()).filter(Boolean),
    listeners,
    extraArgs: (input.extraArgs ?? [])
      .map((item) => item.trim())
      .filter(Boolean),
    noTun: input.noTun ?? true,
    rpcPortal: input.rpcPortal?.trim() || undefined,
    env: Object.fromEntries(
      Object.entries(input.env ?? {}).filter(
        ([, value]) => value !== undefined,
      ),
    ),
  };
}

export function buildEasyTierArgs(config: InstanceConfig): string[] {
  const args: string[] = [];

  args.push("--network-name", config.networkName ?? "");
  args.push("--network-secret", config.networkSecret ?? "");

  if (config.noTun ?? true) {
    args.push("--no-tun");
  }

  if (config.rpcPortal) {
    args.push("--rpc-portal", config.rpcPortal);
  }

  for (const listener of config.listeners ?? []) {
    args.push("--listeners", listener);
  }

  for (const peer of config.peers ?? []) {
    args.push("--peer", peer);
  }

  args.push(...(config.extraArgs ?? []));

  return args;
}

export function parseInstanceConfig(value: unknown): InstanceConfig {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new RequestError(400, "request body must be a JSON object");
  }

  const record = value as Record<string, unknown>;

  const parseStringArray = (key: string): string[] | undefined => {
    const raw = record[key];
    if (raw === undefined) {
      return undefined;
    }
    if (!Array.isArray(raw) || raw.some((item) => typeof item !== "string")) {
      throw new RequestError(400, `${key} must be an array of strings`);
    }
    return raw;
  };

  const env = record.env;
  if (
    env !== undefined &&
    (typeof env !== "object" ||
      env === null ||
      Array.isArray(env) ||
      Object.values(env).some((item) => typeof item !== "string"))
  ) {
    throw new RequestError(400, "env must be an object of string values");
  }

  const booleanOrUndefined = (key: string): boolean | undefined => {
    const raw = record[key];
    if (raw === undefined) {
      return undefined;
    }
    if (typeof raw !== "boolean") {
      throw new RequestError(400, `${key} must be a boolean`);
    }
    return raw;
  };

  const stringOrUndefined = (key: string): string | undefined => {
    const raw = record[key];
    if (raw === undefined) {
      return undefined;
    }
    if (typeof raw !== "string") {
      throw new RequestError(400, `${key} must be a string`);
    }
    return raw;
  };

  return normalizeConfig({
    instanceName: stringOrUndefined("instanceName"),
    networkName: stringOrUndefined("networkName"),
    networkSecret: stringOrUndefined("networkSecret"),
    peers: parseStringArray("peers"),
    listeners: parseStringArray("listeners"),
    extraArgs: parseStringArray("extraArgs"),
    noTun: booleanOrUndefined("noTun"),
    rpcPortal: stringOrUndefined("rpcPortal"),
    env: (env as Record<string, string> | undefined) ?? undefined,
  });
}

export function formatConfigExample(): InstanceConfig {
  return {
    instanceName: DEFAULT_INSTANCE,
    networkName: "example-network",
    networkSecret: "example-secret",
    noTun: true,
    peers: [],
    listeners: [DEFAULT_WS_LISTENER],
    extraArgs: [],
  };
}

export function buildSharedRelayConfig(instanceName: string): InstanceConfig {
  return normalizeConfig({
    instanceName,
    networkName: SHARED_RELAY_NETWORK_NAME,
    networkSecret: "",
    noTun: true,
    listeners: [DEFAULT_WS_LISTENER],
    peers: [],
    extraArgs: [],
  });
}
