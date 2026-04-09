# EasyTier Cloudflare Control Plane

第一阶段目标只有一件事:

- 直接使用 `docker.io/easytier/easytier:v2.5.0`
- 在 Cloudflare Container 里启动 `easytier-core`
- 通过 Worker 暴露 `WS /connect`
- 完成客户端和容器内 `easytier-core` 的 WebSocket 握手与通信

这个阶段故意不做多实例分流，也不做 network router。先把最小链路跑通:

`easytier-core client -> Cloudflare Worker -> Cloudflare Container -> easytier-core server`

## 当前结构

- `src/index.ts`
  - Worker 入口
  - `GET /healthz`
  - `GET /api/network-route`
  - `WS /connect`
  - WebSocket upgrade 直接透传到 Container
- `src/easytier-container.ts`
  - Cloudflare Container Durable Object
  - 保存实例配置
  - 以 `easytier-core` 作为容器入口启动 EasyTier
  - 自己处理 Container 内 WebSocket 代理，避免 Cloudflare 容器库的默认代理把二进制帧错误处理成文本
- `wrangler.toml`
  - 直接引用 `docker.io/easytier/easytier:v2.5.0`
  - 开启 `[placement] mode = "smart"`，让 fetch 请求按 Smart Placement 选择更优执行位置

## 本地开发

```bash
cd easytier-contrib/cloudflare/easytier-cloudflare-control-plane
npm install
npm run cf-typegen
npm run dev
```

## Placement

当前配置已经切到 `wrangler.toml`，并启用了:

```toml
[placement]
mode = "smart"
```

Cloudflare 官方文档说明 Smart Placement 只影响 `fetch` handler，并且在部署后最多可能需要大约 15 分钟完成分析并开始稳定按更优位置调度。

## 查询 networkName 当前分配地区

如果你需要知道某个 `networkName` 当前已经被分配到了哪个地区，可以查询:

```bash
curl "http://127.0.0.1:8787/api/network-route?networkName=stage1-demo"
```

返回里会包含:

- `assignedRegion`: 当前分配到的地区代码，等同于内部的 `locationHint`
- `instanceName`: 当前命中的共享入口实例
- `routeMode`: 实际持久化使用的路由模式
- `lookupMode`: 本次查询使用的查找模式
- `lookupLatencyMs`: 本次 Worker 查询路由记录的耗时，单位毫秒

如果你手里只有 `networkSecret` 原文，可以让服务端代算 digest，再查同一条精确路由。对包含 `?`、`&` 之类特殊字符的 secret，推荐用 `--data-urlencode`:

```bash
curl -G "http://127.0.0.1:8787/api/network-route" \
  --data-urlencode "networkName=stage1-demo" \
  --data-urlencode "networkSecret=stage1-demo"
```

如果你已经拿到了握手里的 `secretDigestHex`，也可以直接查:

```bash
curl -G "http://127.0.0.1:8787/api/network-route" \
  --data-urlencode "networkName=stage1-demo" \
  --data-urlencode "secretDigestHex=<hex>"
```

说明:

- 不带 `secretDigestHex` 时，查询的是 `network:${networkName}` 这条 `network-name-only` 路由。
- 带 `networkSecret` 时，服务端会先按 EasyTier 的算法计算 `secretDigestHex`，再查询 `digest:${networkName}:${secretDigestHex}`。
- 带 `secretDigestHex` 时，查询的是 `digest:${networkName}:${secretDigestHex}` 这条精确路由。
- 如果同时传了 `networkSecret` 和 `secretDigestHex`，接口会校验两者是否一致。
- 如果对应路由还没被首次握手创建，接口会返回 `404`。

## 客户端验证

启动本地客户端，经由 Worker 的 `/connect` 接入:

```bash
cargo run -p easytier --bin easytier-core -- \
  --network-name stage1-demo \
  --network-secret stage1-demo \
  --no-tun \
  -p ws://127.0.0.1:8787/connect \
  --rpc-portal 127.0.0.1:25888 \
  --hostname cf-client
```

或者直接用官方 Docker 镜像做客户端验证:

```bash
docker run --rm docker.io/easytier/easytier:v2.5.0 \
  --network-name stage1-demo \
  --network-secret stage1-demo \
  --no-tun \
  --no-listener \
  --disable-p2p true \
  -p wss://easytier-cloudflare-control-plane.icewhale.workers.dev/connect
```

再用 CLI 查看客户端是否已经握手成功并出现收发数据:

```bash
cargo run -p easytier --bin easytier-cli -- \
  -p 127.0.0.1:25888 \
  -o json \
  peer list
```

## 本地已知限制

当前在 `wrangler dev` + OrbStack 的本地模式下，Worker 能接受 EasyTier WebSocket Upgrade，也能解析并记录客户端首帧里的 `network_name`，但在把请求继续转发给本地 Container 时，Wrangler 的容器就绪探测会报:

```text
Network connection lost.
Container failed to start
```

这会导致最终的上游 `super.fetch()` WebSocket upgrade 返回 500。当前代码已经尽量把业务逻辑和这个本地运行时问题解耦，但如果你要验证完整的“客户端握手并持续通信”，更可靠的方式是：

- 在原生 Linux Docker 环境运行 `wrangler dev`
- 或直接部署到真实 Cloudflare 环境后验证

## 第一阶段成功标准

- Worker 能接受 EasyTier WebSocket Upgrade
- Worker 能把连接转发到 `easytier/easytier:v2.5.0`
- 容器内 `easytier-core` 成功监听 `ws://0.0.0.0:11011/`
- 客户端 `easytier-core` 能通过 `/connect` 完成握手
- `easytier-cli peer list` 能看到有效 peer，且有真实收发数据

当前线上验证结果:

- 部署地址:
  - `https://easytier.icewhale.workers.dev`
  - `https://et.icewhale.io`
- 官方 `easytier/easytier:v2.5.0` 客户端已经能经由 `/connect` 建立 `tunnel_proto = "wss"` 的 peer
- `peer list` 中对应 peer 的 `rx_bytes` / `tx_bytes` 已经为非 0
