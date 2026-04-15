# CloudPlayer Tauri

CloudPlayer desktop app built with Tauri + Rust + Vite.  
基于 Tauri + Rust + Vite 的 CloudPlayer 桌面应用。

---

## Dependency Check / 依赖检查

### Frontend dependencies / 前端依赖

- `vite`
- `@tauri-apps/cli`
- `@tauri-apps/api`
- `@tauri-apps/plugin-dialog`

### Backend dependencies / 后端依赖

Key dependencies from `src-tauri/Cargo.toml` include:  
`src-tauri/Cargo.toml` 的主要依赖包括：

- `tauri`, `tauri-build`, `tauri-plugin-dialog`
- `tokio`, `reqwest`, `rusqlite`, `serde`, `serde_json`
- `walkdir`, `regex`, `url`, `image`, `imageproc`, `rand`, `chrono`

### Check commands / 检查命令

```bash
npm install
npm outdated
cargo check --manifest-path src-tauri/Cargo.toml
```

---

## Build Service / 构建服务

### Environment / 环境要求

- Node.js 18+ (LTS recommended) / Node.js 18+（建议 LTS）
- npm
- Rust stable toolchain
- Tauri prerequisites on Windows (WebView2, MSVC build tools)  
  Windows Tauri 前置环境（WebView2、MSVC 构建工具）

Version check / 版本检查：

```bash
node -v
npm -v
rustc -V
cargo -V
```

### Dev mode / 开发模式

```bash
npm run dev
npm run tauri dev
```

### Production build / 生产构建

```bash
npm run build
```

Frontend output is `dist/`, used by Tauri as `frontendDist: ../dist`.  
前端产物输出到 `dist/`，并由 Tauri 的 `frontendDist: ../dist` 使用。

---

## Release Build (Local) / 本地 Release 构建

Current config in `src-tauri/tauri.conf.json`:  
`src-tauri/tauri.conf.json` 当前配置：

```json
"bundle": {
  "active": false,
  "targets": "all"
}
```

If `bundle.active = false`, installer packages will NOT be generated.  
若 `bundle.active = false`，不会生成安装包。

To build release package / 构建 release 安装包：

1. Set `bundle.active` to `true`
2. Run:

```bash
npm run tauri build
```

Output path / 产物目录：

- `src-tauri/target/release/`
- `src-tauri/target/release/bundle/`

---

## GitHub Release / GitHub 发布

This section is for GitHub Release (uploading binary assets to a versioned release page).  
本节是 GitHub Release（将安装包上传到版本发布页）的流程。

### 1) Prepare version / 准备版本

Keep versions aligned in:
请同步更新以下版本号：

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

Recommended tag format: `v0.1.0`  
建议 tag 格式：`v0.1.0`

### 2) Build artifacts / 生成发布产物

```bash
npm run tauri build
```

Collect assets from `src-tauri/target/release/bundle/` (e.g. `.msi`, `.exe`, etc).  
从 `src-tauri/target/release/bundle/` 收集产物（如 `.msi`、`.exe` 等）。

### 3A) Create release in GitHub Web UI / 在 GitHub 网页创建 Release

1. Push commits and tag:

```bash
git add .
git commit -m "release: v0.1.0"
git tag v0.1.0
git push origin main --tags
```

2. Open your GitHub repo -> **Releases** -> **Draft a new release**
3. Select tag `v0.1.0`
4. Upload files from `src-tauri/target/release/bundle/`
5. Publish release

### 3B) Create release with GitHub CLI / 用 GitHub CLI 创建 Release

```bash
gh release create v0.1.0 ^
  "src-tauri/target/release/bundle/**" ^
  --title "v0.1.0" ^
  --notes "CloudPlayer v0.1.0 release"
```

If wildcard upload fails on your shell, upload files explicitly.  
如果你的 shell 不支持通配符上传，请改为逐个文件路径上传。

---

## Release Checklist / 发布检查清单

- `npm install` and `cargo check` pass
- `bundle.active = true`
- Version numbers are synced
- `npm run tauri build` artifacts verified
- Git tag created and pushed
- GitHub Release published with installer assets

