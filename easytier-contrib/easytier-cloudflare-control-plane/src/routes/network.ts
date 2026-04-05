import { Hono } from "hono";
import { getNetworkRegistry, registerNodeToNetwork, getNodesInNetwork } from "../utils/network";
import type { WorkerEnv, NodeInfo } from "../types/index";

const network = new Hono<{ Bindings: WorkerEnv }>();

// GET /api/networks - List all networks
network.get("/", async (c) => {
  const registry = await getNetworkRegistry(c.env.NETWORK_REGISTRY);
  const networks = Object.keys(registry).map(name => ({
    name,
    nodeCount: registry[name]?.nodes?.length || 0,
    updatedAt: registry[name]?.updatedAt,
  }));
  
  return c.json({ networks });
});

// GET /api/networks/:name/nodes - Get nodes in a specific network
network.get("/:name/nodes", async (c) => {
  const networkName = c.req.param("name");
  const nodes = await getNodesInNetwork(c.env.NETWORK_REGISTRY, networkName);
  
  // Filter out expired nodes (older than 5 minutes)
  const now = Date.now();
  const activeNodes = nodes.filter((node: NodeInfo) => now - node.lastSeen < 5 * 60 * 1000);
  
  return c.json({
    network: networkName,
    nodes: activeNodes,
    total: activeNodes.length,
  });
});

// POST /api/networks/:name/register - Register a node to network
network.post("/:name/register", async (c) => {
  const networkName = c.req.param("name");
  const body = await c.req.json();
  
  const nodeInfo: NodeInfo = {
    doId: body.doId,
    nodeId: body.nodeId,
    publicEndpoint: body.publicEndpoint,
    vpnIp: body.vpnIp,
    lastSeen: Date.now(),
  };
  
  await registerNodeToNetwork(c.env.NETWORK_REGISTRY, networkName, nodeInfo);
  
  return c.json({ ok: true, network: networkName, node: nodeInfo });
});

export { network as networkRoutes };
