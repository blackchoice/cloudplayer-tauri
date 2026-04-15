# CloudPlayer Tauri 项目审查报告

- 审查日期：2026-04-15
- 审查范围：前端（`src/`）、Tauri 配置与权限（`src-tauri/tauri.conf.json`、`src-tauri/permissions/`）、后端命令与核心模块（`src-tauri/src/`）
- 审查方式：静态代码审查 + 本地构建验证尝试

## 结论摘要

当前项目功能结构清晰、模块边界基本合理，但存在 1 个高风险安全配置问题和 2 个中风险功能/稳定性问题，建议优先处理安全配置与 ACL 缺口。

## 主要发现（按严重级别）

### 1) 高风险：Tauri 安全面过宽（`csp: null` + 资产协议全路径）

- 证据：`src-tauri/tauri.conf.json:23`、`src-tauri/tauri.conf.json:26`
- 现状：
  - `"csp": null`
  - `assetProtocol.scope` 包含 `"**"`（同时包含 `$HOME/**`）
- 影响：一旦渲染层出现注入点（或第三方资源被污染），攻击面将显著扩大，可能触达本地文件资源与高权限 IPC 能力。
- 建议：
  - 启用并收紧 CSP（至少限制脚本来源为 `self`，避免内联脚本执行）。
  - 将 `assetProtocol.scope` 缩小到最小必要目录（例如仅缓存/下载目录），移除 `"**"`。

### 2) 中风险：主窗口 ACL 缺少已调用命令，导致功能静默失效

- 证据：
  - 前端调用：`src/main.js:2101`（`invoke("fetch_lrc_cx_cover")`）
  - 命令已注册：`src-tauri/src/lib.rs:141`
  - ACL 未放行：`src-tauri/permissions/main-app.toml:4`
- 现状：`allow-main-app` 的 `commands.allow` 中缺少 `fetch_lrc_cx_cover`。
- 影响：播放时自动补封面逻辑会因为权限拒绝失败，前端仅 `console.warn`，用户侧表现为“偶发无封面”。
- 建议：在 `allow-main-app` 增加 `"fetch_lrc_cx_cover"`，并在前端对权限拒绝错误给出可观测提示（可选）。

### 3) 中风险：下载实现并未流式写盘，存在高内存占用风险

- 证据：`src-tauri/src/download.rs:357`、`src-tauri/src/download.rs:375`
- 现状：使用 `resp.bytes().await` 一次性读入完整音频，再 `write_all` 落盘。
- 影响：大文件（尤其 FLAC）下载时内存峰值高，可能导致卡顿或失败；同时进度反馈不够精细。
- 建议：改为 `bytes_stream()` 分块写入文件，边下边写并更新进度。

### 4) 低风险：README 与实际打包配置不一致

- 证据：
  - 文档写法：`README.md:75`（示例为 `bundle.active = false`、`targets = all`）
  - 实际配置：`src-tauri/tauri.conf.json:31`（`active = true`）、`src-tauri/tauri.conf.json:32`（`targets = nsis`）
- 影响：发布流程说明容易误导维护者。
- 建议：同步更新 README 的“当前配置”片段。

## 构建验证结果

以下命令已尝试，但受当前环境限制未完成：

1. `npm run build`
- 结果：失败（`spawn EPERM`，esbuild 子进程拉起失败）

2. `cargo check --manifest-path src-tauri/Cargo.toml`
- 结果：失败（`src-tauri/target/debug/.cargo-lock` 打开被拒绝，`os error 5`）

## 审查备注

- 本次为静态审查，未执行端到端 UI 回归。
- 建议先修复第 1、2 项后再进行一次完整构建与功能回归（搜索、播放、封面补全、下载）。
