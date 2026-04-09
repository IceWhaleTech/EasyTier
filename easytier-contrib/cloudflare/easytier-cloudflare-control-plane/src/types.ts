export type InstanceConfig = {
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

export type StoredState = {
  config: InstanceConfig | null;
  updatedAt: string | null;
  lastStartAt: string | null;
  lastStopAt: string | null;
  lastHandshakeAt: string | null;
  lastHandshakeNetworkName: string | null;
  runtime: { status: string; lastChange?: number; exitCode?: number } | null;
};
