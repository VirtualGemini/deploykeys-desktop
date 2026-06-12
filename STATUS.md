# 项目状态（唯一事实来源）

> 本文件取代根目录此前的阶段性报告（PHASE*/S_PLUS/DEEP_CODE_REVIEW 等，已归档至
> `docs/archive/`）。那些报告包含与代码不符的声明（虚构测试、虚报覆盖率与评分），
> 不应作为依据。更新本文件时只写可验证的事实。

最后更新：2026-06-13（从 iced 重构为 Tauri 2 + Leptos 后）

## 架构

桌面端为 Tauri 2 应用：原生宿主 crate `deploykeys-gui`（二进制 `deploykeys`）持有
所有后端能力，经 IPC 命令面桥接 `deploykeys-core`；前端是独立的 Leptos CSR/wasm
crate `deploykeys-ui`，由 Trunk 构建、跑在 webview 里，仅通过 IPC 收发脱敏 DTO，
机密永不跨界。`deploykeys-core` 在重构中原封不动。详见 [ARCHITECTURE.md](ARCHITECTURE.md)。

## 当前能力

| 能力 | 状态 | 说明 |
|---|---|---|
| OAuth Device Flow（核心库） | ✅ 已修复 | 端点指向 github.com、`Accept: application/json`、捕获 refresh_token/expires_in；mockito 测试覆盖全部分支 |
| OAuth 轮询（前端） | ✅ 已修复 | 遵守 interval、slow_down +5s、取消即失效（前端按屏幕状态守卫，见 ui/src/app.rs poll_loop）；轮询经 `poll_github_auth` IPC 命令 |
| 登录态持久化 | ✅ 新增 | AuthService：keyring 存 access/refresh token，accounts 表 upsert |
| Deploy Key CRUD | ✅ 已修复 | DELETE 走 204 通道；owner/repo 路径段校验 |
| 数据库 | ✅ 已修复 | create_if_missing + WAL + 外键强制；`run_migrations()` 真实执行 `sqlx::migrate!` |
| 密钥生成 | ✅ 已修复 | RSA 2048/4096 位长真实生效；部分写入失败清理私钥残留 |
| Key 绑定流程 | ✅ 已加固 | 上传失败清理本地文件；落库失败回滚 GitHub key；阻塞操作走 spawn_blocking |
| 验证/漂移检测 | ⚙️ 部分 | verify_key 按 PLAN 语义产出 Active/Drifted/OrphanedRemote；git ls-remote 实测未实现（Phase 5） |
| 前端界面 | ⚙️ 部分 | Leptos CSR：启动直接进主界面（Main，占位待 Phase 4 的 Repos/Targets/Keys/Forge）；登录改为主界面顶栏按钮按需触发，进入 OAuth 屏走设备流，含错误提示 |
| 国际化（i18n） | ✅ 新增 | 前端内联词条表（ui/src/i18n.rs）+ `t(key)` + Leptos 响应式 locale，英/中双语、默认英文；运行时即时切换，偏好经 IPC 持久化到 app_settings。详见 docs/I18N_DESIGN.md |
| 主题系统 | ✅ 新增 | 全局组件化：语义色板单一来源（ui/styles/input.css 的 `@theme`+`.dark`）+ 响应式 Theme 信号（ui/src/theme.rs），组件只用语义令牌。默认跟随系统（live 跟踪 prefers-color-scheme）。暂无应用内切换器 / 持久化。详见 docs/THEME_DESIGN.md |
| Installations / 仓库同步 | ❌ 未实现 | PLAN Phase 2.3，下一优先级 |
| Token 刷新 | ❌ 未实现 | refresh_token 已存储，刷新流程待做 |
| 远程 Target / SSH | ❌ 未实现 | 仅 trait 定义（Phase 6） |

## 测试

- 测试均为隔离可重复：DB 测试使用临时目录 + 真实迁移 + 外键种子链；
  keyring 走进程内 mock（`credentials/test_support.rs`）；HTTP 走 mockito。
- 覆盖：OAuth 全分支、DeployKey CRUD（含 204 与路径校验）、AuthService upsert、
  KeyBindingService 补偿回滚与漂移状态、DB CRUD/级联/唯一约束、keygen（RSA 位长
  用例标记 `#[ignore]`，需 `cargo test -- --ignored` 显式运行）、枚举往返、日志脱敏。
- 覆盖率数字以 `make coverage`（tarpaulin）实测为准；不要在文档里写估算值。

## 已知技术债

1. sqlx 编译期校验依赖根目录 `deploykeys.db`（`make db-setup` 生成）。迁移到
   `cargo sqlx prepare` 离线缓存后可移出 git。
2. `verify_key` 为 4 次串行查询，可改 JOIN（规模小，暂不紧急）。
3. workspace 根 Cargo.toml 仍声明 russh/russh-keys 等未使用依赖（成员未引用，构建无影响，
   Phase 6 使用 russh 时再清理）。
4. DMG 打包（`cargo tauri build` 的最后一步）依赖 Finder/AppleScript 设置磁盘镜像窗口布局，
   无 GUI 的会话里常失败；`.app` 本体不受影响。需 DMG 时在桌面会话里跑，或简化 tauri.conf.json 的 DMG 配置。
5. `time` crate 钉在 0.3.47：0.3.48 在当前 rustc 下与 cookie/tauri-utils 的 blanket impl 触发
   coherence 冲突，且 plist 1.9 要求 `^0.3.47`，可编译区间只剩这一版（见 Cargo.lock）。

## 验证命令

```bash
make db-setup        # 生成 sqlx 编译期校验库
make check           # fmt + clippy -D warnings + test（仅原生 crate）
cargo test -- --ignored   # 含慢速 RSA 用例（default-members 已排除 wasm UI crate）
make audit           # cargo audit
```
