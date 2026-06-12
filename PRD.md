# DeployKeys Desktop 产品需求文档（PRD）v2.0

> **版本定位**：Key-only Edition  
> **产品方向**：目标环境 Deploy Key 自动化工具  
> **产品 Slogan**：目标生钥，仓库授权，私钥不出界。  
> **核心模型**：`1 Repository × 1 Target = 1 Deploy Key`

---

## 目录

1. [产品定位](#1-产品定位)
2. [产品边界](#2-产品边界)
3. [目标用户与核心场景](#3-目标用户与核心场景)
4. [核心概念模型](#4-核心概念模型)
5. [GitHub 授权方案](#5-github-授权方案)
6. [功能模块设计](#6-功能模块设计)
7. [核心流程](#7-核心流程)
8. [状态机与异常恢复](#8-状态机与异常恢复)
9. [安全与隐私要求](#9-安全与隐私要求)
10. [技术路线建议](#10-技术路线建议)
11. [MVP 验收标准](#11-mvp-验收标准)
12. [Roadmap](#12-roadmap)
13. [最终产品定义](#13-最终产品定义)

---

## 1. 产品定位

**DeployKeys Desktop** 是一个本地优先的 **GitHub Deploy Key 自动化配置工具**。

它不做部署、不做 CI/CD、不做 Hook、不做拉代码执行任务。它只解决一个问题：

> 让用户在指定目标环境中生成 SSH Key，并自动把公钥绑定到指定 GitHub 仓库，权限可选只读或可写，私钥永远留在目标环境。

### 1.1 一句话价值主张

> DeployKeys 让用户通过桌面 GUI，把本机或远程服务器安全授权给指定 GitHub 仓库，而不需要手写 SSH 配置、手动复制公钥、手动创建 Deploy Key。

### 1.2 产品不是

DeployKeys Desktop 不是：

- CI/CD 平台
- GitHub Actions 替代品
- 通用 SSH 客户端
- 服务器部署工具
- 进程管理工具
- 云端密钥托管服务
- 团队协作平台

DeployKeys 的核心是：

> **Target-based Deploy Key Forge**

即：面向目标环境的 Deploy Key 自动化铸造工具。

---

## 2. 产品边界

## 2.1 MVP 只做三件事

### 第一，GitHub 授权

用户通过 GitHub App OAuth Device Flow 登录 DeployKeys，并选择允许 DeployKeys 访问哪些仓库。

### 第二，目标环境生钥

用户选择：

```text
GitHub 仓库
目标环境：Local 或 Remote
权限：Read-only 或 Read-write
```

DeployKeys 在目标环境中生成 SSH Key。

### 第三，绑定公钥到仓库

DeployKeys 只读取目标环境中的 `.pub` 公钥，然后调用 GitHub API，把公钥作为 Deploy Key 添加到目标仓库。

---

## 2.2 MVP 明确不做

MVP 阶段不做以下功能：

```text
不做部署
不做 git clone
不做 git pull
不做 Shell Hook
不做 CI/CD
不做 Webhook
不做服务器进程管理
不做日志终端
不做团队协作
不做云端同步
不做 GitLab / Gitea 适配
不做 Kubernetes / Docker 编排
```

这样产品边界会非常清晰：  
DeployKeys 只负责 **目标环境访问 GitHub 私有仓库的最小授权自动化**。

---

## 3. 目标用户与核心场景

## 3.1 外包开发者 / 自由职业者

### 用户痛点

外包开发者可能同时维护多个客户的私有仓库。如果所有项目都依赖同一把个人 SSH Key，容易造成权限混淆和安全边界模糊。

### 使用 DeployKeys 后

用户可以为每个客户、每个仓库生成独立 key：

```text
client-a/api     -> ~/.ssh/deploykeys/github.com/client-a/api/id_ed25519
client-b/web     -> ~/.ssh/deploykeys/github.com/client-b/web/id_ed25519
client-c/admin   -> ~/.ssh/deploykeys/github.com/client-c/admin/id_ed25519
```

每个仓库一把独立 Deploy Key，权限边界清晰。

---

## 3.2 独立开发者 / 小团队

### 用户痛点

开发者想让云服务器访问私有仓库，但不想把自己的 GitHub 主 SSH Key 上传到服务器，也不想手动复制公钥、登录 GitHub 页面创建 Deploy Key。

### 使用 DeployKeys 后

用户只需要：

```text
登录 GitHub
添加服务器
选择仓库
选择服务器目标环境
选择 Read-only
点击 Generate & Bind
```

DeployKeys 会在服务器本地生成私钥，只把 `.pub` 公钥绑定到 GitHub 仓库。

---

## 3.3 需要目标环境写权限的高级用户

### 使用场景

某些目标环境可能需要向仓库写入内容，例如：

```text
push tag
push generated file
自动更新版本号
自动提交构建产物
```

### 产品策略

默认权限必须是：

```text
Read-only
```

用户选择 Read-write 时，需要强提示：

```text
Read-write Deploy Key 可以向仓库写入内容。
仅在你明确需要目标环境 push 代码、tag 或自动提交时使用。
```

---

## 4. 核心概念模型

DeployKeys 的核心安全单元不是简单的 `Repo = Key`，而是：

```text
Repository × Target = KeyBinding
```

原因是：

- 同一个仓库可能授权给本地电脑、测试服务器、生产服务器。
- 同一个服务器可能需要访问多个仓库。
- GitHub Deploy Key 本身只授予单个仓库访问权限。
- 每个目标环境都应该拥有独立私钥。

---

## 4.1 Account

代表一个 GitHub 登录身份。

```text
Account
- id
- github_user_id
- login
- avatar_url
- auth_type: github_app_device_flow
- token_ref
- refresh_token_ref
- token_expires_at
- created_at
- last_login_at
```

说明：

- `token_ref` 和 `refresh_token_ref` 只保存系统凭据管理器中的引用。
- Token 明文不得进入 SQLite、本地日志或错误报告。

---

## 4.2 GitHubInstallation

代表 DeployKeys GitHub App 被安装到哪个 GitHub 账号或组织。

```text
GitHubInstallation
- id
- github_installation_id
- account_owner
- account_type: user | org
- permissions_snapshot
- repository_selection: all | selected
- last_synced_at
```

---

## 4.3 Repository

代表 DeployKeys 当前可访问的 GitHub 仓库。

```text
Repository
- github_repo_id
- installation_id
- owner
- name
- full_name
- private
- archived
- default_branch
- ssh_url
- html_url
- permissions_snapshot
- last_synced_at
```

---

## 4.4 Target

目标环境是私钥真正存在的位置。

```text
Target
- id
- type: local | remote
- alias
- os: macos | windows | linux | unknown
- host
- port
- username
- auth_method: password | ssh_key
- auth_ref
- key_base_dir
- status
- host_key_fingerprint
- created_at
- last_checked_at
```

### Local Target

系统默认存在：

```text
Local Machine
```

不可删除。

### Remote Target

用户手动添加，例如：

```text
Prod Server
Staging Server
Client A VPS
```

---

## 4.5 KeyBinding

DeployKeys 的核心业务对象。

```text
KeyBinding
- id
- repo_id
- target_id
- github_deploy_key_id
- deploy_key_title
- permission: read_only | read_write
- public_key
- public_key_fingerprint
- private_key_path
- private_key_residency: local | remote
- status: active | pending | failed | drifted | orphaned | revoked
- created_at
- last_verified_at
```

Remote Target 的 `private_key_path` 只保存路径元数据，例如：

```text
remote://prod-server-1/~/.ssh/deploykeys/github.com/owner/repo/id_ed25519
```

桌面端永远不保存、读取、缓存远程私钥内容。

---

## 5. GitHub 授权方案

## 5.1 推荐主线：GitHub App + OAuth Device Flow

DeployKeys 应使用：

```text
GitHub App
+ OAuth 2.0 Device Flow
+ GitHub App User Access Token
```

该方案适合桌面端应用，用户无需手动创建 PAT，也不需要 DeployKeys 在客户端内置 GitHub App client secret。

### 核心优势

```text
用户不需要手动创建 PAT
用户可以选择 App 能访问哪些仓库
Token 权限受 GitHub App 权限和用户自身权限共同限制
Token 生命周期更短
支持 refresh token
不会因为撤销 OAuth token 而删除已创建的 Deploy Key
```

---

## 5.2 不推荐传统 OAuth App 作为主线

传统 OAuth App 不适合作为主线，原因如下：

### 第一，权限过宽

传统 OAuth App 往往需要请求较宽的 `repo` scope。对于一个只负责创建 Deploy Key 的工具来说，这个权限显得过大。

### 第二，Deploy Key 生命周期风险

如果使用传统 OAuth App token 或 PAT 创建 Deploy Key，当用户删除相关 token 时，GitHub 可能会删除由该 token 创建的 Deploy Key。  
这会导致目标环境原本可用的仓库访问突然失效。

### 第三，安装范围不够细

GitHub App 可以安装到指定仓库，更符合 DeployKeys 的最小权限原则。

---

## 5.3 PAT 只作为高级开发者模式

PAT 可以保留为隐藏的高级选项，用于：

```text
本地开发测试
GitHub Enterprise 特殊环境
用户明确不愿安装 GitHub App 的场景
```

但产品默认入口不应是 PAT。

---

## 5.4 GitHub App 最小权限设计

GitHub App 建议请求：

```text
Repository metadata: read
Repository administration: read/write
```

### 为什么需要 Administration 权限？

因为 GitHub 把 Deploy Key 管理归类为仓库管理能力。  
即使 DeployKeys 创建的是只读 Deploy Key，也需要调用仓库 Deploy Key API，因此需要仓库级 Administration 写权限。

### 推荐 UI 解释文案

```text
DeployKeys 需要 Repository Administration 权限，不是为了读取或修改你的代码，
而是因为 GitHub 把 Deploy Key 管理归类为仓库管理权限。
你仍然可以为目标环境创建只读 Deploy Key。
```

---

## 6. 功能模块设计

## 6.1 模块一：GitHub 授权中心

### 目标

让用户通过 GitHub App Device Flow 完成登录、安装和仓库授权。

### 功能需求

```text
GitHub App Device Flow 登录
显示当前 GitHub 用户
显示 App 安装账号
显示 App 可访问仓库范围
支持重新授权
支持刷新 token
支持退出登录
支持权限不足提示
```

### 关键 UI 文案

```text
DeployKeys 使用 GitHub App 进行授权。
它只会访问你安装时选择的仓库。
它需要仓库 Administration 权限来创建和删除 Deploy Key。
它不会读取你的代码内容。
```

---

## 6.2 模块二：Repository Picker

### 目标

让用户从 GitHub App 可访问的仓库中选择目标仓库。

### 功能需求

```text
展示 GitHub App 可访问仓库
按 owner / org 过滤
按 private / public 过滤
按 archived 状态过滤
按是否已绑定 key 过滤
搜索 repo full_name
显示当前用户是否具备足够权限
```

### 仓库卡片建议

```text
owner/repo
Private · main

Targets:
- Local Machine · read-write · active
- Prod Server · read-only · active
```

---

## 6.3 模块三：Target Manager

### 目标

管理私钥实际存在的位置。

### Local Target 功能

```text
默认创建 Local Machine
配置本地 key 根目录
配置默认命名规则
检查本地目录权限
检查 ssh-keygen 或 Rust 本地生钥能力
```

推荐本地目录：

```text
~/.ssh/deploykeys/github.com/<owner>/<repo>/id_ed25519
~/.ssh/deploykeys/github.com/<owner>/<repo>/id_ed25519.pub
```

权限要求：

```text
directory: 700
private key: 600
public key: 644
```

### Remote Target 字段

```text
Alias
Host
Port
Username
Auth method: password | ssh_key
Password credential ref
SSH private key path
SSH key passphrase credential ref
Remote key base dir
```

### Remote Target 连通性测试

不要只做单个红绿灯，应拆成检查列表：

```text
TCP reachable
SSH auth success
Host key verified
ssh-keygen available
mkdir/chmod available
key directory writable
```

---

## 6.4 模块四：Key Forge

### 目标

在指定目标环境中生成 key，并绑定公钥到指定 GitHub 仓库。

### 输入

```text
Repository
Target
Permission: Read-only | Read-write
Key title
Key path
```

### 输出

```text
KeyBinding
GitHub Deploy Key ID
Public key fingerprint
Target private key path
Verification status
```

### 权限选择

默认：

```text
Read-only
```

可选：

```text
Read-write
```

Read-write 必须二次确认。

---

## 6.5 模块五：Verify & Drift Detection

### 目标

确认 DeployKeys 记录、GitHub 状态、目标环境文件状态三者一致。

### Verify 检查项

```text
GitHub Deploy Key 是否存在
GitHub Deploy Key read_only 状态是否符合记录
目标环境私钥文件是否存在
目标环境公钥 fingerprint 是否匹配
目标环境是否能通过该 key 访问 git@github.com:OWNER/REPO.git
```

### 只读 Key 验证

推荐使用：

```bash
GIT_SSH_COMMAND="ssh -i <key_path> -o IdentitiesOnly=yes" git ls-remote git@github.com:OWNER/REPO.git
```

### 可写 Key 验证

默认不执行 push 测试，避免污染仓库。  
可写权限默认通过 GitHub API 返回的 `read_only=false` 判断。

未来可提供高级验证：

```text
创建临时分支
push 空提交或 tag
立即删除临时分支或 tag
```

但 MVP 不做。

---

## 6.6 模块六：Revoke / Rotate

### Revoke

撤销某个 KeyBinding。

流程：

```text
1. 删除 GitHub Deploy Key
2. 用户选择是否删除目标环境私钥
3. 用户选择是否仅删除本地元数据
4. 更新状态为 revoked
```

### Rotate

Deploy Key 应视为不可变对象。  
如果需要更新 key，应删除旧 key 并创建新 key。

流程：

```text
1. 在目标环境生成新 key
2. 创建新的 GitHub Deploy Key
3. 验证新 key 可用
4. 删除旧 GitHub Deploy Key
5. 可选归档或删除旧私钥
6. 更新 KeyBinding
```

---

## 7. 核心流程

## 7.1 首次授权流程

```text
1. 用户打开 DeployKeys
2. 点击 Sign in with GitHub
3. DeployKeys 启动 GitHub App Device Flow
4. 用户在浏览器中输入 device code
5. 用户安装 DeployKeys GitHub App
6. 用户选择允许访问的仓库
7. DeployKeys 获得 GitHub App user access token
8. DeployKeys 同步可访问仓库列表
```

---

## 7.2 Local Target 生钥流程

```text
1. 用户选择 Repository
2. 用户选择 Local Machine
3. 用户选择权限：Read-only / Read-write
4. DeployKeys 显示执行计划
5. 用户确认
6. 本地创建目录
7. 本地生成 Ed25519 key pair
8. 设置本地文件权限
9. 读取 public key
10. 调用 GitHub API 创建 Deploy Key
11. 保存 KeyBinding
12. 执行 Verify
```

### 推荐执行计划展示

```text
Repository:
owner/repo

Target:
Local Machine

Permission:
Read-only

Private Key Path:
~/.ssh/deploykeys/github.com/owner/repo/id_ed25519

GitHub Deploy Key Title:
deploykeys/local-machine/owner-repo
```

---

## 7.3 Remote Target 生钥流程

```text
1. 用户选择 Repository
2. 用户选择 Remote Target
3. 用户选择权限：Read-only / Read-write
4. DeployKeys 测试 SSH 连通性
5. DeployKeys 显示远程 key 路径
6. 用户确认
7. SSH 登录目标服务器
8. 在服务器创建 key 目录
9. 在服务器执行 ssh-keygen
10. 设置文件权限
11. DeployKeys 只读取 .pub 公钥
12. 调用 GitHub API 创建 Deploy Key
13. 保存 KeyBinding
14. 执行 Verify
```

### 推荐远程命令

```bash
umask 077
mkdir -p ~/.ssh/deploykeys/github.com/OWNER/REPO
ssh-keygen -t ed25519 -N ""   -C "deploykeys:github.com:OWNER/REPO:TARGET_ID"   -f ~/.ssh/deploykeys/github.com/OWNER/REPO/id_ed25519
chmod 700 ~/.ssh ~/.ssh/deploykeys ~/.ssh/deploykeys/github.com
chmod 600 ~/.ssh/deploykeys/github.com/OWNER/REPO/id_ed25519
chmod 644 ~/.ssh/deploykeys/github.com/OWNER/REPO/id_ed25519.pub
```

---

## 8. 状态机与异常恢复

## 8.1 KeyBinding 状态

```text
pending
active
failed
drifted
orphaned_local
orphaned_remote
revoked
```

### 状态解释

| 状态 | 含义 |
|---|---|
| pending | 流程执行中 |
| active | GitHub key 存在，目标私钥存在，验证通过 |
| failed | 创建过程失败 |
| drifted | GitHub 状态和 DeployKeys 记录不一致 |
| orphaned_local | 目标环境有私钥，但 GitHub 无 Deploy Key |
| orphaned_remote | GitHub 有 Deploy Key，但目标环境私钥不存在 |
| revoked | 已撤销 |

---

## 8.2 失败恢复策略

| 失败场景 | 产品行为 |
|---|---|
| 目标环境 key 已存在 | 提示复用、覆盖、换路径 |
| GitHub API 创建失败 | 保留本地/远程 orphan key，允许重试绑定 |
| GitHub key 创建成功但本地 DB 写入失败 | 下次启动通过 title/fingerprint 识别并导入 |
| 用户删除 GitHub Deploy Key | 标记为 drifted，允许重新绑定或删除本地元数据 |
| 用户删除目标环境私钥 | 标记为 orphaned_remote，允许重新生成 |
| 权限不足 | 明确提示需要 Repository Administration write |
| OAuth token 过期 | 自动 refresh |
| refresh token 过期 | 重新走 Device Flow |
| SSH Host Key 变化 | 阻止连接，提示可能存在服务器重装或中间人攻击 |

---

## 8.3 幂等性要求

同一组：

```text
Repository × Target
```

默认只能存在一个 active KeyBinding。

重复创建时，DeployKeys 应提供：

```text
Verify
Rotate
Revoke
Import Existing
```

而不是直接创建第二把 key。

---

## 9. 安全与隐私要求

## 9.1 私钥不离目标

### Local 模式

```text
私钥只存在用户电脑
```

### Remote 模式

```text
私钥只存在服务器
DeployKeys 只读取 .pub 公钥
DeployKeys 不读取、不缓存、不打印远程私钥
```

---

## 9.2 GitHub Token 存储

所有 token 必须进入系统原生凭据管理器：

```text
macOS Keychain
Windows Credential Manager
Linux Secret Service / keyring
```

SQLite 只保存 token 引用，不保存明文 token。

---

## 9.3 Remote Target 凭据存储

以下内容必须进入系统原生凭据管理器：

```text
SSH password
SSH private key passphrase
GitHub user access token
GitHub refresh token
```

---

## 9.4 Host Key Pinning

Remote Target 首次连接时：

```text
展示 host key fingerprint
用户确认后保存
后续 fingerprint 变化时阻止连接
```

禁止默认使用：

```bash
StrictHostKeyChecking=no
```

---

## 9.5 日志脱敏

UI 日志和本地日志必须脱敏：

```text
ghu_****
ghr_****
github_pat_****
ghp_****
Authorization: Bearer ****
password=****
```

---

## 9.6 零全局污染

DeployKeys 不应修改：

```text
~/.ssh/config
~/.gitconfig
/etc/ssh/ssh_config
```

因为 DeployKeys Key-only Edition 不做 clone、不做 deploy，也不应改变用户全局 SSH 或 Git 行为。

---

## 9.7 Read-write Key 强警告

Read-write Deploy Key 必须被视为高风险权限。  
用户选择时需要二次确认。

推荐文案：

```text
你正在创建 Read-write Deploy Key。
目标环境将可以向该仓库 push 代码、tag 或其他 Git 对象。
仅在你完全信任该目标环境，并明确需要写入权限时继续。
```

---

## 10. 技术路线建议

## 10.1 前端 GUI

推荐：

```text
Iced
```

原因：

```text
纯 Rust
跨平台
适合桌面端
状态驱动 UI
适合构建安全工具类产品
```

---

## 10.2 GitHub API

推荐：

```text
reqwest + rustls
```

建议配置：

```toml
reqwest = { version = "...", default-features = false, features = ["json", "rustls-tls"] }
```

---

## 10.3 SSH

推荐优先抽象接口：

```rust
trait SshExecutor {
    async fn connect(&self) -> Result<()>;
    async fn exec(&self, command: &str) -> Result<CommandOutput>;
    async fn read_file(&self, path: &str) -> Result<String>;
}
```

实现层可以优先考虑：

```text
russh
```

如果 `russh` 工程成本过高，可临时提供系统 `ssh` 命令 fallback。

---

## 10.4 本地数据库

推荐：

```text
SQLite
```

保存内容：

```text
accounts metadata
installations
repositories
targets
key_bindings
operation_logs
```

不保存内容：

```text
GitHub token 明文
refresh token 明文
SSH password
SSH private key passphrase
远程私钥内容
```

---

## 10.5 本地 Key 生成

本地模式建议使用 Rust 原生库生成 OpenSSH 兼容的 Ed25519 key。  
远程模式建议使用目标服务器上的 `ssh-keygen`，以确保私钥直接诞生于目标环境。

---

## 11. MVP 验收标准

## 11.1 GitHub Auth

```text
用户可以通过 GitHub App Device Flow 登录
用户可以安装 DeployKeys GitHub App 到指定仓库
DeployKeys 能列出 App 可访问仓库
Token 存储不落明文
Token 过期后可刷新
权限不足时能明确解释原因
```

---

## 11.2 Local Key

```text
用户可以选择仓库 + Local Target + Read-only
DeployKeys 本地生成 Ed25519 key
GitHub 仓库 Deploy Keys 页面出现对应 key
DeployKeys 能验证 git ls-remote 成功
私钥权限为 600
公钥权限为 644
```

---

## 11.3 Remote Key

```text
用户可以添加远程服务器
首次连接展示 host key fingerprint
服务器本地生成 Ed25519 key
DeployKeys 只读取 .pub
GitHub Deploy Key 创建成功
服务器上 git ls-remote 验证成功
桌面端日志中不出现远程私钥内容
```

---

## 11.4 Read-write Key

```text
用户选择 Read-write 时出现强警告
用户需要二次确认
GitHub API 创建 read_only=false 的 Deploy Key
UI 明确标记该 key 可写
默认不执行 push 测试
```

---

## 11.5 Revoke

```text
用户可以删除 GitHub Deploy Key
用户可以选择是否删除目标环境私钥
用户可以选择是否只删除本地元数据
删除后状态变为 revoked
```

---

## 11.6 Drift Detection

```text
删除 GitHub 上的 Deploy Key 后，DeployKeys 能发现 drift
删除目标环境私钥后，DeployKeys 能发现 orphan
重复创建同一 Repository × Target key 时，DeployKeys 阻止重复创建
```

---

## 12. Roadmap

## v0.1：Key Forge MVP

```text
GitHub App Device Flow 登录
GitHub App installation 仓库选择
Repository Picker
Local Target
Remote Target
Read-only Deploy Key 创建
Read-write Deploy Key 创建
Verify
Revoke
```

---

## v0.2：状态治理

```text
Drift Detection
Orphan Key Recovery
Key Rotation
批量 Verify
批量 Revoke
Host Key 管理
```

---

## v0.3：多账号与组织体验

```text
多个 GitHub Account
多个 GitHub Installation
Org / User 分组
权限不足解释
SAML SSO 提示
```

---

## v0.4：导入与审计

```text
导入已有 Deploy Keys
按 fingerprint 匹配目标环境 key
导出审计报告
操作历史
变更日志
```

---

## v0.5：平台扩展

```text
GitHub Enterprise Server
GitLab Deploy Keys
Gitea Deploy Keys
多 Git 平台统一管理
```

---

## 13. 最终产品定义

**DeployKeys Desktop** 是一个通过 GitHub App OAuth 授权的 Target-based Deploy Key Forge。

它在本机或远程服务器生成 SSH 私钥，只把公钥绑定到指定 GitHub 仓库，并让用户用 GUI 管理每个仓库、每个目标环境的只读或可写访问权。

最终产品公式：

```text
DeployKeys = GitHub App Auth + Target Key Generation + Deploy Key Binding + Verify / Revoke / Rotate
```

它不负责部署。  
它只负责一件事：

> **让目标环境以最小权限、安全、可审计的方式访问指定 GitHub 仓库。**

---

## 参考资料

- GitHub REST API: Deploy Keys  
  <https://docs.github.com/en/rest/deploy-keys/deploy-keys>

- GitHub Docs: Managing deploy keys  
  <https://docs.github.com/en/authentication/connecting-to-github-with-ssh/managing-deploy-keys>

- GitHub Docs: Generating a user access token for a GitHub App  
  <https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app>

- GitHub Docs: Differences between GitHub Apps and OAuth Apps  
  <https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/differences-between-github-apps-and-oauth-apps>

- GitHub Docs: Scopes for OAuth Apps  
  <https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/scopes-for-oauth-apps>