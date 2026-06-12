# 国际化（i18n）设计文档

状态：**已实施**（2026-06-11）。本文件既是设计依据，也是现行约束；项目级开发约束见 [`PLAN.md` §开发约束 C1](../PLAN.md)。
范围：`deploykeys-gui` 全部用户可见文案；`deploykeys-core` 错误的本地化策略
默认语言：**English (`en`)**；首发第二语言：**简体中文 (`zh`)**

---

## 1. 目标与非目标

### 目标
- 所有用户可见字符串集中管理，可在英文 / 中文之间切换，**默认英文**。
- 运行时切换语言，无需重启（iced 每帧重建视图，天然支持）。
- 翻译键集合在多语言间保持一致，缺漏可由测试自动发现。
- `deploykeys-core` 不持有面向用户的本地化文案，仅返回稳定的技术错误；本地化由 GUI 层承担。
- 用户的语言选择可持久化，下次启动沿用。

### 非目标（本期不做）
- 复数 / 性别 / 语法格变形（当前文案无此需求；见 §9 选型权衡）。
- 从右到左（RTL）布局。
- 运行时热加载外部翻译文件（翻译在编译期内嵌）。
- 日期 / 数字 / 货币的区域格式化（暂无相关展示）。

---

## 2. 现状盘点

字符串目前硬编码且中英混杂，分布如下：

| 文件 | 现有文案（节选） | 处理 |
|---|---|---|
| `screens/welcome.rs` | `DeployKeys Desktop`、`Target-based GitHub Deploy Key Manager`、`Sign in with GitHub`、`正在连接 GitHub...` | 品牌名保留；其余进词条 |
| `screens/oauth.rs` | `GitHub 认证`、`请在浏览器中完成认证`、`1. 访问以下网址:`、`2. 输入以下代码:`、`正在等待授权…`、`取消` | 全部进词条 |
| `app.rs`（view） | 导航 `Home/Repos/Targets/Keys/Forge`、屏幕标题、`已登录: {login}`、`未登录`、`此界面将在 Phase 4 实现`、`正在请求设备码...` | 全部进词条 |
| `app.rs`（错误） | `数据库初始化失败: {e}`、`数据库不可用，无法登录`、`登录失败: {e}`、`无法确定用户数据目录` | 进词条；`{e}` 部分见 §6 |
| `deploykeys-core`（`error.rs` 等） | 英文技术错误（`Database error: …` 等） | 保持英文，不本地化；GUI 按错误类别映射词条 |

**结论**：`deploykeys-core` 错误已是英文技术信息，符合“核心层不本地化”的目标，无需改动其文案；问题集中在 GUI 层。

---

## 3. 选型

**采用 `rust-i18n`（v3.x）。**

理由：
- 编译期把 `locales/*.yml` 内嵌进二进制，零运行时文件依赖，契合单文件分发。
- `t!("key")` / `t!("key", arg => val)` 宏简洁，返回 `Cow<str>`，可直接喂给 `iced::widget::text`。
- `rust_i18n::set_locale("zh")` 实现运行时全局切换；`available_locales!()` 便于做一致性测试。
- 体积与心智负担小，与项目当前规模匹配。

权衡见 §9（与 Fluent 的对比）。

新增依赖（`crates/deploykeys-gui/Cargo.toml`）：
```toml
rust-i18n = "3"
sys-locale = "0.3"   # 启动时探测系统语言（可选）
```

---

## 4. 目录与文件结构

```
crates/deploykeys-gui/
├── locales/
│   ├── en.yml          # 英文（基准，键的事实来源）
│   └── zh.yml          # 简体中文
├── src/
│   ├── i18n.rs         # Locale 枚举、初始化、切换、系统探测
│   ├── main.rs         # rust_i18n::i18n!("locales", fallback = "en")
│   └── ...
```

`main.rs` 顶部声明（编译期加载 + 回退到 en）：
```rust
rust_i18n::i18n!("locales", fallback = "en");
```

---

## 5. 词条键规范

- 命名：`snake_case`，按界面 / 域分组的点号命名空间。
- 基准语言 `en.yml` 是键的唯一事实来源；其它语言文件必须键集完全一致。
- 插值用具名占位符 `%{name}`，不要用位置参数（顺序在不同语言会变）。

