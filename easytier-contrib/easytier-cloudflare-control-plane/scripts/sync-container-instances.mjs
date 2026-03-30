import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

const projectRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const workerUrl = process.env.EASYTIER_CONTROL_PLANE_URL ?? "https://et.icewhale.io";
const authToken =
  process.env.API_AUTH_TOKEN ?? process.env.EASYTIER_API_AUTH_TOKEN ?? "";

const wranglerToml = await readFile(resolve(projectRoot, "wrangler.toml"), "utf8");
const appIdMatch = wranglerToml.match(
  /CF_EASYTIER_CONTAINER_APP_ID\s*=\s*"([^"]+)"/,
);

if (!appIdMatch) {
  throw new Error("CF_EASYTIER_CONTAINER_APP_ID not found in wrangler.toml");
}

const appId = appIdMatch[1];
const { stdout } = await execFileAsync(
  "npx",
  ["wrangler", "containers", "instances", appId, "--json"],
  {
    cwd: projectRoot,
    maxBuffer: 10 * 1024 * 1024,
  },
);

const instances = JSON.parse(stdout);
const headers = {
  "content-type": "application/json",
};

if (authToken) {
  headers.authorization = `Bearer ${authToken}`;
}

const response = await fetch(new URL("/api/admin/instances/sync", workerUrl), {
  method: "PUT",
  headers,
  body: JSON.stringify({
    syncedAt: new Date().toISOString(),
    instances,
  }),
});

const text = await response.text();
if (!response.ok) {
  throw new Error(`sync failed: ${response.status} ${text}`);
}

console.log(text);
