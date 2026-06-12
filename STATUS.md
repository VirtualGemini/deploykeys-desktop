# 项目状态（唯一事实来源）

> 本文件取代根目录此前的阶段性报告（PHASE*/S_PLUS/DEEP_CODE_REVIEW 等，已归档至
> `docs/archive/`）。那些报告包含与代码不符的声明（虚构测试、虚报覆盖率与评分），
> 不应作为依据。更新本文件时只写可验证的事实。

最后更新：2026-06-11（全量修复会话）

## 当前能力

| 能力 | 状态 | 说明 |
|---|---|---|
| OAuth Device Flow（核心库） | ✅ 已修复 | 端点指向 github.com、`Accept: application/json`、捕获 refresh_token/expires_in；mockito 测试覆盖全部分支 |
| OAuth 轮询（GUI） | ✅ 已修复 | 遵守 interval、slow_down +5s、expires_in 超时、取消即失效（会话计数器） |
| 登录态持久化 | ✅ 新增 | AuthService：keyring 存 access/refresh token，accounts 表 upsert |
| Deploy Key CRUD | ✅ 已修复 | DELETE 走 204 通道；owner/repo 路径段校验 |
| 数据库 | ✅ 已修复 | create_if_missing + WAL + 外键强制；`run_migrations()` 真实执行 `sqlx::migrate!` |
| 密钥生成 | ✅ 已修复 | RSA 2048/4096 位长真实生效；部分写入失败清理私钥残留 |
| Key 绑定流程 | ✅ 已加固 | 上传失败清理本地文件；落库失败回滚 GitHub key；阻塞操作走 spawn_blocking |
| 验证/漂移检测 | ⚙️ 部分 | verify_key 按 PLAN 语义产出 Active/Drifted/OrphanedRemote；git ls-remote 实测未实现（Phase 5） |
| GUI | ⚙️ 部分 | Welcome/OAuth 完整含错误提示；Repos/Targets/Keys/Forge 为占位（Phase 4） |
| 国际化（i18n） | ✅ 新增 | rust-i18n，英/中双语，默认英文；运行时即时切换、偏好持久化到 app_settings。详见 docs/I18N_DESIGN.md |
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

1. `crates/deploykeys-core/src/services/key_forge.rs` 为空壳待删（与 KeyBindingService 合并）。
2. sqlx 编译期校验依赖根目录 `deploykeys.db`（`make db-setup` 生成）。迁移到
   `cargo sqlx prepare` 离线缓存后可移出 git。
3. `verify_key` 为 4 次串行查询，可改 JOIN（规模小，暂不紧急）。
4. workspace 根 Cargo.toml 仍声明 russh/iced_aw 等未使用依赖（成员未引用，构建无影响，
   Phase 6 使用 russh 时再清理）。

## 验证命令

```bash
make db-setup        # 生成 sqlx 编译期校验库
make check           # fmt + clippy -D warnings + test
cargo test --workspace -- --ignored   # 含慢速 RSA 用例
make audit           # cargo audit
```
