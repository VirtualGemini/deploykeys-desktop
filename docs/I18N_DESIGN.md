# 国际化（i18n）设计文档

状态：**已实施**（2026-06-13，随 Tauri + Leptos 重构更新）。本文件既是设计依据，也是现行约束；项目级开发约束见 [`PLAN.md` §开发约束 C1](../PLAN.md)。
范围：`deploykeys-ui`（Leptos CSR 前端）全部用户可见文案；`deploykeys-core` 错误的本地化策略。
默认语言：**English (`en`)**；首发第二语言：**简体中文 (`zh`)**

> 实现位置：所有词条与查找逻辑集中在 `crates/deploykeys-ui/src/i18n.rs`。
> 语言偏好的持久化在原生侧（`deploykeys-app`），前端经 IPC 命令
> `get_language` / `set_language` 读写。

---

## 1. 目标与非目标

### 目标
- 所有用户可见字符串集中管理，可在英文 / 中文之间切换，**默认英文**。
- 运行时切换语言，无需重启（Leptos 响应式信号驱动，翻转 locale 即重渲染）。
- 翻译键集合在多语言间保持一致。
- `deploykeys-core` 不持有面向用户的本地化文案，仅返回稳定的技术错误；本地化由前端承担。
- 用户的语言选择可持久化，下次启动沿用。

### 非目标（本期不做）
- 复数 / 性别 / 语法格变形（当前文案无此需求；见 §9 选型权衡）。
- 从右到左（RTL）布局。
- 运行时热加载外部翻译文件（词条编译期内嵌进 wasm）。
- 日期 / 数字 / 货币的区域格式化（暂无相关展示）。

---

## 2. 架构约束：为什么是内联词条表

重构后前端是纯 CSR（client-side rendered）wasm，由 Trunk 构建，跑在 Tauri webview 里。
这带来两条硬约束，直接决定了 i18n 的形态：

1. **前端不依赖 `deploykeys-core`**：core 会拉入 tokio/sqlx/keyring 等 native-only
   依赖，无法编进 wasm。因此词条不能放在 core，也不能复用 core 的任何本地化设施。
2. **运行环境是 wasm，不是原生进程**：没有文件系统读取、没有 `sys-locale` 那套原生
   探测。启动语言来自 webview 的 `navigator.language`（见 `ui/src/app.rs` 的
   `detect_locale`），偏好则由原生侧持久化、经 IPC 回传。

据此，词条以**内联静态字符串表**编译进 wasm（`const EN`/`const ZH`，`&[(&str, &str)]`），
查找用 `t(key)`，响应式由 Leptos `RwSignal<Locale>` 提供。这是当前规模下最直接、零运行时
依赖的选择。

---

## 3. 实现概览（`ui/src/i18n.rs`）

### 3.1 Locale 类型

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale { En, Zh }

impl Locale {
    pub fn code(self) -> &'static str { /* "en" / "zh" */ }
    pub fn from_code(code: &str) -> Locale {
        if code.to_ascii_lowercase().starts_with("zh") { Locale::Zh } else { Locale::En }
    }
}
```

### 3.2 响应式 locale 信号

locale 以 Leptos context 形式在 App 根注入，任何组件都能取用：

```rust
pub fn provide_locale(initial: Locale) { provide_context(RwSignal::new(initial)); }
pub fn locale() -> RwSignal<Locale> { use_context().expect("locale signal provided at root") }
```

`provide_context` 必须在 reactive owner 内调用，因此它在 `App` 组件里执行（见
`ui/src/app.rs`），而非 `main`。

### 3.3 查找函数

```rust
pub fn t(key: &str) -> &'static str {
    let loc = locale().get();                       // 订阅信号 → 切语言即重渲染
    lookup(loc, key).unwrap_or_else(|| lookup(Locale::En, key).unwrap_or("⟨missing⟩"))
}
```

- 因为 `t` 读取 `locale().get()`，凡是在 `view!` 中以 `move || t("…")` 形式使用的文案，
  都会随 locale 信号变化自动刷新。
- 缺键策略：先按当前语言查，缺失回退英文，再缺失返回 `⟨missing⟩`——**让漏翻在 UI 上
  显形**，而不是静默。

### 3.4 词条表

`const EN` 与 `const ZH` 两张 `&[(&str, &str)]` 表并列在文件内。新增文案＝两张表各加一行。

---

## 4. 词条键规范

- 命名：`snake_case`，按界面 / 域分组的点号命名空间（如 `oauth.title`、`nav.repos`）。
- 英文表 `EN` 是键的事实来源；`ZH` 必须键集与之一致。
- 品牌名 `app.brand` 也入表（中英一致），以备将来调整。

### 4.1 当前键清单（节选，完整见源码）

```
app.brand / app.tagline
nav.home / nav.repos / nav.targets / nav.keys / nav.forge
welcome.sign_in / welcome.signing_in
oauth.title / oauth.instruction / oauth.step_visit / oauth.step_code /
  oauth.waiting / oauth.requesting_code / oauth.open_in_browser /
  oauth.copy / oauth.copied
session.not_signed_in
screen.placeholder_phase4
common.cancel / common.sign_out
settings.language
```

> 注：当前键均为**无占位符**的静态串。`t(key)` 返回 `&'static str`，不做插值——
> 需要插值的文案（如「已登录：%{login}」）目前在调用点用 Rust `format!` 拼接
> （见 `app.rs` 的 `signed_in_as`），词条只承担固定前缀部分。若插值需求增多，见 §9。

