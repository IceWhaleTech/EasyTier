import {
  DEFAULT_INSTANCE,
  DEFAULT_RPC_PORTAL,
  DEFAULT_WS_LISTENER,
} from "./constants";
import type { ConfigRecord, InstanceConfig, JsonValue } from "./types/index";

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
  const listeners = input.listeners && input.listeners.length > 0
    ? [...new Set(input.listeners.map((item) => item.trim()).filter(Boolean))]
    : [DEFAULT_WS_LISTENER];

  return {
    instanceName: input.instanceName?.trim() || DEFAULT_INSTANCE,
    hostname: input.hostname?.trim() || undefined,
    networkName: input.networkName?.trim() || "",
    networkSecret: input.networkSecret ?? "",
    ipv4: input.ipv4?.trim() || undefined,
    peers: (input.peers ?? []).map((item) => item.trim()).filter(Boolean),
    listeners,
    extraArgs: (input.extraArgs ?? [])
      .map((item) => item.trim())
      .filter(Boolean),
    noTun: input.noTun ?? true,
    rpcPortal: input.rpcPortal?.trim() || DEFAULT_RPC_PORTAL,
    env: Object.fromEntries(
      Object.entries(input.env ?? {}).filter(
        ([, value]) => value !== undefined,
      ),
    ),
  };
}

export function buildEasyTierArgs(config: InstanceConfig): string[] {
  const args: string[] = [];
  if (config.hostname) {
    args.push("--hostname", config.hostname);
  }
  if (config.ipv4) {
    args.push("--ipv4", config.ipv4);
  }
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
    args.push("-p", peer);
  }
  args.push(...(config.extraArgs ?? []));
  return args;
}

export function parseInstanceConfig(record: ConfigRecord): InstanceConfig {
  const parseStringArray = (key: string): string[] => {
    const raw = record[key];
    if (raw === undefined) {
      return [];
    }
    if (
      !Array.isArray(raw) ||
      raw.some((item: JsonValue) => typeof item !== "string")
    ) {
      throw new RequestError(400, `${key} must be an array of strings`);
    }
    return raw as string[];
  };

  const parseOptionalString = (key: string): string | undefined => {
    const raw = record[key];
    if (raw === undefined) {
      return undefined;
    }
    if (typeof raw !== "string") {
      throw new RequestError(400, `${key} must be a string`);
    }
    return raw;
  };

  const parseRequiredString = (key: string): string => {
    const value = parseOptionalString(key);
    if (!value?.trim()) {
      throw new RequestError(400, `${key} is required`);
    }
    return value;
  };

  const parseOptionalBoolean = (key: string): boolean | undefined => {
    const raw = record[key];
    if (raw === undefined) {
      return undefined;
    }
    if (typeof raw !== "boolean") {
      throw new RequestError(400, `${key} must be a boolean`);
    }
    return raw;
  };

  const env = record.env;
  if (
    env !== undefined &&
    (typeof env !== "object" || env === null || Array.isArray(env) ||
    Object.values(env).some((item: JsonValue) => typeof item !== "string"))
  ) {
    throw new RequestError(400, "env must be an object of string values");
  }

  return normalizeConfig({
    instanceName: parseOptionalString("instanceName"),
    hostname: parseOptionalString("hostname"),
    networkName: parseRequiredString("networkName"),
    networkSecret: parseRequiredString("networkSecret"),
    ipv4: parseOptionalString("ipv4"),
    peers: parseStringArray("peers"),
    listeners: parseStringArray("listeners"),
    extraArgs: parseStringArray("extraArgs"),
    noTun: parseOptionalBoolean("noTun"),
    rpcPortal: parseOptionalString("rpcPortal"),
    env: (env as Record<string, string> | undefined) ?? undefined,
  });
}

export function formatConfigExample(): InstanceConfig {
  return {
    instanceName: DEFAULT_INSTANCE,
    networkName: "stage1-demo",
    networkSecret: "stage1-demo",
    noTun: true,
    rpcPortal: DEFAULT_RPC_PORTAL,
    peers: [],
    listeners: [DEFAULT_WS_LISTENER],
    extraArgs: [],
  };
}
