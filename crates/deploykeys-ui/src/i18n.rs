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
    ("nav.targets", "Targets"),
    ("nav.keys", "Key Bindings"),
    ("nav.forge", "Key Forge"),
    ("welcome.sign_in", "Sign in with GitHub"),
    ("welcome.signing_in", "Connecting…"),
    ("oauth.title", "Connect to GitHub"),
    (
        "oauth.instruction",
        "Authorize DeployKeys in your browser to finish signing in.",
    ),
    ("oauth.step_visit", "Open this link"),
    ("oauth.step_code", "Enter this code"),
    ("oauth.waiting", "Waiting for authorization…"),
    ("oauth.requesting_code", "Requesting device code…"),
    ("oauth.open_in_browser", "Open in browser"),
    ("oauth.copy", "Copy"),
    ("oauth.copied", "Copied"),
    ("session.not_signed_in", "Not signed in"),
    (
        "screen.placeholder_phase4",
        "This screen will be implemented in Phase 4",
    ),
    ("common.cancel", "Cancel"),
    ("common.sign_out", "Sign out"),
    ("settings.language", "Language"),
    ("settings.theme", "Theme"),
    ("search.placeholder", "Search"),
    ("search.clear", "Clear"),
];

const ZH: &[(&str, &str)] = &[
    ("app.brand", "DeployKeys Desktop"),
    ("app.tagline", "基于目标环境的 GitHub Deploy Keys 管理器"),
    ("nav.home", "主页"),
    ("nav.repos", "仓库"),
    ("nav.targets", "目标"),
    ("nav.keys", "密钥绑定"),
    ("nav.forge", "密钥生成"),
    ("welcome.sign_in", "使用 GitHub 登录"),
    ("welcome.signing_in", "正在连接…"),
    ("oauth.title", "连接 GitHub"),
    (
        "oauth.instruction",
        "在浏览器中授权 DeployKeys 以完成登录。",
    ),
    ("oauth.step_visit", "打开此链接"),
    ("oauth.step_code", "输入此代码"),
    ("oauth.waiting", "正在等待授权…"),
    ("oauth.requesting_code", "正在请求设备码…"),
    ("oauth.open_in_browser", "在浏览器中打开"),
    ("oauth.copy", "复制"),
    ("oauth.copied", "已复制"),
    ("session.not_signed_in", "未登录"),
    ("screen.placeholder_phase4", "此界面将在 Phase 4 实现"),
    ("common.cancel", "取消"),
    ("common.sign_out", "退出登录"),
    ("settings.language", "语言"),
    ("settings.theme", "主题"),
    ("search.placeholder", "搜索"),
    ("search.clear", "清除"),
];
