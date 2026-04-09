// Config types - separate from other types to avoid circular dependencies
export interface InstanceConfig {
  instanceName?: string;
  hostname?: string;
  networkName: string;
  networkSecret: string;
  ipv4?: string;
  peers: string[];
  listeners: string[];
  extraArgs: string[];
  noTun?: boolean;
  rpcPortal?: string;
  env?: Record<string, string>;
}

// Type for parsed config from JSON
export type ConfigInput = {
  instanceName?: string;
  hostname?: string;
  networkName?: string;
  networkSecret?: string;
  ipv4?: string;
  peers?: string[];
  listeners?: string[];
  extraArgs?: string[];
  noTun?: boolean;
  rpcPortal?: string;
  env?: Record<string, string>;
};

// Raw config record from JSON input - allows any JSON-compatible value
export type ConfigRecord = Record<string, JsonValue>;

// JSON value types
export type JsonValue = 
  | string 
  | number 
  | boolean 
  | null 
  | JsonValue[] 
  | { [key: string]: JsonValue };

export class RequestError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "RequestError";
  }
}
