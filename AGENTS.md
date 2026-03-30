# EasyTier Fork Agent Guide

本文件是本仓库内 Agent 的执行约束，重点覆盖 CI/CD 改造与分支策略。

## 1. 总原则

- 优先最小侵入 upstream。
- 优先改 workflow、脚本、配置，不优先改核心 Rust 逻辑。
- 保留现有手动发布入口，自动化是增量补充，不是粗暴替换。
- 任何新加的自动化都必须可回滚、可关闭、可按分支约束。

## 2. 分支语义

- `main`、`develop`、`releases/**`
  - 视为 upstream 兼容分支。
  - CI 行为尽量保持与原仓库语义一致。

- `dev`
  - 视为持续集成分支。
  - 每次 push 都应稳定产出核心制品，不依赖 tag。
  - 允许自动 Docker 镜像构建与自动 release 包打包。

- `icewhale/**`
  - 视为内部定制分支。
  - 允许定制 CI 规则。
  - 但应避免仅因 workflow 配置调整触发重型构建。

## 3. CI 目标

### 3.1 `dev` 分支

- 每次 push 都应触发：
  - `core` 产物构建
  - `gui` 产物构建
  - `mobile` 产物构建
  - `docker` 镜像构建与推送
  - `release` 包自动打包

- `dev` 分支上的自动产物规则：
  - Docker 镜像不要求 tag 才触发。
  - Release 包不要求 tag 才生成。
  - 自动打包产物可先作为 workflow artifact 存储。
  - 正式 GitHub Release 仍可保留手动流程。

### 3.2 `icewhale/**` 分支

- 如果一次 push 只修改了 `.github/workflows/*.yml`：
  - 应跳过重型构建任务。
  - 尤其不要仅因 `gui.yml`、`mobile.yml`、`ohos.yml` 等 workflow 文件变动而触发对应重活。

- 如果同时改了代码或构建输入：
  - 可以照常触发对应 workflow。

### 3.3 手动流程

- `docker.yml`
  - 保留 `workflow_dispatch`。
  - 自动模式只能作为手动模式的补充。

- `release.yml`
  - 保留 `workflow_dispatch`。
  - 自动打包不应阻断手动 draft release 流程。

## 4. Docker 规则

- Docker 镜像构建优先依赖 `core` workflow 的 artifact。
- `dev` 分支自动推送的 tag 至少应包含：
  - `dev`
  - `dev-<shortsha>`

- 不要要求每次打 tag 才能生成镜像。
- 不要让 Docker workflow 与 core workflow 重复执行同一套二进制构建。

## 5. Release 规则

- Release 包打包依赖：
  - `core`
  - `gui`
  - `mobile`

- `dev` 分支自动打包时：
  - 优先按同一 `head_sha` 关联三套 workflow run。
  - 产物至少应以 artifact 形式保留。

- Tag 发布仍可额外使用 GitHub Release。
- 不应要求人工先手抄 run id 才能完成日常 dev 打包。

## 6. 改动边界

- 改 CI 时优先改：
  - `.github/workflows/*.yml`
  - `.github/actions/*`
  - 相关打包脚本

- 除非确实必要，不要为 CI 目的修改：
  - 核心连接逻辑
  - 路由逻辑
  - peer discovery 核心行为

## 7. Agent 执行要求

- 修改 workflow 后，至少做这些检查：
  - YAML 解析通过
  - 关键 `if` 条件可读
  - 分支触发范围明确
  - 自动与手动触发不冲突

- 如果仓库里有用户未提交改动：
  - 不要覆盖。
  - 不要顺手清理。
  - 只在目标文件范围内修改。

- 如果 CI 目标与本文件冲突：
  - 以本文件为准。
  - 如需偏离，先更新本文件，再改 workflow。

## 8. 当前优先级

当前仓库的 CI 改造优先级如下：

1. 让 `dev` 分支 push 自动产出 docker 镜像和 release 包。
2. 让 `icewhale/**` 分支忽略仅 workflow yml 变更导致的重型构建。
3. 保留手动触发流程作为兜底。
4. 尽量减少与 upstream 的长期分叉成本。
