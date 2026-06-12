# 架构设计文档

## 系统架构

### 整体架构

```
┌─────────────────────────────────────────────────────────┐
│                    deploykeys-gui (GUI层)                   │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐   │
│  │   Welcome   │  │ Repositories │  │   Targets    │   │
│  │   Screen    │  │    Screen    │  │    Screen    │   │
│  └─────────────┘  └──────────────┘  └──────────────┘   │
│         │                │                   │           │
│         └────────────────┴───────────────────┘           │
│                          │                               │
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

### 1. GUI 层 (`deploykeys-gui`)

**职责**：
- 用户交互
- 界面渲染
- 用户输入验证
- 状态管理（仅 UI 状态）

**依赖**：
- `deploykeys-core`（业务逻辑）
- `iced`（GUI 框架）
- `iced_aw`（UI 组件库）

**禁止**：
- ❌ 直接操作数据库
- ❌ 直接调用 GitHub API
- ❌ 实现业务逻辑

**示例**：
```rust
// ✅ 正确：通过 service 层
let repos = service.list_repositories().await?;

// ❌ 错误：直接操作数据库
let repos = db.query("SELECT * FROM repositories")?;
```

---

### 2. 核心层 (`deploykeys-core`)

#### 2.1 服务层 (`services/`)

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

#### 2.2 数据访问层 (`db/`)

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

#### 2.3 外部集成层

**GitHub API** (`github/`):
- Device Flow 认证
- Deploy Keys 管理
- Installations API

**SSH** (`ssh/`):
- 远程命令执行
- 文件传输
- Host Key 验证

**Key 生成** (`keygen/`):
- 本地生成 SSH Key
- 支持多种算法

**凭据管理** (`credentials/`):
- 系统 Keyring 集成
- Token 存储/读取

---

## 数据流

### 创建 Key Binding 流程

```
User Action (GUI)
    │
    ▼
[Button Click: Create Binding]
    │
    ▼
Message::CreateKeyBinding(repo_id, target_id)
    │
    ▼
App::update() - 调用 service
    │
    ▼
KeyBindingService::create_and_bind_key()
    │
    ├─► KeyGenerator::generate() → 生成 SSH Key
    │
    ├─► GitHubClient::create_deploy_key() → 上传公钥
    │
    └─► KeyBindingRepository::create() → 保存到数据库
        │
        ▼
    Success/Error 返回到 GUI
    │
    ▼
[Update UI State]
```

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

- **主运行时**: Tokio
- **GUI 事件循环**: Iced 内置
- **数据库连接池**: SQLx 管理

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

### 3. GUI 渲染

- 避免在 `view()` 中做重计算
- 使用状态缓存
- 惰性加载长列表

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

- **macOS**: `~/Library/Application Support/com.deploykeys.desktop/`
- **Linux**: `~/.local/share/deploykeys/`

### 配置文件

- `.claude/settings.json` - 用户配置
- `deploykeys.db` - 应用数据库

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
