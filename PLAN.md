# DeployKeys Desktop 实现计划

## Context

DeployKeys Desktop 是一个从零开始的 GitHub Deploy Key 自动化配置工具。用户通过桌面 GUI 在指定目标环境（本地或远程服务器）生成 SSH Key，并自动绑定公钥到 GitHub 仓库，私钥永远留在目标环境。

**核心价值**：让开发者通过 GUI 安全地授权目标环境访问 GitHub 私有仓库，无需手动配置 SSH、复制公钥或创建 Deploy Key。

**MVP 范围**: 仅支持 macOS 和 Linux，不支持 Windows。界面设计参考 Docker Desktop 的简洁风格（深色主题、卡片式布局、左侧导航）。

---

## 项目架构

### Cargo Workspace 结构

```
deploykeys-desktop/
├── Cargo.toml                    # Workspace 根（default-members 排除 wasm-only 的 ui crate）
├── crates/
│   ├── deploykeys-core/            # 核心业务逻辑库（原生，无 UI 依赖）
│   │   ├── src/
│   │   │   ├── models/          # 数据模型 (Account, Target, KeyBinding, Repository)
│   │   │   ├── db/              # SQLite 数据访问层
│   │   │   ├── github/          # GitHub API 客户端 (Device Flow, Deploy Keys API)
│   │   │   ├── ssh/             # SSH 操作抽象 (trait + russh 实现)
│   │   │   ├── keygen/          # SSH Key 生成 (本地: ssh-key crate, 远程: ssh-keygen)
│   │   │   ├── credentials/     # 系统凭据管理 (keyring crate)
│   │   │   ├── verification/    # 验证逻辑 (git ls-remote 测试)
│   │   │   └── services/        # 业务服务层 (AuthService, KeyBindingService, TargetService)
│   │   └── Cargo.toml
│   │
│   ├── deploykeys-app/             # Tauri 2 原生宿主（二进制 `deploykeys`）
│   │   ├── src/lib.rs           # IPC 命令面 + AppState + 事件循环
│   │   ├── src/main.rs
│   │   ├── tauri.conf.json      # Tauri 配置
│   │   ├── capabilities/        # 权限能力
│   │   └── Cargo.toml
│   │
│   └── deploykeys-ui/              # Leptos CSR 前端（Trunk 构建为 wasm）
│       ├── src/app.rs          # 根组件、屏幕状态、设备流轮询
│       ├── src/api.rs          # IPC 命令的前端视图（本地 DTO）
│       ├── src/tauri.rs        # IPC invoke 桥
│       ├── src/i18n.rs         # 内联词条表 + 响应式 locale
│       ├── src/screens/        # 各界面 (OAuth, …)
│       ├── styles/             # Tailwind 输入/输出 CSS
│       ├── Trunk.toml
│       └── Cargo.toml
├── migrations/                   # SQLite schema 迁移
└── tools/                        # 固定的 Tailwind v4 standalone 二进制 + 安装脚本
```

### 技术栈

- **桌面壳**: Tauri 2 (原生宿主 + webview)
- **前端**: Leptos 0.6 (CSR/wasm)，Trunk 构建
- **样式**: Tailwind v4 (tools/ 下固定 standalone 二进制，无 Node 依赖)，组件风格抄自 Preline
- **前后端通信**: Tauri IPC（`window.__TAURI__.core.invoke`），仅传脱敏 DTO
- **HTTP**: reqwest + rustls
- **SSH**: russh (优先) 或 ssh 命令 fallback
- **数据库**: SQLx + SQLite (异步)
- **凭据**: keyring (macOS Keychain/Linux Secret Service)
- **SSH Key**: ssh-key crate (支持 Ed25519, RSA, ECDSA 算法选择)
- **国际化**: ui crate 内 `i18n.rs` 内联词条表 + Leptos 响应式 `RwSignal`，默认英文（详见 docs/I18N_DESIGN.md）

---

## 开发约束（基本需求，所有改动必须遵守）

这些是项目级硬约束，不是可选项。新增界面、新增文案、新增错误路径时都必须满足，CI 通过测试强制其中一部分。

### C1. 国际化（i18n）

设计详见 [`docs/I18N_DESIGN.md`](docs/I18N_DESIGN.md)。要点：

- **默认语言 English**；首发支持 `en` / `zh`。运行时可切换，无需重启。
- **所有用户可见文案必须走词条**：前端 `crates/deploykeys-ui/src/i18n.rs` 内联词条表
  （`EN`/`ZH` 静态数组）+ `t("key")`。前端代码中**禁止硬编码面向用户的字符串**
  （含错误提示）。品牌名 `app.brand` 也入词条。
