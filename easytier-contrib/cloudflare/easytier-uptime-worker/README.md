# EasyTier Cloudflare Uptime Worker

Cloudflare Workers Rust 版本的 `easytier-uptime` 后端。

当前阶段先落这几件事：

- 用 `workers-rs` + `axum` 保留现有 REST API 组织方式
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
npm run dev
```

在创建出真实的 `database_id` 和 `namespace_id` 之后，更新 `wrangler.toml` 里的占位值。

