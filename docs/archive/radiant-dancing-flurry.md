Deplock Desktop 实现计划

     Context

     Deplock Desktop 是一个从零开始的 GitHub Deploy Key 自动化配置工具。用户通过桌面 GUI 在指定目标环境（本地或远程服务器）生成 SSH Key，并自动绑定公钥到
      GitHub 仓库，私钥永远留在目标环境。

     核心价值：让开发者通过 GUI 安全地授权目标环境访问 GitHub 私有仓库，无需手动配置 SSH、复制公钥或创建 Deploy Key。

     MVP 范围: 仅支持 macOS 和 Linux，不支持 Windows。界面设计参考 Docker Desktop 的简洁风格（深色主题、卡片式布局、左侧导航）。

     项目架构

     Cargo Workspace 结构

     deplock-desktop/
     ├── Cargo.toml                    # Workspace 根
     ├── crates/
     │   ├── deplock-core/            # 核心业务逻辑库（无 UI 依赖）
     │   │   ├── src/
     │   │   │   ├── models/          # 数据模型 (Account, Target, KeyBinding, Repository)
     │   │   │   ├── db/              # SQLite 数据访问层
     │   │   │   ├── github/          # GitHub API 客户端 (Device Flow, Deploy Keys API)
     │   │   │   ├── ssh/             # SSH 操作抽象 (trait + russh 实现)
     │   │   │   ├── keygen/          # SSH Key 生成 (本地: ssh-key crate, 远程: ssh-keygen)
     │   │   │   ├── credentials/     # 系统凭据管理 (keyring crate)
     │   │   │   ├── verification/    # 验证逻辑 (git ls-remote 测试)
     │   │   │   └── services/        # 业务服务层 (KeyForgeService, VerificationService)
     │   │   └── Cargo.toml
     │   │
     │   └── deplock-gui/             # Iced GUI 应用
     │       ├── src/
     │       │   ├── main.rs
     │       │   ├── app.rs           # Iced Application 主状态机
     │       │   ├── screens/         # 各界面 (Welcome, Auth, RepoList, KeyForge, Detail)
     │       │   └── messages.rs      # Iced Message 枚举
     │       └── Cargo.toml
     ├── migrations/                   # SQLite schema 迁移
     └── assets/                       # 图标、资源

     技术栈

     - GUI: Iced 0.12+ (纯 Rust，支持 macOS/Linux)
     - HTTP: reqwest + rustls
     - SSH: russh (优先) 或 ssh 命令 fallback
     - 数据库: SQLx + SQLite (异步)
     - 凭据: keyring (macOS Keychain/Linux Secret Service)
     - SSH Key: ssh-key crate (支持 Ed25519, RSA, ECDSA 算法选择)

     MVP 实现路线图

     Phase 1: 项目骨架与数据层 (第 1-2 周)

     1.1 初始化 Workspace
     - 创建 Cargo.toml workspace 配置
     - 创建 crates/deplock-core 和 crates/deplock-gui 子项目
     - 配置共享依赖和构建设置
     - 配置目标平台：macOS (x86_64-apple-darwin, aarch64-apple-darwin) 和 Linux (x86_64-unknown-linux-gnu)

     1.2 数据模型设计
     实现文件：deplock-core/src/models/
     - account.rs: Account 结构体 + AuthType 枚举
     - target.rs: Target 结构体 + TargetType/AuthMethod 枚举
     - repository.rs: Repository 结构体
     - key_binding.rs: KeyBinding 结构体 + KeyAlgorithm/DeployKeyPermission/KeyBindingStatus 枚举
       - KeyAlgorithm: Ed25519 / Rsa2048 / Rsa4096 / EcdsaP256 / EcdsaP384 / EcdsaP521

     1.3 SQLite 数据库层
     - migrations/001_initial.sql: 创建 accounts, targets, repositories, key_bindings, github_installations 表
     - deplock-core/src/db/mod.rs: Database 连接池管理
     - deplock-core/src/db/*_repository.rs: 为每个模型实现 CRUD (使用 sqlx 宏)

     1.4 凭据管理
     实现文件：deplock-core/src/credentials/mod.rs
     - CredentialStore::store_token(): 存储 GitHub token 到系统 keyring
     - CredentialStore::get_token(): 读取 token
     - CredentialStore::store_ssh_password(): 存储 SSH 密码

     关键点：SQLite 只保存 token_ref（如 "github_token_user123"），不存明文。

     Phase 2: GitHub 授权流程 (第 3 周)

     2.1 GitHub API 客户端基础
     实现文件：deplock-core/src/github/client.rs
     - GitHubClient::new(): 使用 reqwest + rustls
     - GitHubClient::request(): 通用请求方法，自动添加 Authorization header

     2.2 Device Flow 授权
     实现文件：deplock-core/src/github/auth.rs
     - DeviceFlowAuth::initiate(): POST /login/device/code，返回 device_code 和 user_code
     - DeviceFlowAuth::poll_token(): 轮询 POST /login/oauth/access_token
     - DeviceFlowAuth::refresh_token(): 刷新过期 token

     流程：
     1. 用户点击 "Sign in with GitHub"
     2. 显示 user_code 和二维码/链接
     3. 后台轮询直到用户授权完成
     4. 保存 access_token 和 refresh_token 到 keyring
     5. 调用 /user API 获取用户信息

     2.3 Installations & Repositories API
     实现文件：deplock-core/src/github/installations.rs
     - list_installations(): GET /user/installations
     - list_installation_repos(): GET /user/installations/{id}/repositories
     - 同步到本地 SQLite

     Phase 3: 本地 Target + Key 生成 (第 4 周)

     3.1 本地 Target 初始化
     实现文件：deplock-core/src/services/target_service.rs
     - TargetService::create_local_target(): 创建默认 "Local Machine"
     - 检测 OS 类型 (macOS/Windows/Linux)
     - 设置 key_base_dir 为 ~/.ssh/deplock/github.com/

     3.2 本地 Key 生成
     实现文件：deplock-core/src/keygen/local.rs
     - LocalKeyGenerator::generate(): 支持多种算法选择
       - Ed25519 (默认，推荐)
       - RSA 2048/4096
       - ECDSA P-256/P-384/P-521
     - 写入 ~/.ssh/deplock/github.com/<owner>/<repo>/id_<algorithm>
     - 设置文件权限 (Unix: 600 for private, 644 for public)
     - 返回 KeyPair { algorithm, public_key, fingerprint, private_key_path }

     3.3 Deploy Keys API
     实现文件：deplock-core/src/github/deploy_keys.rs
     - DeployKeyApi::create(): POST /repos/{owner}/{repo}/keys
     - DeployKeyApi::list(): GET /repos/{owner}/{repo}/keys
     - DeployKeyApi::delete(): DELETE /repos/{owner}/{repo}/keys/{key_id}

     3.4 Key Forge 服务
     实现文件：deplock-core/src/services/key_forge.rs
     - KeyForgeService::create_binding(): 完整流程
       a. 检查 repo × target 唯一性
       b. 生成本地 key
       c. 调用 GitHub API 创建 Deploy Key
       d. 保存 KeyBinding 到数据库
       e. 返回 KeyBinding 对象

     Phase 4: 基础 GUI (第 5 周)

     4.1 Iced 应用骨架
     实现文件：deplock-gui/src/app.rs
     - DeplockApp 结构体：持有 current_screen 和 AppState
     - AppState：持有 Database, GitHubClient, 当前账号、仓库列表等
     - Message 枚举：定义所有用户交互消息

     4.2 Welcome 界面
     实现文件：deplock-gui/src/screens/welcome.rs
     - 显示 Deplock logo 和介绍
     - "Sign in with GitHub" 按钮 → 触发 Message::StartGitHubAuth

     4.3 GitHub Auth 界面
     实现文件：deplock-gui/src/screens/auth.rs
     - 显示 device code 和授权链接
     - 显示二维码（可选）
     - 轮询状态：Polling → Success → 跳转到 Repository List

     4.4 Repository List 界面
     实现文件：deplock-gui/src/screens/repo_list.rs
     - 列表显示所有可访问仓库
     - 每个仓库显示：owner/name, private 标记, 已绑定 targets
     - 点击仓库 → 跳转到 Key Forge 界面

     4.5 Key Forge 界面
     实现文件：deplock-gui/src/screens/key_forge.rs
     - Repository picker (预选)
     - Target picker: "Local Machine"
     - Algorithm selector: Ed25519 (默认) / RSA 2048 / RSA 4096 / ECDSA P-256
     - Permission toggle: Read-only (默认) / Read-write (显示警告)
     - Key title 输入框
     - "Generate & Bind" 按钮 → 触发 Message::CreateBinding
     - 显示执行计划预览
     - UI 设计风格: 参考 Docker Desktop 的简洁风格
       - 左侧导航栏：图标 + 文字
       - 主内容区：卡片式布局
       - 深色主题优先
       - 状态指示器：绿/黄/红圆点

     Phase 5: 验证与撤销 (第 6 周)

     5.1 验证引擎
     实现文件：deplock-core/src/verification/verifier.rs
     - KeyBindingVerifier::verify(): 返回 VerificationReport
       - 检查 GitHub Deploy Key 是否存在
       - 检查权限 (read_only) 是否匹配
       - 检查本地私钥文件是否存在
       - 执行 git ls-remote 测试访问

     测试命令：
     GIT_SSH_COMMAND="ssh -i <key_path> -o IdentitiesOnly=yes" \
       git ls-remote git@github.com:owner/repo.git

     5.2 Drift Detection
     实现文件：deplock-core/src/services/sync_service.rs
     - SyncService::detect_drift(): 批量验证所有 KeyBindings
     - 更新状态：active / drifted / orphaned_local / orphaned_remote

     5.3 Revoke 流程
     实现文件：deplock-core/src/services/revoke_service.rs
     - RevokeService::revoke_binding():
       a. 删除 GitHub Deploy Key (调用 API)
       b. 可选：删除本地私钥文件
       c. 更新 KeyBinding status 为 revoked

     5.4 KeyBinding Detail 界面
     实现文件：deplock-gui/src/screens/binding_detail.rs
     - 显示 KeyBinding 完整信息
     - 状态指示器：Active (绿) / Drifted (黄) / Failed (红)
     - 按钮：Verify, Revoke, View Public Key

     Phase 6: Remote Target 支持 (第 7-8 周)

     6.1 SSH Executor 抽象
     实现文件：deplock-core/src/ssh/executor.rs
     - trait SshExecutor: 定义 connect(), exec(), read_file(), disconnect()
     - CommandOutput: 包含 stdout, stderr, exit_code

     6.2 Russh 实现
     实现文件：deplock-core/src/ssh/russh_executor.rs
     - RusshExecutor: 实现 SshExecutor trait
     - 支持 password 和 private_key 认证
     - Host Key 验证：首次连接展示 fingerprint，后续匹配已保存的

     6.3 远程 Key 生成
     实现文件：deplock-core/src/keygen/remote.rs
     - RemoteKeyGenerator::generate(): 支持多种算法
       a. SSH 连接到远程服务器
       b. 执行 mkdir -p 创建目录
       c. 根据算法选择执行：
           - Ed25519: ssh-keygen -t ed25519 -N "" -f <path>
         - RSA: ssh-keygen -t rsa -b 4096 -N "" -f <path>
         - ECDSA: ssh-keygen -t ecdsa -b 256 -N "" -f <path>
       d. 设置权限 chmod 600 <private>; chmod 644 <public>
       e. 只读取 .pub 公钥内容
       f. 返回 RemoteKeyPair { algorithm, public_key, private_key_path }

     6.4 Target Manager 界面
     实现文件：deplock-gui/src/screens/target_manager.rs
     - 列表显示所有 targets (Local + Remote)
     - "Add Remote Target" 按钮 → 弹出表单：
       - Alias (用户友好名称)
       - Host
       - Port (默认 22)
       - Username
       - Auth method: Password / SSH Key
       - Key base dir (默认 ~/.ssh/deplock/github.com/)
     - "Test Connection" 按钮 → 显示连通性检查列表：
       - TCP reachable ✓
       - SSH auth success ✓
       - Host key verified ✓
       - ssh-keygen available ✓
       - Directory writable ✓

     Phase 7: Read-write 权限与安全强化 (第 9 周)

     7.1 Read-write 警告流程
     - Key Forge 界面：Permission toggle 切换到 Read-write 时显示醒目警告
     - 弹出二次确认对话框：
     ⚠️ Read-write Deploy Key can push code to this repository.
     Only continue if you fully trust this target environment.
     - 用户必须勾选 "I understand" 才能继续

     7.2 日志脱敏
     实现文件：deplock-core/src/utils/sanitizer.rs
     - sanitize_log(): 替换敏感信息
       - ghu_****, ghr_****, github_pat_****
       - Authorization: Bearer ****
       - password=****
     - 在所有日志输出点应用

     7.3 零全局污染验证
     - 确认不修改 ~/.ssh/config
     - 确认不修改 ~/.gitconfig
     - Key 生成仅在 ~/.ssh/deplock/ 子目录

     关键文件清单

     核心库 (deplock-core)

     - src/models/{account,target,repository,key_binding}.rs - 数据模型
     - src/db/mod.rs + src/db/*_repository.rs - 数据访问层
     - src/credentials/mod.rs - 凭据管理
     - src/github/{client,auth,deploy_keys,installations}.rs - GitHub API
     - src/ssh/{executor,russh_executor}.rs - SSH 抽象
     - src/keygen/{local,remote}.rs - Key 生成
     - src/services/{key_forge,verification,revoke,target}.rs - 业务服务
     - migrations/001_initial.sql - 数据库 schema

     GUI (deplock-gui)

     - src/main.rs - 入口
     - src/app.rs - Iced Application 主循环
     - src/messages.rs - Message 枚举
     - src/screens/{welcome,auth,repo_list,key_forge,target_manager,binding_detail}.rs - 界面

     验证方案

     端到端测试流程

     1. GitHub Auth:
       - 启动应用 → Welcome 界面
       - 点击 Sign in → 获得 device code
       - 浏览器授权 → 轮询成功 → 跳转到 Repo List
     2. 本地 Key 生成:
       - 选择仓库 → Key Forge 界面
       - Target: Local Machine, Permission: Read-only
       - 点击 Generate & Bind
       - 验证：~/.ssh/deplock/github.com/owner/repo/id_ed25519 存在
       - GitHub 仓库 Settings → Deploy keys 页面出现新 key
       - 执行验证 → git ls-remote 成功
     3. 远程 Target:
       - Target Manager → Add Remote Target
       - 填写服务器信息 → Test Connection → 所有检查通过
       - 返回 Key Forge → 选择 remote target → Generate & Bind
       - 验证：SSH 登录服务器，检查 ~/.ssh/deplock/ 目录有私钥
       - 桌面端日志不包含远程私钥内容
     4. Drift Detection:
       - 手动删除 GitHub Deploy Key → 运行 Verify → 状态变为 drifted
       - 手动删除本地私钥 → Verify → 状态变为 orphaned_remote
     5. Revoke:
       - Binding Detail 界面 → Revoke 按钮 → 选择是否删除私钥
       - GitHub Deploy Key 消失 → 本地状态变为 revoked

     单元测试重点

     - LocalKeyGenerator: 验证生成的 key 格式和权限
     - DeviceFlowAuth: mock HTTP 响应测试轮询逻辑
     - DeployKeyApi: mock GitHub API 测试 CRUD
     - KeyBindingVerifier: mock 各种状态组合测试 drift 检测

     风险与缓解

     风险 1: Russh 复杂度高

     缓解: 先实现 CommandExecutor (调用系统 ssh 命令) 作为 fallback，保证 MVP 可用。Russh 作为后续优化。

     风险 2: GitHub App 注册和 Device Flow 测试困难

     缓解: 提前注册测试用 GitHub App，使用 ngrok 或本地 callback 测试。准备 PAT 作为开发模式 fallback。

     风险 3: 跨平台 keyring 兼容性问题

     缓解: 优先支持 macOS Keychain 和 Linux Secret Service。本期不支持 Windows。提供降级方案（加密本地文件）用于测试环境。

     风险 4: Iced GUI 学习曲线

     缓解: 先实现最简界面（纯文本列表 + 按钮），UI 美化作为后续迭代。参考 Iced 官方示例。

     风险 5: 远程 Key 生成失败回滚

     缓解: 实现事务式操作：先生成 key，验证成功后再创建 GitHub Deploy Key。失败时清理远程临时文件。

     交付物

     MVP (v0.1) 包含：
     - ✅ GitHub App Device Flow 登录
     - ✅ 仓库列表与选择
     - ✅ 本地 Target 自动创建
     - ✅ 远程 Target 添加与连接测试
     - ✅ Read-only Deploy Key 创建（本地 + 远程）
     - ✅ Read-write Deploy Key 创建（含警告）
     - ✅ KeyBinding 验证（git ls-remote 测试）
     - ✅ Revoke 流程
     - ✅ Drift Detection
     - ✅ 基础 Iced GUI（5 个核心界面）

     后续迭代方向

     v0.2:
     - Key Rotation (生成新 key → 替换旧 key)
     - 批量验证和批量撤销
     - Host Key 管理界面

     v0.3:
     - 多 GitHub 账号支持
     - Organization 仓库分组显示
     - SAML SSO 提示

     v0.4:
     - 导入已有 Deploy Keys（通过 fingerprint 匹配）
     - 操作历史和审计日志
     - 导出配置报告

     v0.5:
     - GitHub Enterprise Server 支持
     - GitLab Deploy Keys 适配