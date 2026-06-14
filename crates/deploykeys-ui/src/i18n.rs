//! Minimal reactive i18n for the CSR UI.
//!
//! The native side owns the persisted language preference; here we keep a
//! global reactive locale signal and a static string table. `t(key)` reads the
//! current locale, so flipping the signal re-renders every view that calls it.

use leptos::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Zh,
}

impl Locale {
    /// Every supported locale, in menu order. The language picker iterates this,
    /// so adding a language is a matter of extending the enum + this list (plus
    /// its string table below) — no UI changes needed.
    pub const ALL: &'static [Locale] = &[Locale::En, Locale::Zh];

    /// The locale code (`en`/`zh`), used for persistence and `from_code`.
    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Zh => "zh",
        }
    }

    /// The language's own name, shown in the picker (always in that language,
    /// the convention for language selectors so users find their own tongue).
    pub fn native_name(self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::Zh => "简体中文",
        }
    }

    pub fn from_code(code: &str) -> Locale {
        if code.to_ascii_lowercase().starts_with("zh") {
            Locale::Zh
        } else {
            Locale::En
        }
    }
}

/// Provide the locale signal at the app root so any component can reach it.
pub fn provide_locale(initial: Locale) {
    provide_context(RwSignal::new(initial));
}

pub fn locale() -> RwSignal<Locale> {
    use_context::<RwSignal<Locale>>().expect("locale signal provided at root")
}

/// Look up a UI string for the current locale. Falls back to the key itself if
/// missing (which surfaces typos loudly rather than silently).
pub fn t(key: &str) -> &'static str {
    let loc = locale().get();
    lookup(loc, key).unwrap_or_else(|| lookup(Locale::En, key).unwrap_or("⟨missing⟩"))
}

fn lookup(loc: Locale, key: &str) -> Option<&'static str> {
    let table: &[(&str, &str)] = match loc {
        Locale::En => EN,
        Locale::Zh => ZH,
    };
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

const EN: &[(&str, &str)] = &[
    ("app.brand", "DeployKeys Desktop"),
    ("app.tagline", "Target-based GitHub Deploy Keys Manager"),
    ("nav.home", "Home"),
    ("nav.repos", "Repositories"),
    ("nav.connect", "Connect"),
    ("nav.keys", "Keys"),
    ("welcome.sign_in", "Sign in with GitHub"),
    ("welcome.signing_in", "Connecting…"),
    ("signin.title", "Sign in with a token"),
    (
        "signin.instruction",
        "Paste your GitHub fine-grained access token.",
    ),
    ("signin.token_label", "Access token"),
    (
        "signin.resource_owner_help",
        "Choose your personal account or organization.",
    ),
    (
        "signin.repository_access_help",
        "Choose an appropriate repository scope or selected repositories for your situation.",
    ),
    (
        "signin.permissions_help",
        "Administration (Read and write) + Metadata (Read)",
    ),
    ("signin.create_token", "Create a token on GitHub →"),
    ("signin.submit", "Sign in"),
    ("signin.submitting", "Signing in…"),
    ("session.not_signed_in", "Not signed in"),
    (
        "screen.placeholder_phase4",
        "This screen will be implemented in Phase 4",
    ),
    ("common.cancel", "Cancel"),
    ("common.sign_out", "Sign out"),
    ("sidebar.collapse", "Collapse sidebar"),
    ("sidebar.expand", "Expand sidebar"),
    ("settings.language", "Language"),
    ("settings.theme", "Theme"),
    ("settings.placeholder", "Settings"),
    ("quick_routes.title", "Quick routes"),
    ("quick_routes.github_repository", "GitHub"),
    ("quick_routes.feedback", "Feedback"),
    ("quick_routes.support", "Technical support"),
    ("search.placeholder", "Search"),
    ("search.clear", "Clear"),
    ("palette.trigger", "Search"),
    ("palette.placeholder", "Type a command or search..."),
    ("palette.no_results", "No results found"),
    ("palette.empty_history", "No recent searches"),
    ("palette.navigate", "Navigate"),
    ("palette.select", "Select"),
    ("palette.close", "Close"),
    ("palette.toggle_theme", "Toggle theme"),
    ("palette.change_language", "Change language"),
    ("repos.search_placeholder", "Search repositories"),
    ("repos.filter_visibility", "Visibility"),
    ("repos.all", "All"),
    ("repos.public", "Public"),
    ("repos.private", "Private"),
    ("repos.filter_language", "Language"),
    ("repos.all_languages", "All languages"),
    ("repos.other", "Other"),
    (
        "repos.sign_in_required",
        "Sign in with GitHub to sync your repositories.",
    ),
    ("repos.sync", "Sync"),
    ("repos.syncing", "Syncing…"),
    ("repos.loading", "Loading…"),
    (
        "repos.empty",
        "No repositories yet. Click Refresh to sync from GitHub.",
    ),
    (
        "repos.no_match",
        "No repositories match the current filters.",
    ),
    ("repos.archived", "Archived"),
    ("repos.count", "repositories"),
    ("repos.page_size", "Per page"),
    ("repos.page_size_unit", "items"),
    ("repos.page_of", "Page {page} of {pages}"),
    ("repos.total", "Total {}"),
    ("repos.items", "items"),
    ("repos.go_to_page_before", "Go to page "),
    ("repos.go_to_page_after", ""),
];

