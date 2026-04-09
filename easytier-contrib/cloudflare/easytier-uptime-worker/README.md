# EasyTier Cloudflare Uptime Worker

Cloudflare Workers Rust 版本的 `easytier-uptime` 后端。

当前阶段先落这几件事：

- 用 `workers-rs` + `axum` 保留现有 REST API 组织方式
- 复用 `easytier-contrib/easytier-uptime/frontend` 作为前端静态站点
- 用 `D1` 持久化节点与健康记录
- 用 `KV` 持久化管理员 session token
- 用 `scheduled` 事件替代常驻后台健康检查任务

## 本地开发

```bash
cd easytier-contrib/cloudflare/easytier-uptime-worker
npm install
rustup target add wasm32-unknown-unknown
npx wrangler d1 create easytier-uptime
npx wrangler kv namespace create ADMIN_SESSIONS
npx wrangler d1 migrations apply easytier-uptime --local
npm run dev
```

在创建出真实的 `database_id` 和 `namespace_id` 之后，更新 `wrangler.toml` 里的占位值。

## 部署前初始化

```bash
npx wrangler d1 migrations apply easytier-uptime
```

`wrangler.toml` 里已经声明了:

- `UPTIME_DB`: D1 数据库
- `ADMIN_SESSIONS`: 管理员 session KV
- `*/5 * * * *`: 每 5 分钟一次的健康检查 cron

## 当前实现范围

- 保留了 `uptime` 的核心 REST API:
  - `/health`
  - `/api/nodes`
  - `/api/nodes/:id`
  - `/api/tags`
  - `/api/test_connection`
  - `/api/nodes/:id/health`
  - `/api/nodes/:id/health/stats`
- `/api/admin/*`
- 节点、标签、健康记录全部落在 D1
- 管理员 token 落在 KV
- 定时健康检查会写回:
  - `shared_nodes.is_active`
  - `health_records`
- `uptime.icewhale.io` 同时承载:
  - Vue SPA 静态前端
  - Worker API

## 当前探测语义

Cloudflare Worker 版本不再启动 `easytier-core` 实例去做原始的 peer 级探测，而是按协议做最小连通性探测：

- `tcp` / `ws`: 直接 TCP 建连
- `wss`: TLS 建连
- `udp`: 记录为 `unsupported`

这样可以尽量保留原有 API 和页面行为，同时符合 Workers 的运行模型，代码也更精简。
