# DeployKeys Desktop 快速开始

## 前置要求

1. **安装 Rust 工具链**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **安装系统依赖**

   macOS:
   ```bash
   # 如果使用 Homebrew
   brew install pkg-config
   ```

   Linux (Ubuntu/Debian):
   ```bash
   sudo apt-get update
   sudo apt-get install -y pkg-config libssl-dev libsqlite3-dev
   ```

## 构建项目

```bash
# 进入项目目录
cd /Users/virtualgemini/Workspace/project/deploykeys-desktop

# 检查代码（首次会下载依赖）
cargo check --workspace

# 运行测试
cargo test --workspace

# 构建 debug 版本
cargo build --workspace

# 构建 release 版本（优化编译）
cargo build --release --workspace
```

## 运行应用

```bash
# 运行 GUI 应用（debug 模式）
cargo run -p deploykeys-gui

# 运行 GUI 应用（release 模式）
cargo run --release -p deploykeys-gui

# 设置日志级别
RUST_LOG=debug cargo run -p deploykeys-gui
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
```bash
# 安装 cargo-watch
cargo install cargo-watch

# 监听并自动重新运行
cargo watch -x 'run -p deploykeys-gui'
```

### 4. 清理构建产物
```bash
cargo clean
```

## 项目结构速览

```
deploykeys-desktop/
├── crates/
│   ├── deploykeys-core/          # 核心逻辑库
│   │   ├── models/            # 数据模型
│   │   ├── db/                # 数据库访问
│   │   ├── github/            # GitHub API
│   │   ├── ssh/               # SSH 操作
│   │   ├── keygen/            # Key 生成
│   │   └── credentials/       # 凭据管理
│   │
│   └── deploykeys-gui/           # GUI 应用
│       ├── app.rs             # 主应用
│       ├── messages.rs        # 消息定义
│       └── screens/           # 界面
│
└── migrations/                # 数据库 schema
```

## 常见问题

### Q: 编译时提示找不到 OpenSSL
**A:** 安装系统 OpenSSL 开发包：
```bash
# macOS
brew install openssl

# Linux
sudo apt-get install libssl-dev
```

### Q: Iced 编译慢
**A:** 第一次编译 Iced 需要较长时间（5-10 分钟），之后增量编译会快很多。可以使用 `--release` 模式获得更好的性能。

### Q: 如何启用更详细的日志
**A:** 设置环境变量：
```bash
RUST_LOG=deploykeys_core=debug,deploykeys_gui=debug cargo run -p deploykeys-gui
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