- **`deploykeys-core` 不做本地化**：核心层只返回稳定的英文技术错误；前端负责呈现。
  IPC 命令以 `Result<_, String>` 形式回传错误字符串，前端展示在对应界面。
- **新增语言**：在 `i18n.rs` 加一张词条数组（键集对齐 `EN`）+ 在 `Locale` 枚举注册。
- **CJK 字体**：webview 走系统字体栈，由 OS 提供中文字形；无需应用内嵌字体。
- 语言偏好持久化在 `app_settings` 表（迁移 `002_settings.sql`），由原生命令
  `get_language` / `set_language` 读写。启动顺序：持久化选择 → webview 语言 → 英文兜底。

---

## MVP 实现路线图

# DeployKeys Desktop - Implementation Plan

## Overview

**Objective**: Build a secure, user-friendly GitHub Deploy Key management desktop application

**Core Value**: Target-based key generation, repository authorization, private keys never leave the environment

**Tech Stack**: Rust + Tauri 2 (desktop shell) + Leptos/wasm (frontend) + SQLite + SQLx

**Scope**: macOS and Linux support only (no Windows in MVP)

**Design Reference**: Docker Desktop's minimalist style (dark theme, card-based layout, sidebar navigation)

---

## Architecture

### Cargo Workspace Structure

```
deploykeys-desktop/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── deploykeys-core/            # Core business logic (no UI dependencies)
│   │   ├── src/
│   │   │   ├── models/          # Data models (Account, Target, KeyBinding, Repository)
│   │   │   ├── db/              # SQLite data access layer
│   │   │   ├── github/          # GitHub API client (Device Flow, Deploy Keys API)
│   │   │   ├── ssh/             # SSH operations abstraction (trait + russh impl)
│   │   │   ├── keygen/          # SSH key generation (local: ssh-key crate, remote: ssh-keygen)
│   │   │   ├── credentials/     # System credential management (keyring crate)
│   │   │   ├── verification/    # Verification logic (git ls-remote test)
│   │   │   └── services/        # Business service layer (AuthService, KeyBindingService, TargetService)
│   │   └── Cargo.toml
│   │
│   ├── deploykeys-app/             # Tauri 2 native host (binary: `deploykeys`)
│   │   ├── src/lib.rs           # IPC command surface + AppState + event loop
│   │   ├── tauri.conf.json      # Tauri configuration
│   │   └── capabilities/        # Permission capabilities
│   │
│   └── deploykeys-ui/             # Leptos CSR frontend, built to wasm by Trunk
│       ├── src/app.rs          # Root component, screen state, device-flow polling
│       ├── src/tauri.rs        # IPC invoke bridge
│       ├── src/i18n.rs         # Inline string table + reactive locale
│       ├── src/theme.rs        # Reactive theme signal + global .dark toggle
│       ├── src/screens/        # OAuth screen
│       └── styles/             # Tailwind input/output CSS (semantic tokens)
├── migrations/                   # SQLite schema migrations
└── tools/                        # Pinned Tailwind v4 standalone binary
```

### Technology Stack

- **Desktop shell**: Tauri 2 (native host; webview frontend over IPC)
- **Frontend**: Leptos 0.6 (CSR/wasm), built by Trunk
- **Styling**: Tailwind v4 (pinned standalone binary in `tools/`, no Node toolchain); Preline-style utility classes
- **HTTP**: reqwest + rustls
- **SSH**: russh (priority) or ssh command fallback
- **Database**: SQLx + SQLite (async)
- **Credentials**: keyring (macOS Keychain/Linux Secret Service)
- **SSH Key**: ssh-key crate (supports Ed25519, RSA, ECDSA algorithm selection)

---

## Implementation Roadmap

### Phase 1: Project Skeleton & Data Layer (Week 1-2)

#### 1.1 Initialize Workspace
- Create Cargo.toml workspace configuration
- Create crates/deploykeys-core and crates/deploykeys-app sub-projects
- Configure shared dependencies and build settings
- Configure target platforms: macOS (x86_64-apple-darwin, aarch64-apple-darwin) and Linux (x86_64-unknown-linux-gnu)

