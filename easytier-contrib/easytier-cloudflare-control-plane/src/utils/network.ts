import type { NodeInfo, NetworkRegistry } from "../types/index";

const REGISTRY_KEY = "network_registry";
const NODE_TTL_MS = 5 * 60 * 1000; // 5 minutes

export async function getNetworkRegistry(kv: KVNamespace): Promise<Record<string, NetworkRegistry>> {
  const data = await kv.get(REGISTRY_KEY);
  if (!data) {
    return {};
  }
  return JSON.parse(data) as Record<string, NetworkRegistry>;
}

export async function getNodesInNetwork(kv: KVNamespace, networkName: string): Promise<NodeInfo[]> {
  const registry = await getNetworkRegistry(kv);
  const network = registry[networkName];
  if (!network || !network.nodes) {
    return [];
  }
  
  // Filter out expired nodes
  const now = Date.now();
  return network.nodes.filter(node => now - node.lastSeen < NODE_TTL_MS);
}

export async function registerNodeToNetwork(
  kv: KVNamespace,
  networkName: string,
  nodeInfo: NodeInfo
): Promise<void> {
  const registry = await getNetworkRegistry(kv);
  
  if (!registry[networkName]) {
    registry[networkName] = {
      nodes: [],
      updatedAt: new Date().toISOString(),
    };
  }
  
  // Remove existing node with same doId
  registry[networkName].nodes = registry[networkName].nodes.filter(
    node => node.doId !== nodeInfo.doId
  );
  
  // Add new node
  registry[networkName].nodes.push(nodeInfo);
  registry[networkName].updatedAt = new Date().toISOString();
  
  await kv.put(REGISTRY_KEY, JSON.stringify(registry));
}

export async function removeNodeFromNetwork(
  kv: KVNamespace,
  networkName: string,
  doId: string
): Promise<void> {
  const registry = await getNetworkRegistry(kv);
  
  if (!registry[networkName]) {
    return;
  }
  
  registry[networkName].nodes = registry[networkName].nodes.filter(
    node => node.doId !== doId
  );
  registry[networkName].updatedAt = new Date().toISOString();
  
  await kv.put(REGISTRY_KEY, JSON.stringify(registry));
}