const ZH: &[(&str, &str)] = &[
    ("app.brand", "DeployKeys Desktop"),
    ("app.tagline", "基于目标环境的 GitHub Deploy Keys 管理器"),
    ("nav.home", "主页"),
    ("nav.repos", "仓库"),
    ("nav.connect", "连接"),
    ("nav.keys", "密钥"),
    ("welcome.sign_in", "使用 GitHub 登录"),
    ("welcome.signing_in", "正在连接…"),
    ("signin.title", "用令牌登录"),
    (
        "signin.instruction",
        "粘贴您的 GitHub fine-grained 访问令牌。",
    ),
    ("signin.token_label", "访问令牌"),
    ("signin.resource_owner_help", "选择个人或您的组织"),
    (
        "signin.repository_access_help",
        "根据你的实际情况选择合适的仓库范围或指定仓库",
    ),
    (
        "signin.permissions_help",
        "Administration（读写）+ Metadata（读）",
    ),
    ("signin.create_token", "在 GitHub 上创建令牌 →"),
    ("signin.submit", "登录"),
    ("signin.submitting", "登录中…"),
    ("session.not_signed_in", "未登录"),
    ("screen.placeholder_phase4", "此界面将在 Phase 4 实现"),
    ("common.cancel", "取消"),
    ("common.sign_out", "退出登录"),
    ("sidebar.collapse", "收起菜单"),
    ("sidebar.expand", "展开菜单"),
    ("settings.language", "语言"),
    ("settings.theme", "主题"),
    ("settings.placeholder", "设置"),
    ("quick_routes.title", "快速路由"),
    ("quick_routes.github_repository", "GitHub"),
    ("quick_routes.feedback", "反馈意见"),
    ("quick_routes.support", "技术支持"),
    ("search.placeholder", "搜索"),
    ("search.clear", "清除"),
    ("palette.trigger", "搜索"),
    ("palette.placeholder", "输入命令或搜索..."),
    ("palette.no_results", "无结果"),
    ("palette.empty_history", "无搜索记录"),
    ("palette.navigate", "导航"),
    ("palette.select", "选择"),
    ("palette.close", "关闭"),
    ("palette.toggle_theme", "切换主题"),
    ("palette.change_language", "切换语言"),
    ("repos.search_placeholder", "搜索仓库"),
    ("repos.filter_visibility", "可见性"),
    ("repos.all", "全部"),
    ("repos.public", "公开"),
    ("repos.private", "私有"),
    ("repos.filter_language", "语言"),
    ("repos.all_languages", "全部语言"),
    ("repos.other", "其他"),
    ("repos.sign_in_required", "使用 GitHub 登录以同步你的仓库"),
    ("repos.sync", "同步"),
    ("repos.syncing", "同步中…"),
    ("repos.loading", "加载中…"),
    ("repos.empty", "暂无仓库。点击刷新从 GitHub 同步。"),
    ("repos.no_match", "没有符合当前筛选条件的仓库。"),
    ("repos.archived", "已归档"),
    ("repos.count", "个仓库"),
    ("repos.page_size", "每页"),
    ("repos.page_size_unit", "条"),
    ("repos.page_of", "第 {page} 页，共 {pages} 页"),
    ("repos.total", "共 {} 条"),
    ("repos.items", "条"),
    ("repos.go_to_page_before", "前往第"),
    ("repos.go_to_page_after", "页"),
];