#### 1.2 Data Model Design
**Implementation files**: `deploykeys-core/src/models/`
- `account.rs`: Account struct + AuthType enum
- `target.rs`: Target struct + TargetType/AuthMethod enum
- `repository.rs`: Repository struct
- `key_binding.rs`: KeyBinding struct + KeyAlgorithm/DeployKeyPermission/KeyBindingStatus enum
  - KeyAlgorithm: Ed25519 / Rsa2048 / Rsa4096 / EcdsaP256 / EcdsaP384 / EcdsaP521

#### 1.3 SQLite Database Layer
- `migrations/001_initial.sql`: Create accounts, targets, repositories, key_bindings, github_installations tables
- `deploykeys-core/src/db/mod.rs`: Database connection pool management
- `deploykeys-core/src/db/*_repository.rs`: Implement CRUD for each model (using sqlx macros)

#### 1.4 Credential Management
**Implementation file**: `deploykeys-core/src/credentials/mod.rs`
- `CredentialStore::store_token()`: Store GitHub token to system keyring
- `CredentialStore::get_token()`: Read token
- `CredentialStore::store_ssh_password()`: Store SSH password

**Key Point**: SQLite only saves token_ref (e.g., "github_token_user123"), not plaintext.

---

### Phase 2: GitHub Authorization Flow (Week 3)

#### 2.1 GitHub API 客户端基础
**Implementation file**: `deploykeys-core/src/github/client.rs`
- `GitHubClient::new()`: 使用 reqwest + rustls
- `GitHubClient::request()`: 通用请求方法，自动添加 Authorization header

#### 2.2 Device Flow 授权
**Implementation file**: `deploykeys-core/src/github/auth.rs`
- `DeviceFlowAuth::initiate()`: POST /login/device/code，返回 device_code 和 user_code
- `DeviceFlowAuth::poll_token()`: 轮询 POST /login/oauth/access_token
- `DeviceFlowAuth::refresh_token()`: 刷新过期 token

**流程**：
1. 用户点击 "Sign in with GitHub"
2. 显示 user_code 和二维码/链接
3. 后台轮询直到用户授权完成
4. 保存 access_token 和 refresh_token 到 keyring
5. 调用 /user API 获取用户信息

#### 2.3 Installations & Repositories API
**Implementation file**: `deploykeys-core/src/github/installations.rs`
- `list_installations()`: GET /user/installations
- `list_installation_repos()`: GET /user/installations/{id}/repositories
- 同步到本地 SQLite

---

### Phase 3: 本地 Target + Key 生成 (Week 4 周)

#### 3.1 本地 Target 初始化
**Implementation file**: `deploykeys-core/src/services/target_service.rs`
- `TargetService::create_local_target()`: 创建默认 "Local Machine"
- 检测 OS 类型 (macOS/Windows/Linux)
- 设置 key_base_dir 为 `~/.ssh/deploykeys/github.com/`

#### 3.2 本地 Key 生成
**Implementation file**: `deploykeys-core/src/keygen/local.rs`
- `LocalKeyGenerator::generate()`: 支持多种算法选择
  - Ed25519 (默认，推荐)
  - RSA 2048/4096
  - ECDSA P-256/P-384/P-521
- 写入 `~/.ssh/deploykeys/github.com/<owner>/<repo>/id_<algorithm>`
- 设置文件权限 (Unix: 600 for private, 644 for public)
- 返回 `KeyPair { algorithm, public_key, fingerprint, private_key_path }`

#### 3.3 Deploy Keys API
**Implementation file**: `deploykeys-core/src/github/deploy_keys.rs`
- `DeployKeyApi::create()`: POST /repos/{owner}/{repo}/keys
- `DeployKeyApi::list()`: GET /repos/{owner}/{repo}/keys
- `DeployKeyApi::delete()`: DELETE /repos/{owner}/{repo}/keys/{key_id}

#### 3.4 Key 绑定服务
**Implementation file**: `deploykeys-core/src/services/key_binding_service.rs`
- `KeyBindingService::create_binding()`: 完整流程
  1. 检查 repo × target 唯一性
  2. 生成本地 key
  3. 调用 GitHub API 创建 Deploy Key
  4. 保存 KeyBinding 到数据库
  5. 返回 KeyBinding 对象
- 补偿语义：上传失败清理本地私钥；落库失败回滚已创建的 GitHub key

---

### Phase 4: 基础 GUI (Week 5 周)

GUI 分两侧：原生宿主（`deploykeys-app`）暴露 IPC 命令并桥接 core；前端
（`deploykeys-ui`，Leptos CSR/wasm）渲染界面并经 IPC 调用命令。应用启动直接进入
主界面；GitHub 设备流登录从主界面顶栏按需触发，已端到端跑通；Repos/Targets/Keys/Forge
当前为占位界面。

