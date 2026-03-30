import type { ContainerInstanceRecord, SyncedInstanceRecord } from "./types";

export type InstanceCatalogEnv = Pick<
  Cloudflare.Env,
  | "INSTANCE_CATALOG"
  | "CF_CONTAINERS_ACCOUNT_ID"
  | "CF_EASYTIER_CONTAINER_APP_ID"
> & {
  CF_CONTAINERS_API_TOKEN?: string;
  CF_API_TOKEN?: string;
  CLOUDFLARE_API_TOKEN?: string;
};

export type InstanceCatalogView =
  | {
      source: "instance-catalog";
      instance: SyncedInstanceRecord;
    }
  | {
      source: "instance-catalog-miss";
      instance: null;
    }
  | {
      source: "instance-catalog-error";
      reason: string;
      instance: null;
    };

type CloudflareApiEnvelope = {
  success?: boolean;
  errors?: Array<{ message?: string }>;
  result?: unknown;
  result_info?: {
    next_page_token?: string | null;
  };
};

type CloudflareContainerInstance = {
  id?: string | null;
  name?: string | null;
  state?: string | null;
  status?: string | null;
  location?: string | null;
  version?: number | null;
  app_version?: number | null;
  created?: string | null;
  created_at?: string | null;
};

type CloudflareDurableObjectInstance = {
  id?: string | null;
  name?: string | null;
  deployment_id?: string | null;
  assigned_at?: string | null;
};

type CloudflareInstancesPage = {
  instances?: CloudflareContainerInstance[];
  durable_objects?: CloudflareDurableObjectInstance[];
};

export function syncInstanceCatalog(
  env: InstanceCatalogEnv,
  payload: unknown,
): Promise<Response> {
  return getInstanceCatalogStub(env).fetch(
    new Request(buildInternalUrl("/instances/sync"), {
      method: "PUT",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify(payload),
    }),
  );
}

export async function resolveInstanceCatalogView(
  env: InstanceCatalogEnv,
  instanceName: string,
): Promise<InstanceCatalogView> {
  try {
    const syncedInstance = await getSyncedInstance(env, instanceName);
    if (syncedInstance) {
      return {
        source: "instance-catalog",
        instance: syncedInstance,
      };
    }
  } catch (error) {
    return {
      source: "instance-catalog-error",
      reason: toErrorMessage(error),
      instance: null,
    };
  }

  if (!getCloudflareApiToken(env)) {
    return {
      source: "instance-catalog-miss",
      instance: null,
    };
  }

  try {
    await syncInstanceCatalogFromCloudflare(env);
    const refreshedInstance = await getSyncedInstance(env, instanceName);
    return refreshedInstance
      ? {
          source: "instance-catalog",
          instance: refreshedInstance,
        }
      : {
          source: "instance-catalog-miss",
          instance: null,
        };
  } catch (error) {
    return {
      source: "instance-catalog-error",
      reason: toErrorMessage(error),
      instance: null,
    };
  }
}

export async function syncInstanceCatalogFromCloudflare(
  env: InstanceCatalogEnv,
): Promise<{
  ok: true;
  count: number;
  syncedAt: string;
}> {
  const token = getCloudflareApiToken(env);
  if (!token) {
    throw new Error(
      "CF_CONTAINERS_API_TOKEN, CF_API_TOKEN, or CLOUDFLARE_API_TOKEN is required for automatic instance sync",
    );
  }

  const response = await syncInstanceCatalog(env, {
    syncedAt: new Date().toISOString(),
    instances: await listContainerInstancesFromCloudflare(env, token),
  });
  return readJsonResponse<{ ok: true; count: number; syncedAt: string }>(
    response,
  );
}

async function getSyncedInstance(
  env: InstanceCatalogEnv,
  instanceName: string,
): Promise<SyncedInstanceRecord | null> {
  const response = await getInstanceCatalogStub(env).fetch(
    new Request(
      buildInternalUrl(`/instances/${encodeURIComponent(instanceName)}`),
      { method: "GET" },
    ),
  );
  return readJsonResponse<SyncedInstanceRecord | null>(response);
}

function getInstanceCatalogStub(env: InstanceCatalogEnv) {
  const id = env.INSTANCE_CATALOG.idFromName("global");
  return env.INSTANCE_CATALOG.get(id);
}

