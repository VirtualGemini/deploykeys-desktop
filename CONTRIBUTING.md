# 开发规范 Contributing Guide

## 目录

- [项目结构](#项目结构)
- [代码规范](#代码规范)
- [Git 规范](#git-规范)
- [测试规范](#测试规范)
- [安全规范](#安全规范)
- [禁止事项](#禁止事项)

---

## 项目结构

### 标准目录布局

```
deploykeys-desktop/
├── crates/
│   ├── deploykeys-core/          # 核心业务逻辑（无 GUI 依赖）
│   │   ├── src/
│   │   │   ├── models/        # 数据模型（仅结构体和枚举）
│   │   │   ├── db/            # 数据库访问层（Repository 模式）
│   │   │   ├── github/        # GitHub API 客户端
│   │   │   ├── ssh/           # SSH 操作抽象
│   │   │   ├── keygen/        # SSH Key 生成
│   │   │   ├── credentials/   # 系统凭据管理
│   │   │   ├── services/      # 业务逻辑服务层
│   │   │   ├── verification/  # Key 验证逻辑
│   │   │   ├── utils/         # 工具函数
│   │   │   ├── error.rs       # 错误类型定义
│   │   │   └── lib.rs         # Crate 入口
│   │   ├── tests/             # 集成测试
│   │   └── Cargo.toml
│   │
│   └── deploykeys-gui/           # GUI 应用（依赖 deploykeys-core）
│       ├── src/
│       │   ├── screens/       # 各个界面实现
│       │   ├── components/    # 可复用 UI 组件
│       │   ├── app.rs         # 应用主状态机
│       │   ├── messages.rs    # 消息定义
│       │   └── main.rs        # 程序入口
│       └── Cargo.toml
│
├── migrations/                # SQLite 数据库迁移脚本
├── assets/                    # 资源文件（图标、字体等）
├── docs/                      # 文档
├── .github/                   # GitHub Actions CI/CD
├── target/                    # 构建产物（Git 忽略）
├── Cargo.toml                 # Workspace 配置
├── Cargo.lock                 # 依赖锁文件（提交到 Git）
├── .gitignore
├── README.md
├── CONTRIBUTING.md            # 本文件
├── QUICKSTART.md
└── LICENSE

```

### 文件放置规则

✅ **允许**：
- `crates/deploykeys-core/src/models/` - 数据模型
- `crates/deploykeys-core/src/db/` - 数据库 Repository
- `crates/deploykeys-core/tests/` - 集成测试
- `crates/deploykeys-gui/src/screens/` - UI 界面
- `migrations/*.sql` - 数据库迁移

❌ **禁止**：
- ❌ GUI 代码出现在 `deploykeys-core`
- ❌ 业务逻辑出现在 `deploykeys-gui`（除 UI 状态管理）
- ❌ 测试代码在 `src/` 目录（应在 `tests/`）
- ❌ 临时文件、测试数据库、密钥文件提交到 Git
- ❌ `target/` 目录提交到 Git
- ❌ `.env` 文件提交到 Git（使用 `.env.example`）

---

## 代码规范

### Rust 代码风格

**强制执行**：
```bash
# 代码格式化（提交前必须运行）
cargo fmt --all

# Clippy 检查（无警告才能提交）
cargo clippy --all-targets --all-features -- -D warnings
```

### 命名规范

| 类型 | 规范 | 示例 |
|------|------|------|
| 模块 | `snake_case` | `key_binding`, `github_api` |
| 结构体 | `PascalCase` | `KeyBinding`, `GitHubClient` |
| 枚举 | `PascalCase` | `KeyAlgorithm`, `TargetType` |
| 函数 | `snake_case` | `create_key`, `find_by_id` |
| 常量 | `SCREAMING_SNAKE_CASE` | `MAX_RETRIES`, `DEFAULT_TIMEOUT` |
| 变量 | `snake_case` | `repo_id`, `target_name` |

### 代码组织

**模块顺序**：
```rust
// 1. 外部 crate 导入
use std::path::PathBuf;
use chrono::{DateTime, Utc};

// 2. 同 crate 其他模块导入
use crate::models::KeyBinding;
use crate::error::Result;

// 3. 类型定义
pub struct Repository {
    pool: SqlitePool,
}

// 4. 实现块
impl Repository {
    pub fn new(pool: SqlitePool) -> Self { /* */ }
    pub async fn create(&self, item: &Item) -> Result<i64> { /* */ }
}

// 5. 测试模块（文件末尾）
#[cfg(test)]
mod tests {
    use super::*;
    // 测试代码
}
```

### 错误处理

✅ **正确**：
```rust
// 使用 Result 和 ? 操作符
pub async fn create_key(&self, target: &Target) -> Result<KeyPair> {
    let key = generate_key()?;
    self.db.insert(&key).await?;
    Ok(key)
}
```

❌ **禁止**：
```rust
// 不要使用 unwrap() 或 expect()（除非在测试中）
let key = generate_key().unwrap(); // ❌

// 不要吞掉错误
if let Err(_) = do_something() {
    return Ok(()); // ❌ 应该传播错误
}
```

### 异步代码

✅ **正确**：
```rust
// 使用 async/await
pub async fn fetch_repos(&self) -> Result<Vec<Repository>> {
    let response = self.client.get("/repos").await?;
    Ok(response.json().await?)
}
```

❌ **禁止**：
```rust
// 不要在异步函数中使用阻塞操作
pub async fn read_file(&self, path: &str) -> Result<String> {
    std::fs::read_to_string(path) // ❌ 应该用 tokio::fs
}
```

---

## Git 规范

### 分支策略

- `main` - 生产分支（受保护，只能通过 PR 合并）
- `develop` - 开发主分支
- `feature/xxx` - 功能分支
- `fix/xxx` - Bug 修复分支
- `refactor/xxx` - 重构分支

### Commit Message 格式

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Type**：
- `feat`: 新功能
- `fix`: Bug 修复
- `refactor`: 重构（不改变功能）
- `style`: 代码格式（不影响逻辑）
- `docs`: 文档变更
- `test`: 测试相关
- `chore`: 构建、依赖等

**示例**：
```
feat(github): add Device Flow authentication

- Implement OAuth Device Flow
- Add token refresh logic
- Store tokens in system keyring

Closes #123
```

### 提交前检查清单

- [ ] 运行 `cargo fmt --all`
- [ ] 运行 `cargo clippy -- -D warnings`
- [ ] 运行 `cargo test --all`
- [ ] 更新相关文档
- [ ] 移除调试代码（`println!`, `dbg!`）
- [ ] 确保没有 `TODO` 或 `FIXME` 注释（除非有 Issue 跟踪）

---

## 测试规范

### 测试文件组织

```
crates/deploykeys-core/
├── src/
│   └── db/
│       └── account_repository.rs
└── tests/
    └── db/
        └── account_repository_test.rs  # 集成测试
```

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_algorithm() {
        let alg = KeyAlgorithm::from_str("ed25519").unwrap();
        assert_eq!(alg, KeyAlgorithm::Ed25519);
    }
}
```

### 集成测试

```rust
#[tokio::test]
async fn test_create_account() {
    let db = setup_test_db().await;
    let repo = AccountRepository::new(db.pool());
    
    let account = Account { /* ... */ };
    let id = repo.create(&account).await.unwrap();
    
    assert!(id > 0);
    cleanup_test_db(db).await;
}
```

---

## 安全规范

### 敏感信息处理

✅ **正确**：
```rust
// 使用系统凭据管理器
credential_store.set("github_token", &token)?;

// 日志脱敏
let sanitized = sanitize_log(&log_message);
tracing::info!("{}", sanitized);
```

❌ **禁止**：
```rust
// 不要在日志中输出敏感信息
tracing::info!("Token: {}", token); // ❌

// 不要硬编码密钥
const API_KEY: &str = "ghp_xxxx"; // ❌

// 不要将敏感信息存储在 SQLite
db.execute("INSERT INTO tokens VALUES (?)", token); // ❌
```

### 输入验证

✅ **必须验证**：
- 用户输入的文件路径（防止路径遍历）
- 网络请求的响应（防止注入）
- SSH 主机指纹（防止中间人攻击）

---

## 依赖管理

### 添加依赖

1. 优先使用 Workspace 共享依赖（在根 `Cargo.toml`）
2. 使用精确或补丁版本（避免 `*` 或 `>=`）
3. 检查依赖的安全性：`cargo audit`

### 禁止的依赖

❌ 不允许使用：
- 未维护的 crate（最后更新 > 2 年）
- 有已知安全漏洞的 crate
- License 不兼容的 crate（GPL 等）

---

## 禁止事项

### 绝对禁止

1. **提交敏感信息**
   - ❌ API Key, Token, Password
   - ❌ SSH 私钥
   - ❌ `.env` 文件
   - ❌ 测试数据库文件

2. **不安全的代码**
   - ❌ `unsafe` 块（除非绝对必要且有充分注释）
   - ❌ SQL 拼接（使用参数化查询）
   - ❌ 命令注入风险的代码

3. **破坏性操作**
   - ❌ 修改 `~/.ssh/config`
   - ❌ 修改 `~/.gitconfig`
   - ❌ 删除用户文件（除非明确授权）

4. **性能问题**
   - ❌ 在循环中进行数据库查询（使用批量操作）
   - ❌ 阻塞主线程（使用异步）
   - ❌ 内存泄漏（检查 Drop 实现）

---

## 开发工具

### 推荐工具

- **IDE**: VSCode + rust-analyzer
- **调试**: `rust-lldb` / `rust-gdb`
- **性能分析**: `cargo flamegraph`
- **内存检查**: `valgrind`
- **依赖审计**: `cargo audit`

### VSCode 配置

```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

---

## 发布检查清单

发布前必须确认：

- [ ] 所有测试通过
- [ ] 文档已更新
- [ ] CHANGELOG.md 已更新
- [ ] 版本号已更新（遵循 Semantic Versioning）
- [ ] 无安全漏洞（`cargo audit`）
- [ ] 代码覆盖率 > 80%
- [ ] 性能测试通过

---

## 联系方式

- Issue: https://github.com/yourorg/deploykeys-desktop/issues
- Discussions: https://github.com/yourorg/deploykeys-desktop/discussions

---

**遵守这些规范，保持代码整洁和项目健康！** 🚀