#### 4.1 IPC 命令面 + 原生宿主
**Implementation file**: `deploykeys-app/src/lib.rs`
- `run()`：初始化数据库、注入 `AppState`、注册 IPC 命令、跑 Tauri 事件循环
- `AppState`：持有 `Database`（managed state，注入每个命令）
- 命令：`get_session` / `get_language` / `set_language` /
  `start_github_auth` / `poll_github_auth` / `open_url`
- 跨边界只用脱敏 DTO（`AccountDto`/`DeviceCodeDto`/`PollDto`），机密不外泄

#### 4.2 前端根组件 + IPC 桥
**Implementation file**: `deploykeys-ui/src/app.rs` + `src/tauri.rs` + `src/api.rs`
- `App`：持有屏幕状态（`Main` / `OAuth`）的 `RwSignal`；启动直接进 `Main`
- `tauri.rs`：绑定 `window.__TAURI__.core.invoke`，无 Tauri Rust 依赖
- `api.rs`：镜像命令签名 + 本地 DTO，反序列化命令结果

#### 4.3 主界面（含顶栏登录入口）
**Implementation file**: `deploykeys-ui/src/app.rs`（`Main` 组件）
- 启动直接进入主界面，无 Welcome 页、无登录门禁
- 顶栏右上角按登录态切换：未登录显示 "Sign in with GitHub" 按钮，
  点击 → `api::start_github_auth()` 进入 OAuth 屏并启动轮询；已登录显示账号 + 退出
- 主题默认跟随系统（`theme::provide_theme(Theme::System)`），详见 docs/THEME_DESIGN.md

#### 4.4 OAuth 设备流界面
**Implementation file**: `deploykeys-ui/src/screens/oauth.rs`
- 展示 verification URL 与 user code，附「打开浏览器 / 复制」操作
- 前端按 interval 轮询 `poll_github_auth`，授权成功跳转主界面

#### 4.5 主界面（占位，Phase 4 续）
**Implementation file**: `deploykeys-ui/src/app.rs`（`Placeholder` 组件）
- 顶部导航：Repos / Targets / Keys / Forge + 已登录身份 + 退出
- Repos/Targets/Keys/Forge 的实际内容（仓库列表、Key Forge 表单等）
  待接入对应 IPC 命令后实现

**UI 设计风格**: Tailwind v4 + Preline 风格 utility 类
- 卡片式布局、顶部导航、状态指示圆点（绿/黄/红）
- 响应式 i18n（en/zh），文案走 `i18n.rs` 词条表

---

### Phase 5: 验证与撤销 (Week 6 周)

#### 5.1 验证引擎
**Implementation file**: `deploykeys-core/src/verification/verifier.rs`
- `KeyBindingVerifier::verify()`: 返回 VerificationReport
  - 检查 GitHub Deploy Key 是否存在
  - 检查权限 (read_only) 是否匹配
  - 检查本地私钥文件是否存在
  - 执行 git ls-remote 测试访问

**测试命令**：
```bash
GIT_SSH_COMMAND="ssh -i <key_path> -o IdentitiesOnly=yes" \
  git ls-remote git@github.com:owner/repo.git
```

#### 5.2 Drift Detection
**Implementation file**: `deploykeys-core/src/services/sync_service.rs`
- `SyncService::detect_drift()`: 批量验证所有 KeyBindings
- 更新状态：active / drifted / orphaned_local / orphaned_remote

#### 5.3 Revoke 流程
**Implementation file**: `deploykeys-core/src/services/revoke_service.rs`
- `RevokeService::revoke_binding()`:
  1. 删除 GitHub Deploy Key (调用 API)
  2. 可选：删除本地私钥文件
  3. 更新 KeyBinding status 为 revoked

#### 5.4 KeyBinding Detail 界面
**Implementation file**: `deploykeys-ui/src/screens/binding_detail.rs`（前端）+ 对应 IPC 命令（`deploykeys-app/src/lib.rs`）
- 显示 KeyBinding 完整信息
- 状态指示器：Active (绿) / Drifted (黄) / Failed (红)
- 按钮：Verify, Revoke, View Public Key

---

### Phase 6: Remote Target 支持 (Week 7-8 周)

