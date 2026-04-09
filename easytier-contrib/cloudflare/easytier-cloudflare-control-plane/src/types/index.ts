// Main types export file
export type { InstanceConfig, ConfigRecord, JsonValue } from "./config";
export { RequestError } from "./config";

// Durable Object state
export interface StoredState {
  config: import("./config").InstanceConfig | null;
  updatedAt: string | null;
  lastStartAt: string | null;
  lastStopAt: string | null;
  lastHandshakeAt: string | null;
  lastHandshakeNetworkName: string | null;
  runtime: {
    status: string;
    lastChange: string | number;
    exitCode?: number;
  } | null;
}

export type NetworkRouteMode = "digest" | "network-name-only";

export interface ExtractedNetworkRouteIdentity {
  networkName: string;
  routeKey: string;
  routeMode: NetworkRouteMode;
  secretDigestHex: string | null;
}

export interface NetworkRouteRecord {
  networkName: string;
  routeKey: string;
  routeMode: NetworkRouteMode;
  secretDigestHex: string | null;
  instanceName: string;
  locationHint: DurableObjectLocationHint;
  createdAt: string;
  lastResolvedAt: string;
}

export interface ResolveNetworkRouteResult extends NetworkRouteRecord {
  created: boolean;
}

// Node info in network registry
export interface NodeInfo {
  doId: string;
  nodeId?: string;
  publicEndpoint?: string;
  vpnIp?: string;
  lastSeen: number;
}

// Network registry entry
export interface NetworkRegistry {
  nodes: NodeInfo[];
  updatedAt: string;
}

// Worker environment bindings
export interface WorkerEnv {
  EASYTIER_CONTAINER: DurableObjectNamespace;
  NETWORK_ROUTER: DurableObjectNamespace;
}

// WebSocket data types
export type WebSocketData = string | ArrayBuffer | Blob | ArrayBufferView;

// Blob-like interface
export interface BlobLike {
  arrayBuffer(): Promise<ArrayBuffer>;
}

// Error types
export type ErrorLike = Error | { message?: string; toString(): string } | string | number | boolean | null | undefined;

// Config record value type
export type ConfigRecordValue = import("./config").JsonValue;

// JSON record type for request bodies
export type JsonRecord = Record<string, ConfigRecordValue>;
