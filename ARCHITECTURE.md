# 架构设计文档

## 系统架构

### 整体架构

应用是一个 Tauri 2 桌面程序：原生宿主 crate（`deploykeys-gui`）持有所有
业务能力，前端是独立的 Leptos CSR/wasm crate（`deploykeys-ui`），跑在
webview 里。两者只通过 Tauri 的 IPC 命令桥通信，且只传脱敏 DTO——
keyring 引用、token 等机密永不跨越 IPC 边界。

```
┌──────────────────────────────────────────────────────────┐
│              deploykeys-ui (前端 / Leptos CSR wasm)          │
│  ┌──────────────┐              ┌──────────────┐          │
│  │     Main     │              │    OAuth      │          │
│  │  (主界面/占位) │◀────顶栏登录──▶│ (Device Flow) │          │
│  └──────────────┘              └──────────────┘          │
│   全局主题（theme.rs，.dark on <html>）+ i18n（locale）    │
│         跑在 webview 中，无 deploykeys-core 依赖            │
└──────────────────────────┬───────────────────────────────┘
                           │  Tauri IPC（window.__TAURI__.core.invoke）
                           │  仅传脱敏 DTO，机密不过界
┌──────────────────────────┼───────────────────────────────┐
│           deploykeys-gui (Tauri 原生宿主)                    │
│  IPC 命令面：get_session / get_language / set_language /   │
│  start_github_auth / poll_github_auth / open_url           │
│  打开数据库、注入 AppState、桥接到 core                     │
└──────────────────────────┼───────────────────────────────┘
                           │
┌──────────────────────────┼───────────────────────────────┐
│                   deploykeys-core (核心层)                   │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐   │
│  │  Services   │  │   GitHub     │  │     SSH      │   │
│  │    Layer    │  │   Client     │  │   Executor   │   │
│  └─────────────┘  └──────────────┘  └──────────────┘   │
│         │                │                   │           │
│         └────────────────┴───────────────────┘           │
│                          │                               │
│  ┌────────────────────────────────────────────────────┐ │
│  │            Database Layer (Repository)             │ │
│  └────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
                           │
                           ▼
         ┌─────────────────────────────────────┐
         │  External Systems                    │
         │  ├─ SQLite Database                 │
         │  ├─ System Keyring (macOS/Linux)    │
         │  ├─ GitHub API                       │
         │  └─ Remote SSH Servers               │
         └─────────────────────────────────────┘
```

---

## 分层设计

### 1. 前端层 (`deploykeys-ui`)

纯 CSR（client-side rendered）wasm，用 Leptos 0.6 写，由 Trunk 构建。
样式走 Tailwind v4（tools/ 下固定的 standalone 二进制，无 Node 依赖），
组件风格抄自 Preline 的 utility 类。颜色一律走全局语义令牌（`bg-bg`、
`text-content`、`bg-primary` 等），不写死 `slate-*`/`blue-*`；明暗主题切换
由根组件统一管理，详见 [docs/THEME_DESIGN.md](docs/THEME_DESIGN.md)。

**职责**：
- 用户交互与界面渲染（Leptos `view!` + 响应式信号）
- 用户输入验证（UI 层面）
- UI 状态管理（`RwSignal`，如当前屏幕、登录中标志、错误信息）
- 全局主题状态：根组件 `provide_theme` 一个响应式 `Theme` 信号，默认跟随
  系统（`prefers-color-scheme`），与 i18n 的 locale 信号同构
- 经 Tauri IPC 调用原生命令并解析回传的 DTO

**依赖**：
- `leptos`（CSR 框架）
- `wasm-bindgen` / `serde-wasm-bindgen` / `js-sys` / `web-sys`（绑定与 IPC）
- **不依赖** `deploykeys-core`：core 会拉入 tokio/sqlx/keyring 等 native-only
  依赖，无法编进 wasm。UI 只镜像它需要的字段为本地 DTO。

**禁止**：
- ❌ 直接操作数据库或调用 GitHub API
- ❌ 实现业务逻辑
- ❌ 让机密（token、keyring 引用）出现在前端

**示例**：
```rust
// ✅ 正确：经 IPC 命令拿脱敏 DTO
let account = api::get_session().await?;   // -> Option<Account { login, avatar_url }>

// ❌ 错误：前端无法、也不应直接碰 core 或数据库
let repos = db.query("SELECT * FROM repositories")?;
```

---

### 2. 原生宿主层 (`deploykeys-gui`)

Tauri 2 宿主。产出名为 `deploykeys` 的二进制。负责打开数据库、把
`AppState` 注入每个命令、注册 IPC 命令面、运行 Tauri 事件循环。

**职责**：
- 启动时打开（或创建）数据库并跑迁移，stash 进 Tauri managed state
- 暴露 IPC 命令，桥接到 `deploykeys-core`
- 定义跨 IPC 边界的 DTO，确保机密不外泄