async function listContainerInstancesFromCloudflare(
  env: InstanceCatalogEnv,
  token: string,
): Promise<ContainerInstanceRecord[]> {
  const instances: ContainerInstanceRecord[] = [];
  let pageToken: string | null | undefined;
  const baseUrl = new URL(
    `/client/v4/accounts/${env.CF_CONTAINERS_ACCOUNT_ID}/containers/dash/applications/${env.CF_EASYTIER_CONTAINER_APP_ID}/instances`,
    "https://api.cloudflare.com",
  );

  do {
    const url = new URL(baseUrl);
    url.searchParams.set("per_page", "100");
    if (pageToken) {
      url.searchParams.set("page_token", pageToken);
    }

    const response = await fetch(url, {
      headers: {
        authorization: `Bearer ${token}`,
      },
    });
    const payload = await readCloudflareApiResponse(response);
    instances.push(
      ...mapCloudflareInstancesPage(readCloudflareInstancesPage(payload)),
    );
    pageToken = readCloudflareNextPageToken(payload);
  } while (pageToken);

  return instances;
}

async function readCloudflareApiResponse(
  response: Response,
): Promise<CloudflareApiEnvelope> {
  const payload = (await response.json()) as CloudflareApiEnvelope;
  if (response.ok && payload.success !== false) {
    return payload;
  }

  throw new Error(
    payload.errors
      ?.map((item) => item.message)
      .filter(Boolean)
      .join("; ") || `request failed with status ${response.status}`,
  );
}

function readCloudflareInstancesPage(
  payload: CloudflareApiEnvelope,
): CloudflareInstancesPage | undefined {
  const result = payload.result;
  if (Array.isArray(result)) {
    return {
      instances: result as CloudflareContainerInstance[],
    };
  }

  if (!isRecord(result)) {
    return undefined;
  }

  const resultRecord = result as Record<string, unknown>;
  if (Array.isArray(resultRecord.data)) {
    return {
      instances: resultRecord.data as CloudflareContainerInstance[],
    };
  }
  return isRecord(resultRecord.data)
    ? (resultRecord.data as CloudflareInstancesPage)
    : (resultRecord as CloudflareInstancesPage);
}

function readCloudflareNextPageToken(
  payload: CloudflareApiEnvelope,
): string | null | undefined {
  if (payload.result_info?.next_page_token) {
    return payload.result_info.next_page_token;
  }

  const result = payload.result;
  if (!isRecord(result) || !isRecord(result.result_info)) {
    return undefined;
  }

  const nextPageToken = result.result_info.next_page_token;
  return typeof nextPageToken === "string" ? nextPageToken : null;
}

function mapCloudflareInstancesPage(
  page: CloudflareInstancesPage | undefined,
): ContainerInstanceRecord[] {
  const instances = page?.instances ?? [];
  const durableObjects = page?.durable_objects ?? [];

  if (durableObjects.length === 0) {
    return instances.map((instance) => ({
      id: instance.id ?? null,
      name: instance.name ?? null,
      state: normalizeCloudflareInstanceState(
        instance.state ?? instance.status,
      ),
      location: instance.location ?? null,
      version: instance.version ?? instance.app_version ?? null,
      created: instance.created ?? instance.created_at ?? null,
    }));
  }

  const instanceByDeploymentId = new Map(
    instances.map((instance) => [instance.id, instance] as const),
  );

  return durableObjects.map((durableObject) => {
    const instance =
      durableObject.deployment_id === undefined ||
      durableObject.deployment_id === null
        ? null
        : (instanceByDeploymentId.get(durableObject.deployment_id) ?? null);

    return {
      id: durableObject.id ?? instance?.id ?? null,
      name: durableObject.name ?? null,
      state: instance
        ? normalizeCloudflareInstanceState(instance.state ?? instance.status)
        : "inactive",
      location: instance?.location ?? null,
      version: instance?.version ?? instance?.app_version ?? null,
      created:
        instance?.created ??
        instance?.created_at ??
        durableObject.assigned_at ??
        null,
    };
  });
}

function normalizeCloudflareInstanceState(status: string | null | undefined) {
  switch (status) {
    case "pending":
    case "requested":
    case "running":
    case "failed":
    case "stopping":
    case "stopped":
    case "unhealthy":
      return status;
    default:
      return "unknown";
  }
}

function getCloudflareApiToken(env: InstanceCatalogEnv): string | null {
  return (
    env.CF_CONTAINERS_API_TOKEN ??
    env.CF_API_TOKEN ??
    env.CLOUDFLARE_API_TOKEN ??
    null
  );
}

function buildInternalUrl(pathname: string): string {
  return `https://internal${pathname}`;
}

async function readJsonResponse<T>(response: Response): Promise<T> {
  const data = (await response.json()) as T & { error?: string };
  if (!response.ok) {
    throw new Error(
      typeof data === "object" && data && "error" in data && data.error
        ? String(data.error)
        : `request failed with status ${response.status}`,
    );
  }
  return data;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function toErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
