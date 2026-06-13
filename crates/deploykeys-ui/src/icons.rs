//! Icon registry for external SVG assets.
//!
//! SVG shape data lives under `assets/images/svg/icons`. Components reference
//! those files instead of embedding SVG markup or path data in Rust.

use leptos::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Brand,
    Check,
    Close,
    Folder,
    Github,
    GithubDark,
    GithubLight,
    Globe,
    Key,
    Monitor,
    Moon,
    Server,
    SignOut,
    SidebarToggle,
    SidebarToggleFilled,
    Sun,
}

impl IconName {
    pub const fn src(self) -> &'static str {
        match self {
            IconName::Brand => "assets/images/svg/icons/brand.svg",
            IconName::Check => "assets/images/svg/icons/check.svg",
            IconName::Close => "assets/images/svg/icons/close.svg",
            IconName::Folder => "assets/images/svg/icons/folder.svg",
            IconName::Github => "assets/images/svg/icons/github.svg",
            IconName::GithubDark => "assets/images/svg/icons/github-dark.svg",
            IconName::GithubLight => "assets/images/svg/icons/github-light.svg",
            IconName::Globe => "assets/images/svg/icons/globe.svg",
            IconName::Key => "assets/images/svg/icons/key.svg",
            IconName::Monitor => "assets/images/svg/icons/monitor.svg",
            IconName::Moon => "assets/images/svg/icons/moon.svg",
            IconName::Server => "assets/images/svg/icons/server.svg",
            IconName::SignOut => "assets/images/svg/icons/sign-out.svg",
            IconName::SidebarToggle => "assets/images/svg/icons/sidebar-toggle.svg",
            IconName::SidebarToggleFilled => "assets/images/svg/icons/sidebar-toggle-filled.svg",
            IconName::Sun => "assets/images/svg/icons/sun.svg",
        }
    }
}

#[component]
pub fn Icon(name: IconName, #[prop(into, optional)] class: String) -> impl IntoView {
    let base_class = "shrink-0 inline-block";
    let combined_class = if class.is_empty() {
        base_class.to_string()
    } else {
        format!("{} {}", base_class, class)
    };
    let src = name.src();
    let style = format!(
        "background-color: currentColor; mask: url(\"{src}\") center / contain no-repeat; -webkit-mask: url(\"{src}\") center / contain no-repeat;"
    );

    view! {
        <span class=combined_class style=style aria-hidden="true"></span>
    }
}

/// Direct SVG icon - renders the SVG file as an <img> without mask
#[component]
pub fn IconSvg(#[prop(into)] name: Signal<IconName>, #[prop(into, optional)] class: String) -> impl IntoView {
    let base_class = "shrink-0 inline-block";
    let combined_class = if class.is_empty() {
        base_class.to_string()
    } else {
        format!("{} {}", base_class, class)
    };

    view! {
        <img src=move || name.get().src() class=combined_class aria-hidden="true" />
    }
}