### 5.1 键清单（首版完整集）

```yaml
# en.yml
_version: 1

app:
  brand: "DeployKeys Desktop"          # 品牌名，通常不翻译，入词条以备调整
  tagline: "Target-based GitHub Deploy Key Manager"

nav:
  home: "Home"
  repos: "Repositories"
  targets: "Targets"
  keys: "Key Bindings"
  forge: "Key Forge"

welcome:
  sign_in: "Sign in with GitHub"
  signing_in: "Connecting to GitHub…"

oauth:
  title: "GitHub Authentication"
  instruction: "Complete the authorization in your browser"
  step_visit: "1. Visit this URL:"
  step_code: "2. Enter this code:"
  waiting: "Waiting for authorization, checking every few seconds…"
  requesting_code: "Requesting device code…"

session:
  signed_in_as: "Signed in as %{login}"
  not_signed_in: "Not signed in"

screen:
  placeholder_phase4: "This screen will be implemented in Phase 4"

common:
  cancel: "Cancel"

settings:
  language: "Language"
  language_en: "English"
  language_zh: "简体中文"

error:
  db_init_failed: "Failed to initialize the database: %{detail}"
  db_unavailable_login: "Database unavailable; cannot sign in"
  db_unavailable_save: "Database unavailable; cannot save the session"
  data_dir_unknown: "Could not determine the user data directory"
  device_code_failed: "Failed to obtain device code: %{detail}"
  auth_failed: "Sign-in failed: %{detail}"
  device_code_expired: "The login code has expired; please sign in again"
```

```yaml
# zh.yml
_version: 1

app:
  brand: "DeployKeys Desktop"
  tagline: "基于目标环境的 GitHub Deploy Key 管理器"

nav:
  home: "主页"
  repos: "仓库"
  targets: "目标"
  keys: "密钥绑定"
  forge: "密钥生成"

welcome:
  sign_in: "使用 GitHub 登录"
  signing_in: "正在连接 GitHub…"

oauth:
  title: "GitHub 认证"
  instruction: "请在浏览器中完成认证"
  step_visit: "1. 访问以下网址："
  step_code: "2. 输入以下代码："
  waiting: "正在等待授权，每隔几秒自动检查…"
  requesting_code: "正在请求设备码…"

session:
  signed_in_as: "已登录：%{login}"
  not_signed_in: "未登录"

screen:
  placeholder_phase4: "此界面将在 Phase 4 实现"

common:
  cancel: "取消"

settings:
  language: "语言"
  language_en: "English"
  language_zh: "简体中文"

error:
  db_init_failed: "数据库初始化失败：%{detail}"
  db_unavailable_login: "数据库不可用，无法登录"
  db_unavailable_save: "数据库不可用，无法保存登录状态"
  data_dir_unknown: "无法确定用户数据目录"
  device_code_failed: "获取设备码失败：%{detail}"
  auth_failed: "登录失败：%{detail}"
  device_code_expired: "登录码已过期，请重新登录"
```

---

## 6. 错误本地化策略

**原则：核心层返回稳定的技术错误（英文）；GUI 层决定如何向用户呈现。**

两类文案分离：
1. **GUI 自己产生的失败**（如“数据库不可用，无法登录”）：直接用 `error.*` 词条。
2. **来自 `deploykeys-core` 的错误**（`reqwest`/`sqlx`/keyring 等底层细节）：
   - 不逐句翻译底层英文（既不现实也无价值）。
   - GUI 用一条**已本地化的前缀词条** + 原始技术细节作为 `%{detail}`。
     例：`t!("error.auth_failed", detail => e.to_string())`
   - 技术细节保持英文，便于用户复制、贴 issue、搜索；前缀本地化保证可读性。

后续若要做到错误前缀也完全分类本地化，可在 `deploykeys-core::Error` 上增加稳定的 `code()`（如 `"network.timeout"`），GUI 据 code 选词条——本期不强制，作为演进点（§8 阶段 4）。

---

## 7. 运行时模型

