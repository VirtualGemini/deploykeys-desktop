# DeployKeys Desktop 快速开始

## 前置要求

1. **安装 Rust 工具链**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **安装 wasm 目标与前端/桌面工具链**
   ```bash
   # 前端是 Leptos CSR，编译到 wasm
   rustup target add wasm32-unknown-unknown

   # Trunk 构建前端 wasm bundle
   cargo install trunk

   # Tauri CLI 驱动 dev/build（提供 cargo tauri 子命令）
   cargo install tauri-cli --version "^2"
   ```

   > Tailwind 无需 Node：`tools/` 下有固定版本的 standalone Tailwind v4 二进制，
   > 由 Trunk 的 pre-build hook 自动调用。若缺失，跑一次 `tools/install-tailwind.sh`
   > 拉取即可。

3. **安装系统依赖**

   macOS:
   ```bash
   # 如果使用 Homebrew
   brew install pkg-config
   ```

   Linux (Ubuntu/Debian)，Tauri 需要 WebKitGTK 等：
   ```bash
   sudo apt-get update
   sudo apt-get install -y pkg-config libssl-dev libsqlite3-dev \
     libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
     libayatana-appindicator3-dev
   ```

## 构建项目

```bash
# 进入项目目录
cd /Users/virtualgemini/Workspace/project/deploykeys-desktop

# 生成 sqlx 编译期校验库（首次必需）
make db-setup

# 检查原生 crate（首次会下载依赖）
cargo check

# 运行测试（原生 crate；wasm-only 的 UI crate 已排除在 default-members 外）
cargo test

# 只构建前端 wasm bundle（Trunk）
make ui-build

# 打包 release 桌面应用（Tauri 跑 beforeBuildCommand 先出前端，再 bundle）
make build        # 等价于 cargo tauri build
```

## 运行应用

开发模式由 Tauri 驱动：它会先起 Trunk dev server（`beforeDevCommand`），
再开桌面窗口指向它，支持前端热重载。

```bash
# 开发模式（热重载）
make run                 # 等价于 cargo tauri dev

# 直接用 Tauri CLI
cargo tauri dev

# 设置日志级别（作用于原生侧）
RUST_LOG=debug cargo tauri dev
```

## 开发工作流

### 1. 代码格式化
```bash
cargo fmt --all
```

### 2. 代码检查（Clippy）
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### 3. 监听文件变化自动重新编译
`cargo tauri dev` 已自带热重载：前端由 Trunk dev server 监听 `deploykeys-ui`
源码即时重建 wasm；改动原生 crate（core/gui）时 Tauri 会重编并重启窗口。
一般直接 `make run` 即可，无需额外 watch 工具。

### 4. 清理构建产物
```bash
cargo clean
```

## 项目结构速览

```
deploykeys-desktop/
├── crates/
│   ├── deploykeys-core/          # 核心逻辑库（原生，无 UI 依赖）
│   │   ├── models/            # 数据模型
│   │   ├── db/                # 数据库访问
│   │   ├── github/            # GitHub API
│   │   ├── ssh/               # SSH 操作
│   │   ├── keygen/            # Key 生成
│   │   └── credentials/       # 凭据管理
│   │
│   ├── deploykeys-app/           # Tauri 2 原生宿主（二进制 `deploykeys`）
│   │   ├── src/lib.rs         # IPC 命令面 + AppState + 事件循环
│   │   ├── tauri.conf.json    # Tauri 配置
│   │   └── capabilities/      # 权限能力
│   │
│   └── deploykeys-ui/            # Leptos CSR 前端（Trunk 构建为 wasm）
│       ├── src/app.rs        # 根组件、屏幕状态、设备流轮询
│       ├── src/tauri.rs      # IPC invoke 桥
│       ├── src/i18n.rs       # 内联词条表 + 响应式 locale
│       ├── src/theme.rs      # 响应式主题信号 + .dark 切换（见 docs/THEME_DESIGN.md）
│       ├── src/screens/      # OAuth 界面
│       └── styles/           # Tailwind 输入/输出 CSS（input.css = 全局色板）
│
├── migrations/                # 数据库 schema
└── tools/                     # 固定的 Tailwind v4 standalone 二进制
```

## 常见问题

### Q: Linux 上构建 Tauri 报缺少系统库
**A:** Tauri 的 webview 依赖 webkit2gtk 等系统库（reqwest 走 rustls，
不需要 OpenSSL）。Ubuntu/Debian：
```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential \
  curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```
macOS 无需额外系统库。

### Q: 首次构建很慢
**A:** 首次需编译整个原生依赖树并构建 wasm 前端，耗时较长；之后增量编译会快很多。打 release 包用 `make build`（`cargo tauri build`）。

### Q: 提示找不到 `cargo tauri` 或 `trunk`
**A:** 这两个是独立 CLI，需先安装：
```bash
cargo install tauri-cli --version "^2"
cargo install trunk
rustup target add wasm32-unknown-unknown
```

### Q: 提示找不到 `tools/tailwindcss`
**A:** Trunk 的 pre-build hook 依赖 `tools/` 下固定的 Tailwind v4 standalone 二进制。缺失时跑一次 `tools/install-tailwind.sh` 拉取。

### Q: 如何启用更详细的日志
**A:** 设置环境变量（作用于原生侧）：
```bash
RUST_LOG=deploykeys_core=debug,deploykeys_lib=debug cargo tauri dev
```

### Q: 数据库文件在哪里
**A:** 运行时数据库位置：
- macOS: `~/Library/Application Support/deploykeys/deploykeys.db`
- Linux: `~/.local/share/deploykeys/deploykeys.db`

仓库根目录的 `deploykeys.db` 仅用于 sqlx 编译期 SQL 校验（`make db-setup` 生成），与运行时数据无关。

## 下一步

查看 [STATUS.md](./STATUS.md) 了解当前真实进度与已知技术债。

查看 [PRD.md](./PRD.md) 了解完整的产品需求文档。

## 贡献

1. Fork 项目
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

## 许可证

MIT License - 详见 LICENSE 文件
