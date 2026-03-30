export type InstanceConfig = {
  instanceName?: string;
  hostname?: string;
  networkName?: string;
  networkSecret?: string;
  peers?: string[];
  listeners?: string[];
  extraArgs?: string[];
  noTun?: boolean;
  rpcPortal?: string;
  env?: Record<string, string>;
};

export type StoredState = {
  config: InstanceConfig | null;
  updatedAt: string | null;
  lastStartAt: string | null;
  lastStopAt: string | null;
  runtime: { status: string; lastChange?: number; exitCode?: number } | null;
};

export type NetworkRouteRecord = {
  networkName: string;
  instanceName: string;
  locationHint: DurableObjectLocationHint;
  createdAt: string;
  lastResolvedAt: string;
};

export type ResolveNetworkRouteResult = NetworkRouteRecord & {
  created: boolean;
};

export type ContainerInstanceRecord = {
  id: string | null;
  name: string | null;
  state: string;
  location: string | null;
  version: number | null;
  created: string | null;
};

export type SyncedInstanceRecord = ContainerInstanceRecord & {
  syncedAt: string;
};