### 7.1 Locale 类型与状态
```rust
// i18n.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale { En, Zh }

impl Locale {
    pub fn code(self) -> &'static str { match self { Locale::En => "en", Locale::Zh => "zh" } }
    pub fn from_code(s: &str) -> Locale {
        if s.starts_with("zh") { Locale::Zh } else { Locale::En }
    }
}
```

`DeployKeysApp` 增加字段 `locale: Locale`。切换语言的消息：
```rust
Message::SetLocale(Locale)  // update 中调用 rust_i18n::set_locale(locale.code()) 并存字段
```

因 iced 每次 `view()` 重新求值，`set_locale` 后下一帧即全量刷新，无需手动重绘。

### 7.2 启动时语言决定顺序
1. 持久化的用户选择（见 §7.3）——最高优先。
2. 系统语言：`sys_locale::get_locale()`，`zh*` → `Zh`，其余 → **`En`**。
3. 兜底：`En`（默认语言）。

### 7.3 持久化
- 复用现有 SQLite，新增 `app_settings(key TEXT PRIMARY KEY, value TEXT)` 迁移（`migrations/002_settings.sql`）。
- 读：启动初始化 DB 后查 `language`；写：`SetLocale` 时 upsert。
- DB 尚未就绪时（如初始化失败）退回系统语言 / 英文，不阻塞 UI。

---

## 8. 实施阶段

| 阶段 | 内容 | 验收 |
|---|---|---|
| 1 | 接入 `rust-i18n`、建 `locales/{en,zh}.yml`、`i18n.rs`、`main.rs` 声明 | 编译通过；`t!("welcome.sign_in")` 返回英文 |
| 2 | 替换 `welcome.rs`/`oauth.rs`/`app.rs` 全部硬编码为 `t!` | 全程默认英文，无残留中文；clippy 0 警告 |
| 3 | `Locale` 状态 + `SetLocale` + 系统探测；Welcome 加语言切换入口 | 运行时英↔中切换即时生效 |
| 4 | `002_settings.sql` + 读写 `language` 偏好 | 重启沿用上次语言 |
| 5 | 键一致性测试 + 文档更新（README/STATUS） | 测试通过；CI 纳入 |

每阶段后跑 `make check`，保持全绿。

---

## 9. 选型权衡：rust-i18n vs Fluent

| 维度 | rust-i18n（采用） | Fluent（备选） |
|---|---|---|
| 复数 / 语法格 | 不支持 | 支持（ICU MessageFormat 级别） |
| 上手成本 | 低，`t!` 宏 | 高，需 `.ftl` 语法与 bundle 管理 |
| 运行时切换 | `set_locale` 全局 | 需自管 bundle |
| 体积 | 小 | 较大（intl-memoizer 等） |
| 适配本项目 | 文案简单、无复数需求，契合 | 偏重，当前收益不足 |

**决策**：当前文案无复数 / 格变需求，rust-i18n 性价比最高。若未来出现 “3 keys / 1 key” 这类复数或大量动态语法，再评估迁移到 Fluent（词条键命名规范已为迁移留好结构）。

---

## 10. 测试与质量门禁

1. **键一致性测试**（`tests/i18n_keys.rs`）：解析两个 YAML，断言键集合完全相等（防止漏翻 / 多余键）。
2. **占位符一致性**：对每个键，断言各语言中 `%{...}` 占位符集合相同。
3. **默认语言断言**：未设置时 `t!("welcome.sign_in") == "Sign in with GitHub"`。
4. **无残留硬编码**（可选 lint）：grep `text("…非 ASCII…")` 在 `src/` 应为空。
5. 字体：CJK 需 `PingFang SC`（macOS）/ `Noto Sans CJK SC`（Linux）覆盖，已在 `main.rs` 设默认字体；切到中文时验证无豆腐块。

---

## 11. 风险

- **字体缺失致中文豆腐块**：依赖系统 CJK 字体；若需跨平台像素级一致，后续可内嵌 Noto Sans SC（约 +8MB，本期不做）。
- **品牌名误译**：`app.brand` 入词条但中英一致，评审时确认是否需要本地化。
- **错误细节英文外泄给中文用户**：属有意为之（§6），技术细节保留英文利于排查；用户可读前缀已本地化。