**IPC 命令面**：
| 命令 | 作用 |
|---|---|
| `get_session` | 返回持久化的会话账号（脱敏 DTO） |
| `get_language` / `set_language` | 读写持久化语言偏好 |
| `start_github_auth` | 发起设备流，返回 device/user code |
| `poll_github_auth` | 轮询一次 token 端点；授权成功则完成登录 |
| `open_url` | 用系统默认浏览器打开 URL |

**依赖**：`deploykeys-core`、`tauri`、`tokio`、`dirs`、`open` 等。

---

### 3. 核心层 (`deploykeys-core`)

#### 3.1 服务层 (`services/`)

**职责**：
- 业务逻辑编排
- 事务管理
- 跨模块协调

**示例**：
```rust
pub struct KeyBindingService {
    db: Database,
    github: GitHubClient,
    keygen: KeyGenerator,
}

impl KeyBindingService {
    pub async fn create_and_bind_key(
        &self,
        repo_id: i64,
        target_id: i64,
    ) -> Result<KeyBinding> {
        // 1. 生成 Key
        let key_pair = self.keygen.generate(KeyAlgorithm::Ed25519)?;
        
        // 2. 上传到 GitHub
        let deploy_key = self.github.create_deploy_key(repo_id, &key_pair.public).await?;
        
        // 3. 保存到数据库
        let binding = KeyBinding { /* ... */ };
        self.db.key_bindings().create(&binding).await?;
        
        Ok(binding)
    }
}
```

#### 3.2 数据访问层 (`db/`)

**职责**：
- CRUD 操作
- SQL 查询
- 数据映射

**规范**：
- 使用 Repository 模式
- 一个 Repository 对应一个表
- 所有查询使用 `sqlx::query!` 宏（编译时检查）

**示例**：
```rust
pub struct AccountRepository {
    pool: SqlitePool,
}

impl AccountRepository {
    pub async fn create(&self, account: &Account) -> Result<i64> { /* */ }
    pub async fn find_by_id(&self, id: i64) -> Result<Option<Account>> { /* */ }
    pub async fn list_all(&self) -> Result<Vec<Account>> { /* */ }
    pub async fn update(&self, account: &Account) -> Result<()> { /* */ }
    pub async fn delete(&self, id: i64) -> Result<()> { /* */ }
}
```

#### 3.3 外部集成层

**GitHub API** (`github/`):
- `oauth.rs`：Device Flow 认证（设备码请求 + token 轮询）
- `deploy_keys.rs`：Deploy Keys CRUD
- `client.rs`：通用 HTTP 客户端
- Installations / 仓库同步尚未实现（PLAN Phase 2.3）

**SSH** (`ssh/`):
- `executor.rs`：仅定义 `SshExecutor` trait（远程命令执行、文件读取等）
- 具体实现（russh）属 Phase 6，尚未落地

**Key 生成** (`keygen/`):
- `local.rs`：本地生成 SSH Key，支持 Ed25519 / RSA 2048·4096 / ECDSA P-256·384·521
- 远程生成（`remote.rs`）属 Phase 6，尚未落地

**凭据管理** (`credentials/`):
- 系统 Keyring 集成
- Token 存储/读取

---

## 数据流

### GitHub 设备流登录（现已落地的代表性数据流）

```
用户点击 "Sign in with GitHub"（前端主界面顶栏，未登录时显示）
    │
    ▼
前端 spawn_local → api::start_github_auth()
    │  经 Tauri IPC (invoke "start_github_auth")
    ▼
原生命令 start_github_auth → DeviceFlowClient::request_device_code()
    │  返回 DeviceCodeDto（device_code/user_code/verification_uri/interval）
    ▼
前端切到 OAuth 屏，展示 user_code，并按 interval 启动轮询循环
    │
    ▼
api::poll_github_auth(device_code)  ── 每隔 interval 秒 ──┐
    │  IPC invoke "poll_github_auth"                       │
    ▼                                                      │
原生命令 poll_github_auth → DeviceFlowClient::poll_for_token()
    │                                                      │
    ├─ Pending  → 前端再排一次轮询 ────────────────────────┘
    ├─ SlowDown → interval += 5s 后再轮询 ──────────────────┘
    └─ Authorized(tokens)
         │
         ▼
       AuthService::complete_device_flow(tokens)
         ├─► keyring 存 access/refresh token
         └─► accounts 表 upsert 账号
         │
         ▼
       返回 AccountDto → 前端切到主界面（Placeholder）
```

> 注：Key Binding 创建流程（KeyBindingService）核心库已实现，但前端对应
> 界面属 Phase 4，尚未接入 IPC；上面的设备流是当前端到端跑通的代表。

---

## 错误处理

### 错误类型层次

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("GitHub API error: {0}")]
    GitHub(String),
    
    #[error("SSH error: {0}")]
    Ssh(String),
    
    #[error("Key generation error: {0}")]
    KeyGen(String),
    
    #[error("Credential storage error: {0}")]
    Credential(String),
    
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

### 错误传播规则

1. **底层错误 → 核心错误**：使用 `#[from]` 自动转换
2. **核心错误 → GUI**：转换为用户友好的消息
3. **不要吞掉错误**：使用 `?` 传播或显式处理

---

## 并发模型

### 异步运行时