#### 6.1 SSH Executor 抽象
**Implementation file**: `deploykeys-core/src/ssh/executor.rs`
- `trait SshExecutor`: 定义 connect(), exec(), read_file(), disconnect()
- `CommandOutput`: 包含 stdout, stderr, exit_code

#### 6.2 Russh 实现
**Implementation file**: `deploykeys-core/src/ssh/russh_executor.rs`
- `RusshExecutor`: 实现 SshExecutor trait
- 支持 password 和 private_key 认证
- Host Key 验证：首次连接展示 fingerprint，后续匹配已保存的

#### 6.3 远程 Key 生成
**Implementation file**: `deploykeys-core/src/keygen/remote.rs`
- `RemoteKeyGenerator::generate()`: 支持多种算法
  1. SSH 连接到远程服务器
  2. 执行 `mkdir -p` 创建目录
  3. 根据算法选择执行：
     - Ed25519: `ssh-keygen -t ed25519 -N "" -f <path>`
     - RSA: `ssh-keygen -t rsa -b 4096 -N "" -f <path>`
     - ECDSA: `ssh-keygen -t ecdsa -b 256 -N "" -f <path>`
  4. 设置权限 `chmod 600 <private>; chmod 644 <public>`
  5. **只读取 .pub 公钥内容**
  6. 返回 `RemoteKeyPair { algorithm, public_key, private_key_path }`

#### 6.4 Target Manager 界面
**Implementation file**: `deploykeys-ui/src/screens/target_manager.rs`（前端界面）+ `deploykeys-app` 侧对应 IPC 命令
- 列表显示所有 targets (Local + Remote)
- "Add Remote Target" 按钮 → 弹出表单：
  - Alias (用户友好名称)
  - Host
  - Port (默认 22)
  - Username
  - Auth method: Password / SSH Key
  - Key base dir (默认 `~/.ssh/deploykeys/github.com/`)
- "Test Connection" 按钮 → 显示连通性检查列表：
  - TCP reachable ✓
  - SSH auth success ✓
  - Host key verified ✓
  - ssh-keygen available ✓
  - Directory writable ✓

---

### Phase 7: Read-write 权限与安全强化 (Week 9 周)

#### 7.1 Read-write 警告流程
- Key Forge 界面：Permission toggle 切换到 Read-write 时显示醒目警告
- 弹出二次确认对话框：
  ```
  ⚠️ Read-write Deploy Key can push code to this repository.
  Only continue if you fully trust this target environment.
  ```
- 用户必须勾选 "I understand" 才能继续

#### 7.2 日志脱敏
**Implementation file**: `deploykeys-core/src/utils/sanitizer.rs`
- `sanitize_log()`: 替换敏感信息
  - `ghu_****`, `ghr_****`, `github_pat_****`
  - `Authorization: Bearer ****`
  - `password=****`
- 在所有日志输出点应用

#### 7.3 零全局污染验证
- 确认不修改 `~/.ssh/config`
- 确认不修改 `~/.gitconfig`
- Key 生成仅在 `~/.ssh/deploykeys/` 子目录

---

## 关键文件清单

### 核心库 (deploykeys-core)

- `src/models/{account,target,repository,key_binding}.rs` - 数据模型
- `src/db/mod.rs` + `src/db/*_repository.rs` - 数据访问层
- `src/credentials/mod.rs` - 凭据管理
- `src/github/{client,deploy_keys,oauth}.rs` - GitHub API（client / Deploy Keys / 设备流）
- `src/ssh/{mod,executor}.rs` - SSH 抽象（Phase 6 接 russh 实现）
- `src/keygen/{mod,local}.rs` - Key 生成（remote 属 Phase 6）
- `src/services/{auth_service,key_binding_service,target_service}.rs` - 业务服务
- `src/verification/mod.rs` - 验证 / 漂移语义
- `migrations/{001_initial,002_settings}.sql` - 数据库 schema

### 原生宿主 (deploykeys-app，Tauri)

- `src/main.rs` - 入口（调 `deploykeys_lib::run()`）
- `src/lib.rs` - IPC 命令面 + DTO + AppState + 事件循环
- `tauri.conf.json` - Tauri 配置（窗口、CSP、`withGlobalTauri`、bundle）
- `capabilities/default.json` - 主窗口权限能力

### 前端 (deploykeys-ui，Leptos/wasm)

