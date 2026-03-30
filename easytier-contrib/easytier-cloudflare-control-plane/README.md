# EasyTier Cloudflare Control Plane

这个目录现在是一个尽量精简的 Hono + Cloudflare Workers 项目。

当前版本的目标是:

- 客户端仍然只连 `wss://et.icewhale.io/`
- Worker 读取 EasyTier 的第一帧握手消息
- 从握手里的 `network_name` 提取分流键
- 同一个 `network_name` 永远粘到同一个 Cloudflare Container 实例
- 首次分配时按请求来源大陆选择一个 `locationHint`
- `/api/routes/:networkName/where` 通过外部同步的实例目录查询真实容器位置

## 现在的结构

- `src/index.ts`
  - Hono 入口
  - 路由、错误处理、可选 API Bearer 鉴权
  - WebSocket 首帧解析和双向桥接
- `src/easytier-container.ts`
  - EasyTier 容器 Durable Object
  - 每个网络最终都会绑定到某一个实例名
- `src/network-router.ts`
  - 每个 `network_name` 一个 Durable Object
  - 负责把网络名绑定到实例名和地区 hint
- `src/instance-catalog.ts`
  - 存放由外部同步任务写入的 `instanceName -> location/state/version`
- `src/instance-catalog-sync.ts`
  - 负责 Cloudflare Containers 实例拉取、目录同步和 `/where` 的 miss 自愈
- `src/easytier-proto.ts`
  - 只解析首帧里需要的最小 protobuf 字段
- `src/config.ts`
  - 配置解析和 shared relay 默认配置

删掉的内容:

- `NetworkRegistry`
- 区域分片和 shared group 逻辑
- 手写路由匹配器
- 额外的代理/运行时同步层

## 路由

- `GET /`
  - 返回一个简单说明页
  - 如果是 WebSocket Upgrade，请求会直接代理到容器
- `GET /api/config-example`
- `GET /api/instance`
- `PUT /api/instance`
- `DELETE /api/instance`
- `POST /api/instance/start`
- `POST /api/instance/stop`
- `GET /api/routes/:networkName`
- `DELETE /api/routes/:networkName`
- `GET /api/routes/:networkName/instance`
- `GET /api/routes/:networkName/where`
- `WS /connect`

## 示例配置

```json
{
  "instanceName": "shared",
  "networkName": "demo-network",
  "networkSecret": "demo-secret",
  "noTun": true,
  "listeners": [
    "ws://0.0.0.0:11011/"
  ],
  "peers": [],
  "extraArgs": []
}
```

## 本地开发

```bash
cd easytier-contrib/easytier-cloudflare-control-plane
pnpm install --ignore-workspace --lockfile=false
pnpm cf-typegen
pnpm dev
```

## 调用示例

```bash
curl -X PUT http://127.0.0.1:8787/api/instance \
  -H 'content-type: application/json' \
  -d '{"networkName":"demo","networkSecret":"demo","noTun":true}'

curl -X POST http://127.0.0.1:8787/api/instance/start
```

WebSocket 接入:

```text
wss://<your-worker-domain>/connect
```

或者兼容根路径:

```text
wss://<your-worker-domain>/
```

## 可选鉴权

如果设置了 `API_AUTH_TOKEN` secret，则所有 `/api/*` 请求都需要:

```text
Authorization: Bearer <token>
```

设置方式:

```bash
wrangler secret put API_AUTH_TOKEN
```

如果要让 Worker 自动刷新实例目录，还需要一个有 Containers 权限的 Cloudflare API token。当前代码会优先读取现有的 `CF_CONTAINERS_API_TOKEN` secret，也兼容以下名字：

```bash
wrangler secret put CF_CONTAINERS_API_TOKEN
# 或
wrangler secret put CF_API_TOKEN
# 或
wrangler secret put CLOUDFLARE_API_TOKEN
```

当前实现会每 5 分钟自动同步一次实例目录，并且在 `/api/routes/:networkName/where` 遇到 catalog miss 时尝试即时刷新一次。

`/api/routes/:networkName/where` 里的 `instance` 现在只保留这些字段：

- `source`
- `instance`
- `reason` 仅在 `instance-catalog-error` 时返回

## 部署说明

这个版本故意去掉了 `account_id`、`routes`、多余 `vars` 等部署期配置，方便把它当成一个可移植的最小模板。如果你有固定域名或账号绑定，再按你的环境把这些配置加回 `wrangler.toml`。

当前 `/api/routes/:networkName/where` 依赖外部同步的实例目录。同步方式：

```bash
cd easytier-contrib/easytier-cloudflare-control-plane
API_AUTH_TOKEN=<worker-api-token> npm run sync:instances
```

同步脚本会调用：

- `wrangler containers instances <APP_ID> --json`
- `PUT /api/admin/instances/sync`

如果 `/api/routes/:networkName/where` 里出现 `instance.source = "instance-catalog-miss"`，表示路由本身是存在的，但外部实例目录当前还没有同步到这个 `route.instanceName`，不是 Worker 路由解析失败。

如果你之前已经部署过带 `NetworkRegistry` 的旧版本，并且要原地升级这个 Worker，需要按 Cloudflare Durable Objects 迁移规则，额外补一条删除旧类的 migration。这个最小版没有保留那段配置，是为了保证新项目能直接跑起来。

## 重要限制

- 路由键只使用 `network_name`
- 不修改客户端，所以不能使用 URL path、query 或 header 传递房间信息
- 这意味着如果两个不同网络误用了同一个 `network_name`，它们会被路由到同一个 relay 实例
- `/where` 返回的服务端实例信息来自外部同步的实例目录，不是 EasyTier `hostname`
