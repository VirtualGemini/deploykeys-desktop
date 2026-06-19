//! Icon registry for external SVG assets.
//!
//! SVG shape data lives under `assets/images/svg/icons`. Components reference
//! those files instead of embedding SVG markup or path data in Rust.

use leptos::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Check,
    ChevronLeft,
    ChevronRight,
    Close,
    Copy,
    CopyDone,
    Delete,
    Download,
    Edit,
    Folder,
    Github,
    Globe,
    Key,
    Monitor,
    Moon,
    MoreVertical,
    Power,
    QuickRoutes,
    Server,
    SettingsBack,
    SettingsPlaceholder,
    SignOut,
    SidebarToggle,
    SidebarToggleFilled,
    Sun,
    TutorialHelp,
}

impl IconName {
    pub const fn src(self) -> &'static str {
        match self {
            IconName::Check => "assets/images/svg/icons/check.svg",
            IconName::ChevronLeft => "assets/images/svg/icons/chevron-left.svg",
            IconName::ChevronRight => "assets/images/svg/icons/chevron-right.svg",
            IconName::Close => "assets/images/svg/icons/close.svg",
            IconName::Copy => "assets/images/svg/icons/copy.svg",
            IconName::CopyDone => "assets/images/svg/icons/copy-done.svg",
            IconName::Delete => "assets/images/svg/icons/delete.svg",
            IconName::Download => "assets/images/svg/icons/download.svg",
            IconName::Edit => "assets/images/svg/icons/edit.svg",
            IconName::Folder => "assets/images/svg/icons/folder.svg",
            IconName::Github => "assets/images/svg/icons/github.svg",
            IconName::Globe => "assets/images/svg/icons/globe.svg",
            IconName::Key => "assets/images/svg/icons/key.svg",
            IconName::Monitor => "assets/images/svg/icons/monitor.svg",
            IconName::Moon => "assets/images/svg/icons/moon.svg",
            IconName::MoreVertical => "assets/images/svg/icons/more-vertical.svg",
            IconName::Power => "assets/images/svg/icons/power.svg",
            IconName::QuickRoutes => "assets/images/svg/icons/quick-routes.svg",
            IconName::Server => "assets/images/svg/icons/server.svg",
            IconName::SettingsBack => "assets/images/svg/icons/settings-back.svg",
            IconName::SettingsPlaceholder => "assets/images/svg/icons/settings-placeholder.svg",
            IconName::SignOut => "assets/images/svg/icons/sign-out.svg",
            IconName::SidebarToggle => "assets/images/svg/icons/sidebar-toggle.svg",
            IconName::SidebarToggleFilled => "assets/images/svg/icons/sidebar-toggle-filled.svg",
            IconName::Sun => "assets/images/svg/icons/sun.svg",
            IconName::TutorialHelp => "assets/images/svg/icons/tutorial-help.svg",
        }
    }

    const fn svg(self) -> &'static str {
        match self {
            IconName::Check => include_str!("../assets/images/svg/icons/check.svg"),
            IconName::ChevronLeft => include_str!("../assets/images/svg/icons/chevron-left.svg"),
            IconName::ChevronRight => include_str!("../assets/images/svg/icons/chevron-right.svg"),
            IconName::Close => include_str!("../assets/images/svg/icons/close.svg"),
            IconName::Copy => include_str!("../assets/images/svg/icons/copy.svg"),
            IconName::CopyDone => include_str!("../assets/images/svg/icons/copy-done.svg"),
            IconName::Delete => include_str!("../assets/images/svg/icons/delete.svg"),
            IconName::Download => include_str!("../assets/images/svg/icons/download.svg"),
            IconName::Edit => include_str!("../assets/images/svg/icons/edit.svg"),
            IconName::Folder => include_str!("../assets/images/svg/icons/folder.svg"),
            IconName::Github => include_str!("../assets/images/svg/icons/github.svg"),
            IconName::Globe => include_str!("../assets/images/svg/icons/globe.svg"),
            IconName::Key => include_str!("../assets/images/svg/icons/key.svg"),
            IconName::Monitor => include_str!("../assets/images/svg/icons/monitor.svg"),
            IconName::Moon => include_str!("../assets/images/svg/icons/moon.svg"),
            IconName::MoreVertical => include_str!("../assets/images/svg/icons/more-vertical.svg"),
            IconName::Power => include_str!("../assets/images/svg/icons/power.svg"),
            IconName::QuickRoutes => include_str!("../assets/images/svg/icons/quick-routes.svg"),
            IconName::Server => include_str!("../assets/images/svg/icons/server.svg"),
            IconName::SettingsBack => include_str!("../assets/images/svg/icons/settings-back.svg"),
            IconName::SettingsPlaceholder => {
                include_str!("../assets/images/svg/icons/settings-placeholder.svg")
            }
            IconName::SignOut => include_str!("../assets/images/svg/icons/sign-out.svg"),
            IconName::SidebarToggle => {
                include_str!("../assets/images/svg/icons/sidebar-toggle.svg")
            }
            IconName::SidebarToggleFilled => {
                include_str!("../assets/images/svg/icons/sidebar-toggle-filled.svg")
            }
            IconName::Sun => include_str!("../assets/images/svg/icons/sun.svg"),
            IconName::TutorialHelp => include_str!("../assets/images/svg/icons/tutorial-help.svg"),
        }
    }
}

#[component]
pub fn Icon(name: IconName, #[prop(into, optional)] class: String) -> impl IntoView {
    let base_class = "shrink-0 inline-flex items-center justify-center";
    let combined_class = if class.is_empty() {
        base_class.to_string()
    } else {
        format!("{} {}", base_class, class)
    };
    let svg = name
        .svg()
        .replace(
            "<svg ",
            "<svg style=\"width:100%;height:100%;display:block;\" ",
        )
        .replace("#fff", "currentColor")
        .replace("#FFF", "currentColor")
        .replace("#ffffff", "currentColor")
        .replace("#FFFFFF", "currentColor");

    view! {
        <span class=combined_class inner_html=svg aria-hidden="true"></span>
    }
}

/// Direct SVG icon - renders the SVG file as an <img> without mask
#[component]
pub fn IconSvg(
    #[prop(into)] name: Signal<IconName>,
    #[prop(into, optional)] class: String,
) -> impl IntoView {
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
