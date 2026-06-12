# 主题设计与维护指南

前端的主题（夜间 / 白天 / 跟随系统）做了**全局组件化**：单一数据源管理主题状态，
单一 CSS 文件管理语义色板。组件只引用语义令牌，不再各自硬编码深浅色。

本文是**每次新增或改动 UI 时维护全局主题的依据**——照着做就不会破坏暗色支持。

---

## 两个事实来源

主题只有两处需要维护，其余全是引用：

| 关心什么 | 改哪里 |
|---|---|
| 颜色值（浅色 / 暗色） | `crates/deploykeys-ui/styles/input.css` 的 `@theme` 与 `.dark` 两个块 |
| 当前生效的主题、何时切 `.dark` | `crates/deploykeys-ui/src/theme.rs` |

组件（`src/app.rs`、`src/screens/*.rs`）**不持有任何颜色定义**，只写语义工具类。

---

## 1. 语义色板（`styles/input.css`）

这是 theming 的唯一色彩来源。结构：

```css
@import "tailwindcss";
@source "../src/**/*.rs";

/* 改为基于 class 的暗色变体（v4 默认是 prefers-color-scheme）。
   .dark 由 theme.rs 切到 <html> 上。 */
@custom-variant dark (&:where(.dark, .dark *));

/* 浅色为默认值 */
@theme {
  --color-bg: #f8fafc;          /* 页面底 */
  --color-surface: #ffffff;     /* 卡片 / 顶栏 */
  --color-content: #1e293b;     /* 主文字 */
  --color-muted: #64748b;       /* 次要文字 / 标签 */
  --color-border: #e2e8f0;      /* 描边 */
  --color-primary: #2563eb;     /* 主操作色 */
  --color-primary-hover: #1d4ed8;
  --color-primary-soft: #dbeafe;/* 主色浅底（标签按钮） */
  --color-on-primary: #ffffff;  /* 主色之上的文字 */
}

/* 暗色覆盖：令牌同名，换暗色值。切 .dark 即整体翻转。 */
.dark {
  --color-bg: #0f172a;
  --color-surface: #1e293b;
  --color-content: #f1f5f9;
  --color-muted: #94a3b8;
  --color-border: #334155;
  --color-primary: #3b82f6;
  --color-primary-hover: #60a5fa;
  --color-primary-soft: #1e3a5f;
  --color-on-primary: #ffffff;
}
```

Tailwind v4 会为 `@theme` 里每个 `--color-X` 自动生成 `bg-X` / `text-X` / `border-X`
等工具类，且这些类引用 `var(--color-X)`。因此 `.dark` 只要覆盖同名变量，所有用到
该令牌的工具类**一次性翻转**——无需在组件里写 `dark:` 双份。

### 现有令牌速查

| 工具类 | 用途 |
|---|---|
| `bg-bg` | 页面级背景 |
| `bg-surface` | 卡片、顶栏等抬升表面 |
| `text-content` | 主文字、标题 |
| `text-muted` | 次要文字、标签、占位 |
| `border-border` | 所有描边 |
| `bg-primary` / `bg-primary-hover` | 主操作按钮（及 hover） |
| `text-on-primary` | 主色按钮上的文字 |
| `bg-primary-soft` / `text-primary` | 主色浅底标签 / 主色文字 |

---

## 2. 主题状态（`src/theme.rs`）

复刻 `i18n.rs` 的模式：root 级 `provide_context(RwSignal<Theme>)`，组件经 `theme()`
读取。一个 `create_effect` 在主题信号变化时把结果落到 `<html>` 的 `.dark` 类上。

- `enum Theme { Light, Dark, System }`，带 `code()` / `from_code()`。
- `provide_theme(initial)`：在 `app.rs` 根组件调用一次（当前传 `Theme::System`）。
- `Light` → 移除 `.dark`；`Dark` → 添加 `.dark`；
- `System` → 读 `prefers-color-scheme: dark` 决定，并注册 media-query 监听器，
  **OS 切换深浅色时实时跟随**；离开 `System` 时监听器随之销毁（`Drop`）。
- `theme()` 访问器目前保留给未来的应用内切换器 / 持久化（标了 `#[allow(dead_code)]`）。

> 当前未做应用内主题切换器，也未持久化偏好——默认 `System`，跟随系统。
> 后续要加切换器：调 `theme().set(Theme::Dark)` 即可，effect 会自动落 `.dark`。

---

## 维护清单（改动 UI 时照做）

1. **只用语义令牌着色**：`bg-bg` / `bg-surface` / `text-content` / `text-muted` /
   `border-border` / `bg-primary` 等。
2. **禁止写裸色阶**：不要 `bg-slate-50`、`text-slate-800`、`bg-white`、`bg-blue-600`
   这类硬编码深浅色——它们不会跟随主题。
3. **基本不写 `dark:` 变体**：语义令牌已自带暗色。仅当某处确需脱离令牌体系微调时
   才用（如 app.rs 的红色错误条用了 `dark:` 兜底）——这是例外，不是常态。
4. **需要新颜色时**：在 `input.css` 的 `@theme` **和** `.dark` 两个块里**成对**新增
   同名 `--color-*`，再在组件里用对应工具类。改这一处即全局可用、自动暗色。
5. **改完跑构建**：`make ui-build`（Trunk 预钩子会用 tools/tailwindcss 重生成
   `styles/output.css`），确认新令牌的工具类已生成、wasm 编译无警告。

---

## 验证

```bash
make ui-build        # Tailwind 重新生成 + wasm 构建
# 暗色翻转：将 <html> 加 class="dark" 应整体变暗（System 模式则切换 OS 深浅色）
# 抽查 OAuth 屏 / 主界面：文字、卡片、描边在深浅两色下对比正常，无残留写死浅色
```