---

## 5. 错误本地化策略

**原则不变：核心层返回稳定的技术错误（英文）；前端决定如何呈现。**

- **前端自身产生的失败**：直接用 `error.*` 类词条（如需要时新增）。
- **来自 `deploykeys-core` / IPC 的错误**：当前经 IPC 命令以 `Result<_, String>` 回传，
  前端把该字符串原样展示在错误区（见 `screens/welcome.rs` 的错误条、`app.rs` 的
  `error` 信号）。技术细节保持英文，便于复制、贴 issue、搜索。

后续若要给错误加**已本地化前缀 + 英文技术细节**的组合，可在前端按错误类别套词条；
更彻底的做法是让 `deploykeys-core::Error` 暴露稳定 `code()`，前端据 code 选词条——
本期不强制，作为演进点。

---

## 6. 运行时模型

### 6.1 启动时语言决定顺序
1. **持久化的用户选择**——最高优先。前端 bootstrap 时经 IPC `get_language` 读取；
   命中则 `locale().set(Locale::from_code(&code))`。
2. **webview 语言**：`navigator.language()`（`ui/src/app.rs::detect_locale`），
   `zh*` → `Zh`，其余 → `En`。作为 `provide_locale` 的初始值。
3. **兜底**：`En`。

### 6.2 切换与持久化
- 运行时切换：翻转 `locale()` 信号即可，Leptos 下一帧重渲染所有 `t(...)` 文案。
- 持久化：前端调用 IPC `set_language(code)`，原生侧 upsert 到 `app_settings` 表
  （迁移 `migrations/002_settings.sql`，键 `language`）。
- DB 尚未就绪时（如初始化失败）退回 webview 语言 / 英文，不阻塞 UI。

> 现状：词条表与切换机制已就绪，但**面向用户的语言切换入口（设置界面）尚未实现**
> （`Locale::code` 与 `set_language` 标了 `#[allow(dead_code)]`，预留给 Phase 4 的设置屏）。
> 当前语言由「持久化偏好 → webview 语言 → 英文」自动决定。

---

## 7. 与原生侧的接口

| IPC 命令 | 方向 | 作用 |
|---|---|---|
| `get_language` | 前端 ← 原生 | 读取持久化的 `language`（`Option<String>`） |
| `set_language` | 前端 → 原生 | 写入语言偏好（upsert 到 `app_settings`） |

命令定义见 `deploykeys-app/src/lib.rs`，前端封装见 `deploykeys-ui/src/api.rs`。

---

## 8. 与旧方案的差异（重构记录）

iced 时代的设计曾选用 `rust-i18n` + `locales/{en,zh}.yml` + `t!` 宏 + `sys-locale`，
并配 `tests/i18n_keys.rs` 做键一致性门禁。Tauri + Leptos 重构后这套整体废弃，原因：

- 前端是 wasm，`rust-i18n` 的文件内嵌与 `sys-locale` 的原生探测都不再契合；
- 词条规模小（约 20+ 键），内联静态表 + `t(key)` 心智负担更低、零额外依赖；
- 语言探测改由 webview `navigator.language` + 原生持久化承担。

因此本文档自 §2 起描述的均为**现行**实现；`rust-i18n` / `.yml` / `t!` 宏 / iced
「每帧重建视图」等表述属历史方案，不再适用。

---

## 9. 选型权衡：内联表 vs rust-i18n vs Fluent

| 维度 | 内联静态表（采用） | rust-i18n | Fluent |
|---|---|---|---|
| wasm 适配 | 原生契合（纯 Rust 数据） | 文件内嵌可用，但偏重 | 偏重 |
| 复数 / 语法格 | 不支持 | 不支持 | 支持 |
| 插值 | 调用点 `format!` 手工 | `t!` 具名占位符 | ICU 级 |
| 依赖 / 体积 | 零额外依赖 | 一个 crate | 较大 |
| 适配本项目 | 文案少、无复数，最契合 | 规模够但收益不足 | 当前过重 |

**决策**：当前文案简单、无复数需求、且运行在 wasm，内联表性价比最高。若未来出现大量
动态插值或复数 / 格变，再评估迁移到 `rust-i18n`（点号命名规范已为迁移留好结构）或 Fluent。

---

## 10. 质量约束

1. **键集一致性**：新增文案必须同时补 `EN` 与 `ZH`；缺键会在 UI 显形（回退英文或 `⟨missing⟩`）。
   当前靠 review 保证；如需自动化，可加一个比较两表键集的单元测试。
2. **无残留硬编码**：前端 `view!` 中面向用户的字符串必须走 `t(key)`，不得硬编码（含错误前缀）。
3. **CJK 字体**：webview 走系统字体栈，中文依赖系统已装 CJK 字体；如需跨平台像素级一致，
   后续可在前端 CSS 内嵌 Noto Sans SC（本期不做）。

---

## 11. 风险

- **字体缺失致中文豆腐块**：依赖系统 CJK 字体。webview 环境下一般由系统字体覆盖；
  必要时前端 CSS 指定字体族或内嵌字体。
- **错误细节英文外泄给中文用户**：属有意为之（§5），技术细节保留英文利于排查。
- **键一致性靠人工**：放弃了旧的 CI 键集测试；规模再增时应补回自动化检查。