- **原生侧运行时**: Tokio（Tauri 托管；IPC 命令为 `async fn`，在其上执行）
- **数据库连接池**: SQLx 管理
- **前端**: wasm 单线程，异步任务经 `wasm_bindgen_futures::spawn_local` 调度，
  通过 IPC 调用原生命令；轮询节流用 `setTimeout`（见 `ui/src/app.rs`）

### 并发策略

```rust
// ✅ 正确：并发执行独立任务
let (repos, targets) = tokio::join!(
    repo_service.list_all(),
    target_service.list_all(),
);

// ❌ 错误：串行执行
let repos = repo_service.list_all().await?;
let targets = target_service.list_all().await?;
```

---

## 安全设计

### 1. 凭据隔离

```
┌─────────────────────────────────────┐
│  Application Memory                 │
│  ├─ SQLite: 存储 token_ref          │
│  └─ 不存储实际 token                 │
└─────────────────────────────────────┘
            │
            ▼
┌─────────────────────────────────────┐
│  System Keyring (macOS/Linux)       │
│  ├─ macOS: Keychain                 │
│  └─ Linux: Secret Service           │
└─────────────────────────────────────┘
```

### 2. 私钥隔离

- **本地模式**：私钥存储在本地 `~/.ssh/deploykeys/`
- **远程模式**：私钥留在远程服务器，永不读取

### 3. 日志脱敏

```rust
// 自动过滤敏感信息
let log = sanitize_log(&message);
// "Token: ghp_abc123" → "Token: ****"
```

---

## 性能优化

### 1. 数据库

- 使用连接池（最大 10 连接）
- 批量操作代替循环查询
- 添加适当的索引

### 2. 网络请求

- 复用 HTTP 连接（reqwest）
- 实现请求重试（指数退避）
- 使用流式处理大响应

### 3. UI 渲染（Leptos webview）

- 视图是细粒度响应式：信号（`RwSignal`）变化只更新依赖它的 DOM 节点，无整树重绘
- 重计算放进 `Memo`，不要写在视图闭包里反复求值
- 长列表用 `<For>` 配合稳定 key，避免整列表重建
- IPC 调用在 `spawn_local` 里异步进行，不阻塞渲染

---

## 扩展点

### 支持新的 SSH Key 算法

```rust
// 1. 在 models/key_binding.rs 添加枚举
pub enum KeyAlgorithm {
    Ed25519,
    Rsa2048,
    // 新增：
    Rsa4096,
}

// 2. 在 keygen/local.rs 实现生成逻辑
impl LocalKeyGenerator {
    fn generate_rsa4096(&self) -> Result<KeyPair> { /* */ }
}
```

### 支持新的 Git 平台

```rust
// 1. 定义新的 trait
pub trait GitPlatformClient {
    async fn create_deploy_key(&self, repo: &str, key: &str) -> Result<DeployKey>;
    async fn list_repositories(&self) -> Result<Vec<Repository>>;
}

// 2. 实现 GitHub
impl GitPlatformClient for GitHubClient { /* */ }

// 3. 实现 GitLab
impl GitPlatformClient for GitLabClient { /* */ }
```

---

## 测试策略

### 单元测试

- 覆盖所有 `models/` 中的转换逻辑
- 覆盖 `utils/` 中的工具函数

### 集成测试

- 数据库操作（使用内存 SQLite）
- 服务层业务逻辑

### E2E 测试

- GUI 交互流程
- 完整的 Key 创建流程

---

## 部署架构

### 应用分发

```
deploykeys-desktop/
├── macOS: .app 或 .dmg
├── Linux: .deb / .rpm / .AppImage
└── Windows: .exe / .msi (未来支持)
```

### 数据存储位置

运行时数据目录由 `dirs::data_dir()/deploykeys/` 决定：

- **macOS**: `~/Library/Application Support/deploykeys/`
- **Linux**: `~/.local/share/deploykeys/`

（旧版本曾用 `deplock/`；首次启动会自动迁移，连带 `-wal`/`-shm` 一起搬，避免孤立 WAL 损坏数据库。）

### 应用数据

- `deploykeys.db` - 运行时 SQLite 数据库（位于上面的数据目录）
- `app_settings` 表 - 语言偏好等键值设置（迁移 `002_settings.sql`）

仓库根目录的 `deploykeys.db` 与运行时无关，仅供 sqlx 编译期查询校验（`make db-setup` 生成）。

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Token 泄露 | 高 | 使用系统 Keyring，日志脱敏 |
| 私钥泄露 | 高 | 远程模式不读取私钥 |
| API 限流 | 中 | 实现重试和退避 |
| 数据库损坏 | 中 | 定期备份，使用 WAL 模式 |
| 网络中断 | 低 | 缓存数据，离线可用 |

---

## 未来规划

- [ ] GitHub Enterprise 支持
- [ ] GitLab 支持
- [ ] Bitbucket 支持
- [ ] Windows 支持
- [ ] 多语言支持（i18n）
- [ ] 自动 Key 轮换
- [ ] Key 使用审计日志

---

**保持架构清晰，确保系统可维护！** 🏗️