- `src/main.rs` - wasm 入口（`mount_to_body`）
- `src/app.rs` - 根组件、屏幕状态机、设备流轮询循环
- `src/api.rs` - IPC 命令的前端镜像 DTO + 调用封装
- `src/tauri.rs` - `window.__TAURI__.core.invoke` 桥
- `src/i18n.rs` - 内联词条表 + 响应式 locale 信号
- `src/theme.rs` - 全局主题信号（亮/暗/系统）+ `<html>` `.dark` 同步
- `src/screens/oauth.rs` - 设备流界面（从主界面顶栏进入）
- `styles/{input,output}.css` + `Trunk.toml` - Tailwind 语义色板与构建配置

---

## 验证方案

### 端到端测试流程

1. **GitHub Auth**:
   - 启动应用 → 直接进入主界面（无需登录；主题跟随系统）
   - 点击顶栏 "Sign in with GitHub" → 获得 device code
   - 浏览器授权 → 轮询成功 → 返回主界面，顶栏显示账号

2. **本地 Key 生成**:
   - 选择仓库 → Key Forge 界面
   - Target: Local Machine, Permission: Read-only
   - 点击 Generate & Bind
   - 验证：`~/.ssh/deploykeys/github.com/owner/repo/id_ed25519` 存在
   - GitHub 仓库 Settings → Deploy keys 页面出现新 key
   - 执行验证 → git ls-remote 成功

3. **远程 Target**:
   - Target Manager → Add Remote Target
   - 填写服务器信息 → Test Connection → 所有检查通过
   - 返回 Key Forge → 选择 remote target → Generate & Bind
   - 验证：SSH 登录服务器，检查 `~/.ssh/deploykeys/` 目录有私钥
   - 桌面端日志不包含远程私钥内容

4. **Drift Detection**:
   - 手动删除 GitHub Deploy Key → 运行 Verify → 状态变为 drifted
   - 手动删除本地私钥 → Verify → 状态变为 orphaned_remote

5. **Revoke**:
   - Binding Detail 界面 → Revoke 按钮 → 选择是否删除私钥
   - GitHub Deploy Key 消失 → 本地状态变为 revoked

### 单元测试重点

- `LocalKeyGenerator`: 验证生成的 key 格式和权限
- `DeviceFlowAuth`: mock HTTP 响应测试轮询逻辑
- `DeployKeyApi`: mock GitHub API 测试 CRUD
- `KeyBindingVerifier`: mock 各种状态组合测试 drift 检测

---

## 风险与缓解

### 风险 1: Russh 复杂度高
**缓解**: 先实现 CommandExecutor (调用系统 ssh 命令) 作为 fallback，保证 MVP 可用。Russh 作为后续优化。

### 风险 2: GitHub App 注册和 Device Flow 测试困难
**缓解**: 提前注册测试用 GitHub App，使用 ngrok 或本地 callback 测试。准备 PAT 作为开发模式 fallback。

### 风险 3: 跨平台 keyring 兼容性问题
**缓解**: 优先支持 macOS Keychain 和 Linux Secret Service。本期不支持 Windows。提供降级方案（加密本地文件）用于测试环境。

### 风险 4: 前端（Leptos/wasm + Tauri IPC）学习曲线
**缓解**: 先实现最简界面（纯文本 + 按钮），UI 美化作为后续迭代。前端经 IPC 命令拿脱敏 DTO，业务逻辑全在核心库，前端尽量薄。

### 风险 5: 远程 Key 生成失败回滚
**缓解**: 实现事务式操作：先生成 key，验证成功后再创建 GitHub Deploy Key。失败时清理远程临时文件。

---

## 交付物

### MVP (v0.1) 包含：
- ✅ GitHub App Device Flow 登录
- ✅ 仓库列表与选择
- ✅ 本地 Target 自动创建
- ✅ 远程 Target 添加与连接测试
- ✅ Read-only Deploy Key 创建（本地 + 远程）
- ✅ Read-write Deploy Key 创建（含警告）
- ✅ KeyBinding 验证（git ls-remote 测试）
- ✅ Revoke 流程
- ✅ Drift Detection
- ✅ 基础前端界面（Leptos/wasm，5 个核心界面）

---

## 后续迭代方向

### v0.2:
- Key Rotation (生成新 key → 替换旧 key)
- 批量验证和批量撤销
- Host Key 管理界面

### v0.3:
- 多 GitHub 账号支持
- Organization 仓库分组显示
- SAML SSO 提示

### v0.4:
- 导入已有 Deploy Keys（通过 fingerprint 匹配）
- 操作历史和审计日志
- 导出配置报告

### v0.5:
- GitHub Enterprise Server 支持
- GitLab Deploy Keys 适配
