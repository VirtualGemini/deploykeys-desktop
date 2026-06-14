//! Root component: owns screen state, bootstraps the persisted session, and
//! drives Personal Access Token sign-in.

use crate::api;
use crate::i18n::{self, t, Locale};
use crate::icons::{Icon, IconName};
use crate::page_size::{self, DEFAULT_PAGE_SIZE};
use crate::progress::ProgressHandle;
use crate::screens::repos::Repos;
use crate::screens::signin::SignIn;
use crate::tauri;
use crate::theme::{self, Theme};
use leptos::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, PartialEq)]
enum Screen {
    Main,
    SignIn,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppSection {
    Repos,
    Connect,
    Keys,
}

const APP_SECTIONS: &[AppSection] = &[AppSection::Repos, AppSection::Connect, AppSection::Keys];

impl AppSection {
    const fn label_key(self) -> &'static str {
        match self {
            AppSection::Repos => "nav.repos",
            AppSection::Connect => "nav.connect",
            AppSection::Keys => "nav.keys",
        }
    }

    const fn icon(self) -> IconName {
        match self {
            AppSection::Repos => IconName::Folder,
            AppSection::Connect => IconName::Server,
            AppSection::Keys => IconName::Key,
        }
    }
}

const SIDEBAR_AUTO_COLLAPSE_WIDTH: f64 = 840.0;
const GITHUB_REPOSITORY_URL: &str = match option_env!("DEPLOYKEYS_GITHUB_REPOSITORY_URL") {
    Some(url) => url,
    None => "https://github.com/VirtualGemini/deploykeys-desktop",
};
const FEEDBACK_URL: &str = match option_env!("DEPLOYKEYS_FEEDBACK_URL") {
    Some(url) => url,
    None => "https://github.com/VirtualGemini/deploykeys-desktop/issues/new",
};
const SUPPORT_URL: &str = match option_env!("DEPLOYKEYS_SUPPORT_URL") {
    Some(url) => url,
    None => "https://github.com/VirtualGemini/deploykeys-desktop/discussions",
};

/// A command-palette entry before filtering: (i18n key, icon, action).
type PaletteCommand = (&'static str, IconName, Box<dyn Fn()>);
/// A filtered palette entry ready to render: (display label, optional icon, action).
type FilteredCommand = (String, Option<IconName>, Box<dyn Fn()>);

#[component]
pub fn App() -> impl IntoView {
    // Provide the reactive locale and theme at the root. `provide_context` must
    // run inside a reactive owner, so they live here rather than in `main`. The
    // startup locale guess comes from the webview language; the persisted
    // preference (if any) overrides it once the session bootstrap below runs.
    // Theme defaults to System (follows the OS prefers-color-scheme).
    // Progress handle is also provided at root so the header bar can react to
    // backend progress events and simulated short operations.
    i18n::provide_locale(detect_locale());
    theme::provide_theme(Theme::System);
    page_size::provide_page_size(DEFAULT_PAGE_SIZE);

    let screen = RwSignal::new(Screen::Main);
    let signing_in = RwSignal::new(false);
    let pending_count = RwSignal::new(0_usize);
    let progress = ProgressHandle::new();
    progress.provide();
    let progress_for_listener = progress;
    let error = RwSignal::new(None::<String>);
    let account = RwSignal::new(None::<api::Account>);
    let current_section = RwSignal::new(AppSection::Repos);
    // When true, the page dims/blurs and the sidebar sign-in button takes on
    // its hover emphasis (a 3s gentle hint shown when a signed-out user clicks Sync).
    let prompt_sign_in = RwSignal::new(false);

    // Start listening to backend progress events as soon as the app mounts.
    // Events carry `operation` and `percent`; we keep them in the shared handle.
    spawn_local(async move {
        let _ = tauri::listen_progress(move |ev| {
            progress_for_listener.on_real_progress(ev.operation, ev.percent);
        });
    });

    // Sidebar state: lifted to App level so it persists across screen changes
    let manual_sidebar_collapsed = RwSignal::new(None::<bool>);
    let auto_sidebar_collapsed = RwSignal::new(should_auto_collapse_sidebar());
    let sidebar_collapsed = Signal::derive(move || {
        manual_sidebar_collapsed
            .get()
            .unwrap_or_else(|| auto_sidebar_collapsed.get())
    });

    let resize_handle = window_event_listener(ev::resize, move |_| {
        let next_auto_collapsed = should_auto_collapse_sidebar();
        if next_auto_collapsed != auto_sidebar_collapsed.get_untracked() {
            manual_sidebar_collapsed.set(None);
        }
        auto_sidebar_collapsed.set(next_auto_collapsed);
    });
    on_cleanup(move || resize_handle.remove());

    let sidebar_toggle_callback = Callback::new(move |_| {
        manual_sidebar_collapsed.set(Some(!sidebar_collapsed.get_untracked()));
    });

    // Bootstrap: load persisted language + session. The app opens directly on
    // the main screen; a found session just populates the account (showing the
    // signed-in identity), it no longer gates which screen is shown.
    spawn_local(async move {
        let sim = progress.begin_simulated();
        if let Ok(Some(code)) = api::get_language().await {
            i18n::locale().set(Locale::from_code(&code));
        }
        if let Ok(Some(size)) = api::get_page_size().await {
            if let Some(valid) = page_size::validate_page_size(size) {
                page_size::page_size().set(valid);
            }
        }
        if let Ok(Some(acct)) = api::get_session().await {
            account.set(Some(acct));
        }
        progress.end_simulated(&sim);
    });

    // Open the token-paste sign-in screen.
    let start_auth = move |_| {
        error.set(None);
        screen.set(Screen::SignIn);
    };

    // Submit a pasted Personal Access Token: validate + persist on the backend.
    let submit_token = move |token: String| {
        if signing_in.get_untracked() || token.trim().is_empty() {
            return;
        }
        signing_in.set(true);
        let _sim = progress.begin_simulated();
        error.set(None);
        spawn_local(async move {
            match api::sign_in_with_token(token.trim()).await {
                Ok(acct) => {
                    account.set(Some(acct));
                    screen.set(Screen::Main);
                }
                Err(e) => error.set(Some(e)),
            }
            signing_in.set(false);
            // The real progress stream from the backend ends at 100; no need to
            // keep an extra simulated entry alive.
        });
    };

    let cancel_auth = move |_| {
        screen.set(Screen::Main);
        error.set(None);
        signing_in.set(false);
    };

    let open_url = move |url: String| {
        let sim = progress.begin_simulated();
        spawn_local(async move {
            let _ = api::open_url(&url).await;
            progress.end_simulated(&sim);
        });
    };

    // Signed-out Sync: visually nudge the sidebar sign-in button for 3s rather
    // than erroring. Re-arming while already showing just resets nothing (no-op).
    let hint_sign_in = move |_| {
        if prompt_sign_in.get_untracked() {
            return;
        }
        prompt_sign_in.set(true);
        set_timeout(
            move || prompt_sign_in.set(false),
            std::time::Duration::from_secs(3),
        );
    };

    view! {
        {move || match screen.get() {
            Screen::Main => view! {
                <Main
                    account=account
                    signing_in=signing_in
                    pending_count=pending_count
                    progress=progress
                    on_sign_in=Callback::new(start_auth)
                    on_sign_in_hint=Callback::new(hint_sign_in)
                    prompt_sign_in=prompt_sign_in
                    sidebar_collapsed=sidebar_collapsed
                    sidebar_toggle=sidebar_toggle_callback
                    current_section=current_section
                />
            }.into_view(),
            Screen::SignIn => view! {
                <div class="relative min-h-screen bg-bg">
                    <div class="absolute inset-x-0 top-0 z-50">
                        <HeaderLoading progress=progress />
                    </div>
                    <SignIn
                        signing_in=signing_in
                        error=error
                        on_submit=Callback::new(submit_token)
                        on_open=Callback::new(open_url)
                        on_cancel=Callback::new(cancel_auth)
                    />
                </div>
            }.into_view(),
        }}
    }
}

#[component]
fn HeaderLoading(progress: ProgressHandle) -> impl IntoView {
    let width = progress.bar_width();
    let opaque = progress.bar_opaque();
    let visible = progress.bar_visible();
    view! {
        <div class="relative shrink-0 h-px bg-border overflow-hidden" aria-hidden="true">
            <Show when=move || visible.get()>
                <div
                    class="header-progress-bar absolute inset-y-0 left-0 bg-primary"
                    style:opacity=move || if opaque.get() { "1" } else { "0" }
                    style:width=move || format!("{}%", width.get())
                ></div>
            </Show>
        </div>
    }
}

/// The main app screen (repos / connect / keys), with a top nav. The
/// top-right corner shows the signed-in identity + sign out when authenticated,
/// or a "sign in with GitHub" button that opens token sign-in otherwise.
#[component]
fn Main(
    account: RwSignal<Option<api::Account>>,
    #[prop(into)] signing_in: Signal<bool>,
    #[allow(unused_variables)] pending_count: RwSignal<usize>,
    progress: ProgressHandle,
    on_sign_in: Callback<()>,
    on_sign_in_hint: Callback<()>,
    #[prop(into)] prompt_sign_in: Signal<bool>,
    #[prop(into)] sidebar_collapsed: Signal<bool>,
    sidebar_toggle: Callback<()>,
    current_section: RwSignal<AppSection>,
) -> impl IntoView {
    let sign_out = move |_| {
        // Clear the persisted session on the backend, then drop local state.
        // Without the backend call the account row + keyring token survive, so
        // the session would reappear on the next launch.
        let sim = progress.begin_simulated();
        spawn_local(async move {
            let _ = api::sign_out().await;
            account.set(None);
            progress.end_simulated(&sim);
        });
    };

    // Command palette open state (toggled by ⌘K / Ctrl+K, or clicking the
    // header trigger). When open, a full-screen modal overlay with a centered
    // search box + filtered action list appears.
    let palette_open = RwSignal::new(false);

    let mouse_in_sidebar = RwSignal::new(false);

    let sign_out_confirm_open = RwSignal::new(false);

    view! {
        // Standard web-admin layout: a top title bar, then a body row split into
        // a left sidebar (section nav) and a right content area.
        //   ┌──────────────────────────────────────────┐
        //   │ header (drag region, traffic lights)       │
        //   ├───────────┬────────────────────────────────┤
        //   │  sidebar  │  content                        │
        //   └───────────┴────────────────────────────────┘
        // `h-screen` + `overflow-hidden` pins the chrome; only the content
        // column scrolls.
        <div class="h-screen w-full bg-bg text-content flex flex-col overflow-hidden">
            // macOS Overlay title bar: a full-width dark bar across the top edge.
            // The window's traffic-light buttons float over its left side, so
            // `pl-20` clears them. The whole bar is the window drag handle
            // (`data-tauri-drag-region`); passive children get `pointer-events-none`
            // so their clicks fall through to the drag region (the bar moves the
            // window instead of selecting text), while real buttons keep pointer
            // events and stay clickable.
            <header
                data-tauri-drag-region=""
                class="flex items-center shrink-0 h-14 pl-20 pr-4 gap-3 bg-surface select-none"
            >
                // Brand: mark + product name, right next to the traffic lights.
                <div class="flex items-center gap-2.5 pointer-events-none">
                    <div class="flex items-center justify-center size-9 rounded-lg bg-primary text-on-primary shrink-0">
                        <Icon name=IconName::Brand class="size-5" />
                    </div>
                    <span class="text-lg font-semibold leading-none text-content whitespace-nowrap">{move || t("app.brand")}</span>
                </div>

                // Flexible draggable gap. It carries `data-tauri-drag-region`
                // itself: a child without the attribute (and without
                // pointer-events-none) would otherwise swallow the drag and
                // fall back to text selection instead of moving the window.
                <div data-tauri-drag-region="" class="flex-1 self-stretch"></div>

                // Right: search sits slightly apart from compact utility buttons.
                <div class="flex items-center gap-3">
                    <CommandPaletteTrigger on_open=Callback::new(move |_| palette_open.set(true)) />
                    <div class="flex items-center gap-1">
                        <LanguageToggle progress=progress pending_count=pending_count />
                        <ThemeToggle />
                        <QuickRoutesMenu current_section=current_section pending_count=pending_count progress=progress />
                        <PlaceholderSettingsButton />
                    </div>
                </div>
            </header>

            <HeaderLoading progress=progress />

            // Command palette modal (shown when palette_open is true).
            <CommandPalette open=palette_open pending_count=pending_count progress=progress />

            // Body: sidebar (left) + content (right).
            <div class="flex-1 flex min-h-0">
                // Left sidebar: vertical section nav at the top, account controls
                // (sign in / signed-in identity + sign out) pinned to the bottom.
                <aside class=move || {
                    if sidebar_collapsed.get() {
                        "shrink-0 w-[80px] bg-surface flex flex-col overflow-y-auto overflow-x-hidden transition-[width] duration-300 [transition-timing-function:cubic-bezier(0.22,1,0.36,1)]"
                    } else {
                        "shrink-0 w-[210px] bg-surface flex flex-col overflow-y-auto overflow-x-hidden transition-[width] duration-300 [transition-timing-function:cubic-bezier(0.22,1,0.36,1)]"
                    }
                }
                    on:mouseenter=move |_| mouse_in_sidebar.set(true)
                    on:mouseleave=move |_| mouse_in_sidebar.set(false)
                >
                    <nav class=move || {
                        let base = "flex flex-col gap-1 py-3 pl-2 pr-1 transition-all duration-700";
                        if prompt_sign_in.get() {
                            format!("{base} sign-in-hint-soften pointer-events-none")
                        } else {
                            base.to_string()
                        }
                    }>
                        {APP_SECTIONS.iter().copied().map(|section| {
                            view! {
                                <NavItem
                                    icon=section.icon()
                                    label=move || t(section.label_key())
                                    active=Signal::derive(move || current_section.get() == section)
                                    collapsed=sidebar_collapsed
                                    on_select=Callback::new(move |_| current_section.set(section))
                                />
                            }
                        }).collect_view()}
                    </nav>

                    // Spacer pushes the account block to the bottom.
                    <div class="flex-1"></div>

                    // Account/Sign in block: bottom of the sidebar, above a divider.
                    <div class="shrink-0 py-3 pl-2 pr-1 border-t border-border">
                        {move || match account.get() {
                            Some(acct) => {
                                // Signed in: show avatar + username, click to show dropdown
                                let login = format!("@{}", acct.login);
                                let dropdown_open = RwSignal::new(false);
                                let username_class = move || {
                                    if sidebar_collapsed.get() {
                                        "min-w-0 max-w-0 opacity-0 overflow-hidden whitespace-nowrap text-sm text-content transition-[max-width,opacity] duration-150"
                                    } else {
                                        "min-w-0 max-w-[9rem] opacity-100 truncate text-sm text-content transition-[max-width,opacity] duration-200"
                                    }
                                };
                                view! {
                                    <div class="pr-[7px] relative">
                                        <button
                                            type="button"
                                            title=login.clone()
                                            class="w-full flex items-center gap-2.5 h-10 pl-[16px] pr-3 rounded-lg hover:bg-bg"
                                            on:click=move |_| dropdown_open.update(|o| *o = !*o)
                                        >
                                            <AccountAvatar account=acct.clone() />
                                            <span class=username_class>{login}</span>
                                        </button>

                                        {move || if dropdown_open.get() && sidebar_collapsed.get() {
                                            view! {
                                                // Click-outside backdrop
                                                <div class="fixed inset-0 z-40" on:click=move |_| dropdown_open.set(false)></div>

                                                // Collapsed: icon-only popover
                                                <div class="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 z-50 p-1 bg-surface border border-border rounded-xl shadow-xl">
                                                    <button
                                                        type="button"
                                                        title=move || t("common.sign_out")
                                                        class="flex items-center justify-center size-8 rounded-lg text-content hover:bg-bg focus:outline-none"
                                                        on:click=move |_| {
                                                            dropdown_open.set(false);
                                                            sign_out_confirm_open.set(true);
                                                        }
                                                    >
                                                        <Icon name=IconName::SignOut class="size-4" />
                                                    </button>
                                                </div>
                                            }.into_view()
                                        } else if dropdown_open.get() && !sidebar_collapsed.get() {
                                            view! {
                                                // Click-outside backdrop
                                                <div class="fixed inset-0 z-40" on:click=move |_| dropdown_open.set(false)></div>

                                                // Expanded: dropdown menu
                                                <div class="absolute bottom-full left-2 mb-2 z-50 w-[calc(100%-16px)] p-1 bg-surface border border-border rounded-xl shadow-xl">
                                                    <button
                                                        type="button"
                                                        class="w-full flex items-center gap-x-3 py-2 px-2.5 rounded-lg text-sm text-content hover:bg-bg focus:outline-none"
                                                        on:click=move |_| {
                                                            dropdown_open.set(false);
                                                            sign_out_confirm_open.set(true);
                                                        }
                                                    >
                                                        <Icon name=IconName::SignOut class="size-4" />
                                                        <span class="grow text-left">{move || t("common.sign_out")}</span>
                                                    </button>
                                                </div>
                                            }.into_view()
                                        } else {
                                            ().into_view()
                                        }}
                                    </div>
                                }.into_view()
                            },
                            None => {
                                // Not signed in: show GitHub icon when collapsed, ghost button when expanded
                                // Use two separate components with fade in/out for smooth transition
                                view! {
                                    <div class="pr-[7px] relative rounded-lg">
                                        // Collapsed: icon only button
                                        <button
                                            type="button"
                                            title=move || t("welcome.sign_in")
                                            class=move || {
                                                let hinted = if prompt_sign_in.get() { " sign-in-button-hover" } else { "" };
                                                if sidebar_collapsed.get() {
                                                    if signing_in.get() {
                                                        format!("absolute inset-0 flex items-center justify-center h-10 rounded-lg text-content hover:text-primary focus:outline-none transition-all duration-700 ease-out delay-150 opacity-50 pointer-events-none{hinted}")
                                                    } else {
                                                        format!("absolute inset-0 flex items-center justify-center h-10 rounded-lg text-content hover:text-primary focus:outline-none transition-all duration-700 ease-out delay-150 opacity-100{hinted}")
                                                    }
                                                } else {
                                                    format!("absolute inset-0 flex items-center justify-center h-10 rounded-lg text-content hover:text-primary focus:outline-none transition-all duration-200 ease-in opacity-0 pointer-events-none{hinted}")
                                                }
                                            }
                                            prop:disabled=signing_in.get()
                                            on:click=move |_| on_sign_in.call(())
                                        >
                                            <Icon name=IconName::Github class="size-7" />
                                        </button>

                                        // Expanded: ghost button with text
                                        <button
                                            type="button"
                                            title=move || t("welcome.sign_in")
                                            class=move || {
                                                let hinted = if prompt_sign_in.get() { " sign-in-button-hover border-transparent" } else { "" };
                                                if sidebar_collapsed.get() {
                                                    format!("w-full flex items-center justify-center gap-x-2 h-10 px-4 text-sm font-medium rounded-lg border border-border text-content hover:text-primary focus:outline-none transition-all duration-200 ease-in opacity-0 pointer-events-none{hinted}")
                                                } else {
                                                    if signing_in.get() {
                                                        format!("w-full flex items-center justify-center gap-x-2 h-10 px-4 text-sm font-medium rounded-lg border border-border text-content hover:text-primary focus:outline-none transition-all duration-700 ease-out delay-150 opacity-50 pointer-events-none{hinted}")
                                                    } else {
                                                        format!("w-full flex items-center justify-center gap-x-2 h-10 px-4 text-sm font-medium rounded-lg border border-border text-content hover:text-primary focus:outline-none transition-all duration-700 ease-out delay-150 opacity-100{hinted}")
                                                    }
                                                }
                                            }
                                            prop:disabled=signing_in.get()
                                            on:click=move |_| on_sign_in.call(())
                                        >
                                            <Icon name=IconName::Github class="size-4" />
                                            <span class="whitespace-nowrap">{move || t("welcome.sign_in")}</span>
                                        </button>
                                    </div>
                                }.into_view()
                            },
                        }}
                    </div>
                </aside>

                <SidebarDivider collapsed=sidebar_collapsed on_toggle=sidebar_toggle mouse_in_sidebar=mouse_in_sidebar />

                // Right content area: the Repos list (self-manages its data,
                // gated on the session). Blurs while the sign-in hint is active.
                <main class=move || {
                    // No bottom padding: screens are `h-full`, so a screen's own
                    // bottom bar (e.g. the repos pagination) can sit flush with the
                    // viewport bottom and line up with the sidebar's account divider.
                    let base = "flex-1 min-w-0 overflow-y-auto px-8 pt-8 transition-all duration-700";
                    if prompt_sign_in.get() {
                        format!("{base} sign-in-hint-soften pointer-events-none")
                    } else {
                        base.to_string()
                    }
                }>
                    <div class="max-w-4xl mx-auto h-full">
                        {move || match current_section.get() {
                            AppSection::Repos => view! {
                                <Repos account=account pending_count=pending_count on_sign_in_hint=on_sign_in_hint />
                            }.into_view(),
                            section => view! {
                                <PlaceholderSection section=section />
                            }.into_view(),
                        }}
                    </div>
                </main>
            </div>

            // Sign out confirmation dialog
            <div class=move || {
                if sign_out_confirm_open.get() {
                    "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-100 transition-opacity duration-300"
                } else {
                    "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-0 pointer-events-none transition-opacity duration-300"
                }
            }>
                <div class="w-full max-w-md bg-surface border border-border rounded-xl shadow-2xl overflow-hidden flex flex-col scale-100 transition-transform duration-300">
                    <div class="px-6 py-5">
                        <h2 class="text-base font-semibold text-content">{move || t("sign_out.confirm_title")}</h2>
                        <p class="mt-2 text-sm text-muted">{move || t("sign_out.confirm_message")}</p>
                    </div>
                    <div class="flex justify-end gap-2 px-6 py-4 border-t border-border">
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-bg text-content hover:text-primary focus:outline-none"
                            on:click=move |_| sign_out_confirm_open.set(false)
                        >
                            {move || t("common.cancel")}
                        </button>
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none"
                            on:click=move |_| {
                                sign_out_confirm_open.set(false);
                                sign_out(());
                            }
                        >
                            {move || t("common.confirm")}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn NavItem(
    icon: IconName,
    #[prop(into)] label: Signal<&'static str>,
    #[prop(into)] active: Signal<bool>,
    #[prop(into)] collapsed: Signal<bool>,
    on_select: Callback<()>,
) -> impl IntoView {
    // Sidebar item: full-width, left-aligned row with icon + label.
    let class = move || {
        if active.get() {
            "w-full flex items-center gap-2.5 h-10 pl-[24px] pr-3 overflow-hidden text-sm font-medium rounded-lg bg-primary-soft text-primary hover:bg-primary-soft/80"
        } else {
            "w-full flex items-center gap-2.5 h-10 pl-[24px] pr-3 overflow-hidden text-sm font-medium rounded-lg text-muted hover:bg-bg hover:text-content"
        }
    };
    let wrapper_class = "pr-[7px]";
    let label_class = move || {
        if collapsed.get() {
            "min-w-0 max-w-0 opacity-0 overflow-hidden whitespace-nowrap transition-[max-width,opacity] duration-150"
        } else {
            "min-w-0 max-w-[9rem] opacity-100 truncate transition-[max-width,opacity] duration-200"
        }
    };
    view! {
        <div class=wrapper_class>
            <button
                type="button"
                title=move || label.get()
                class=class
                on:click=move |_| on_select.call(())
            >
                <Icon name=icon class="size-4" />
                <span class=label_class>{move || label.get()}</span>
            </button>
        </div>
    }
}

#[component]
fn PlaceholderSection(section: AppSection) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-5 h-full">
            <h1 class="text-2xl font-semibold text-content">{move || t(section.label_key())}</h1>
            <div class="flex flex-1 items-center justify-center py-16 text-center">
                <p class="text-sm text-muted">{move || t("screen.placeholder_phase4")}</p>
            </div>
        </div>
    }
}

#[component]
fn AccountAvatar(account: api::Account) -> impl IntoView {
    let title = format!("@{}", account.login);
    match account.avatar_url {
        Some(url) => view! {
            <img src=url alt="" title=title class="size-8 shrink-0 rounded-full bg-primary-soft" />
        }
        .into_view(),
        None => {
            let initial = account.login.chars().next().unwrap_or('?').to_string();
            view! {
                <div title=title class="flex items-center justify-center size-8 shrink-0 rounded-full bg-primary-soft text-primary text-sm font-semibold uppercase">
                    {initial}
                </div>
            }.into_view()
        }
    }
}

#[component]
fn SidebarDivider(#[prop(into)] collapsed: Signal<bool>, on_toggle: Callback<()>, #[prop(into)] mouse_in_sidebar: Signal<bool>) -> impl IntoView {
    let hovering = RwSignal::new(false);
    let button_class = move || {
        if hovering.get() || mouse_in_sidebar.get() {
            "absolute top-8 left-[-13px] z-30 flex items-center justify-center size-8 text-content opacity-100 hover:text-primary focus:opacity-100 focus:outline-none transition-opacity duration-150"
        } else {
            "absolute top-8 left-[-13px] z-30 flex items-center justify-center size-8 text-muted opacity-0 hover:text-primary hover:opacity-100 focus:opacity-100 focus:outline-none transition-opacity duration-150"
        }
    };

    let handle_click = move |_| {
        hovering.set(false);
        on_toggle.call(());
    };

    view! {
        <div
            class="relative z-20 shrink-0 self-stretch w-px overflow-visible"
            on:mouseenter=move |_| hovering.set(true)
            on:mouseleave=move |_| hovering.set(false)
        >
            <div class="absolute inset-y-0 left-0 w-px bg-border pointer-events-none"></div>
            <button
                type="button"
                title=move || if collapsed.get() { t("sidebar.expand") } else { t("sidebar.collapse") }
                aria-label=move || if collapsed.get() { t("sidebar.expand") } else { t("sidebar.collapse") }
                class=button_class
                on:mouseenter=move |_| hovering.set(true)
                on:click=handle_click
            >
                <SidebarToggleIcon collapsed=collapsed />
            </button>
        </div>
    }
}

#[component]
fn SidebarToggleIcon(#[prop(into)] collapsed: Signal<bool>) -> impl IntoView {
    view! {
        {move || {
            let icon = if collapsed.get() {
                IconName::SidebarToggleFilled
            } else {
                IconName::SidebarToggle
            };
            view! { <Icon name=icon class="size-6" /> }
        }}
    }
}

/// Header command-palette trigger. Minimal velox-style: just the hint text and
/// shortcut badge, no leading icon bloat. Clicking it or Cmd/Ctrl+K opens the
/// palette modal.
#[component]
fn CommandPaletteTrigger(on_open: Callback<()>) -> impl IntoView {
    // Global Cmd/Ctrl+K handler. Fires even when the trigger is hidden (mobile).
    let handle = window_event_listener(ev::keydown, move |ev| {
        if (ev.meta_key() || ev.ctrl_key()) && ev.key().eq_ignore_ascii_case("k") {
            ev.prevent_default();
            on_open.call(());
        }
    });
    on_cleanup(move || handle.remove());

    view! {
        <button
            type="button"
            class="hidden sm:inline-flex items-center justify-between gap-x-3 w-56 h-9 px-2.5 text-sm rounded-lg bg-bg border border-border text-muted hover:bg-surface focus:outline-none"
            on:click=move |_| on_open.call(())
        >
            <span class="text-xs">{move || t("palette.trigger")}</span>
            <span class="flex items-center gap-x-0.5 h-5 px-1.5 border border-border rounded text-xs text-muted">
                <span>"⌘"</span>
                <span class="uppercase">"K"</span>
            </span>
        </button>
    }
}

/// Command palette modal: full-screen dialog with search + keyboard navigation.
/// Velox-style: clean UI, arrow key navigation, history persistence, footer hints.
/// Commands: nav items (repos/connect/keys), toggle theme/language. Real
/// actions (create key, bind target) land once those screens exist.
#[component]
fn CommandPalette(
    open: RwSignal<bool>,
    #[allow(unused_variables)] pending_count: RwSignal<usize>,
    progress: ProgressHandle,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let input_ref = NodeRef::<html::Input>::new();
    let selected = RwSignal::new(0_usize); // Keyboard-selected index

    // Search history: persisted to localStorage, max 10 items.
    let history = create_rw_signal({
        if let Some(win) = web_sys::window() {
            if let Ok(Some(storage)) = win.local_storage() {
                if let Ok(Some(json)) = storage.get_item("command_palette_history") {
                    serde_json::from_str::<Vec<String>>(&json).unwrap_or_default()
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    });

    let save_history = move |key: &str| {
        let mut h = history.get_untracked();
        h.retain(|k| k != key);
        h.insert(0, key.to_string());
        h.truncate(10);
        history.set(h.clone());
        if let Some(win) = web_sys::window() {
            if let Ok(Some(storage)) = win.local_storage() {
                if let Ok(json) = serde_json::to_string(&h) {
                    let _ = storage.set_item("command_palette_history", &json);
                }
            }
        }
    };

    let delete_history = move |index: usize| {
        let mut h = history.get_untracked();
        if index < h.len() {
            h.remove(index);
            history.set(h.clone());
            if let Some(win) = web_sys::window() {
                if let Ok(Some(storage)) = win.local_storage() {
                    if let Ok(json) = serde_json::to_string(&h) {
                        let _ = storage.set_item("command_palette_history", &json);
                    }
                }
            }
        }
    };

    // Command definitions: (i18n key, icon, action). IconName resolves to
    // external SVG assets under assets/images/svg/icons.
    let all_commands = move || -> Vec<PaletteCommand> {
        let locale = i18n::locale();
        let theme_signal = theme::theme();
        vec![
            (
                "nav.repos",
                IconName::Folder,
                Box::new(|| {}) as Box<dyn Fn()>,
            ),
            ("nav.connect", IconName::Server, Box::new(|| {})),
            ("nav.keys", IconName::Key, Box::new(|| {})),
            (
                "palette.toggle_theme",
                IconName::Moon,
                Box::new(move || {
                    let next = match theme_signal.get_untracked() {
                        Theme::System => Theme::Light,
                        Theme::Light => Theme::Dark,
                        Theme::Dark => Theme::System,
                    };
                    theme_signal.set(next);
                    open.set(false);
                }),
            ),
            (
                "palette.change_language",
                IconName::Globe,
                Box::new(move || {
                    let next = match locale.get_untracked() {
                        Locale::En => Locale::Zh,
                        Locale::Zh => Locale::En,
                    };
                    locale.set(next);
                    let code = next.code();
                    let sim = progress.begin_simulated();
                    spawn_local(async move {
                        let _ = api::set_language(code).await;
                        progress.end_simulated(&sim);
                    });
                    open.set(false);
                }),
            ),
        ]
    };

    // Filtered commands: if query empty, show history; else substring match.
    let filtered = move || -> Vec<FilteredCommand> {
        let q = query.get().trim().to_lowercase();
        if q.is_empty() {
            // Show history (no icon, action re-runs search).
            history
                .get()
                .into_iter()
                .take(10)
                .map(|key| {
                    let k = key.clone();
                    (
                        key,
                        None,
                        Box::new(move || {
                            query.set(k.clone());
                        }) as Box<dyn Fn()>,
                    )
                })
                .collect()
        } else {
            all_commands()
                .into_iter()
                .filter(|(key, _, _)| t(key).to_lowercase().contains(&q))
                .map(|(key, icon, action)| (t(key).to_string(), Some(icon), action))
                .collect()
        }
    };

    // Open: reset query, selected, focus input.
    create_effect(move |_| {
        if open.get() {
            query.set(String::new());
            selected.set(0);
            request_animation_frame(move || {
                if let Some(input) = input_ref.get() {
                    let _ = input.focus();
                }
            });
        }
    });

    // Keyboard: ESC close, ArrowUp/Down navigate, Enter execute.
    let handle = window_event_listener(ev::keydown, move |ev| {
        if !open.get_untracked() {
            return;
        }
        let key = ev.key();
        if key.eq_ignore_ascii_case("escape") {
            ev.prevent_default();
            open.set(false);
        } else if key == "ArrowDown" {
            ev.prevent_default();
            let items = filtered();
            if !items.is_empty() {
                selected.update(|s| *s = (*s + 1).min(items.len() - 1));
            }
        } else if key == "ArrowUp" {
            ev.prevent_default();
            selected.update(|s| *s = s.saturating_sub(1));
        } else if key == "Enter" {
            ev.prevent_default();
            let items = filtered();
            let idx = selected.get_untracked();
            if idx < items.len() {
                let (label, _icon, action) = &items[idx];
                save_history(label);
                action();
            }
        }
    });
    on_cleanup(move || handle.remove());

    view! {
        <Show when=move || open.get()>
            <div
                class="fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-start justify-center pt-[15vh] px-4"
                on:click=move |_| open.set(false)
            >
                <div
                    class="w-full max-w-xl bg-surface border border-border rounded-xl shadow-2xl overflow-hidden flex flex-col"
                    on:click=|ev| ev.stop_propagation()
                >
                    // Search header
                    <div class="flex items-center gap-3 px-4 py-3 border-b border-border">
                        <input
                            node_ref=input_ref
                            type="text"
                            prop:value=move || query.get()
                            on:input=move |ev| {
                                query.set(event_target_value(&ev));
                                selected.set(0);
                            }
                            placeholder=move || t("palette.placeholder")
                            class="flex-1 text-base bg-transparent text-content placeholder:text-muted focus:outline-none"
                        />
                    </div>

                    // Results list (scrollable)
                    <div class="max-h-96 overflow-y-auto py-2 px-2">
                        {move || {
                            let items = filtered();
                            let q = query.get();
                            if items.is_empty() && !q.is_empty() {
                                view! {
                                    <div class="py-8 text-center text-sm text-muted">
                                        {move || t("palette.no_results")}
                                    </div>
                                }.into_view()
                            } else if items.is_empty() {
                                view! {
                                    <div class="py-8 text-center text-sm text-muted">
                                        {move || t("palette.empty_history")}
                                    </div>
                                }.into_view()
                            } else {
                                let is_history = q.is_empty();
                                items.into_iter().enumerate().map(|(idx, (label, icon_opt, action))| {
                                    let active = move || selected.get() == idx;
                                    let label_clone = label.clone();
                                    view! {
                                        <div
                                            class="flex items-center gap-3 py-2.5 px-3 rounded-lg text-sm cursor-pointer"
                                            class:bg-primary=active
                                            class:text-on-primary=active
                                            class:text-content=move || !active()
                                            class:hover:bg-bg=move || !active()
                                            on:click=move |_| {
                                                save_history(&label_clone);
                                                action();
                                            }
                                            on:mouseenter=move |_| selected.set(idx)
                                        >
                                            {move || match icon_opt {
                                                Some(icon) => view! { <Icon name=icon class="size-5" /> }.into_view(),
                                                None => view! { <div class="shrink-0 size-5"></div> }.into_view(),
                                            }}
                                            <span class="flex-1">{label.clone()}</span>
                                            <Show when=move || is_history && active()>
                                                <button
                                                    type="button"
                                                    class="shrink-0 size-5 flex items-center justify-center rounded text-on-primary/70 hover:text-on-primary"
                                                    on:click=move |ev| {
                                                        ev.stop_propagation();
                                                        delete_history(idx);
                                                    }
                                                >
                                                    <Icon name=IconName::Close class="size-3" />
                                                </button>
                                            </Show>
                                        </div>
                                    }
                                }).collect_view()
                            }
                        }}
                    </div>

                    // Footer hints
                    <div class="flex items-center gap-4 px-4 py-3 border-t border-border text-xs text-muted">
                        <div class="flex items-center gap-1.5">
                            <kbd class="px-1.5 py-0.5 bg-bg border border-border rounded text-[10px]">"↑"</kbd>
                            <kbd class="px-1.5 py-0.5 bg-bg border border-border rounded text-[10px]">"↓"</kbd>
                            <span>{move || t("palette.navigate")}</span>
                        </div>
                        <div class="flex items-center gap-1.5">
                            <kbd class="px-1.5 py-0.5 bg-bg border border-border rounded text-[10px]">"↵"</kbd>
                            <span>{move || t("palette.select")}</span>
                        </div>
                        <div class="flex items-center gap-1.5">
                            <kbd class="px-2 py-0.5 bg-bg border border-border rounded text-[10px]">"ESC"</kbd>
                            <span>{move || t("palette.close")}</span>
                        </div>
                    </div>
                </div>
            </div>
        </Show>
    }
}

/// Header icon button shape, shared by the language and theme toggles. Follows
/// Preline's icon-button sizing (square, centered, rounded, hover surface).
#[component]
fn IconButton(
    #[prop(into)] title: Signal<&'static str>,
    on_click: Callback<()>,
    children: Children,
) -> impl IntoView {
    view! {
        <button
            type="button"
            title=move || title.get()
            class="shrink-0 flex justify-center items-center size-8 rounded-lg text-muted hover:bg-bg hover:text-content focus:outline-none"
            on:click=move |_| on_click.call(())
        >
            {children()}
        </button>
    }
}

#[derive(Clone, Copy)]
struct QuickRouteLink {
    label_key: &'static str,
    url: &'static str,
}

const QUICK_ROUTE_LINKS: &[QuickRouteLink] = &[
    QuickRouteLink {
        label_key: "quick_routes.github_repository",
        url: GITHUB_REPOSITORY_URL,
    },
    QuickRouteLink {
        label_key: "quick_routes.feedback",
        url: FEEDBACK_URL,
    },
    QuickRouteLink {
        label_key: "quick_routes.support",
        url: SUPPORT_URL,
    },
];

/// Quick routes: app menu shortcuts on the left and external links on the
/// right. External URLs are build-time placeholders wired through env vars.
#[component]
fn QuickRoutesMenu(
    current_section: RwSignal<AppSection>,
    #[allow(unused_variables)] pending_count: RwSignal<usize>,
    progress: ProgressHandle,
) -> impl IntoView {
    let open = RwSignal::new(false);

    let open_route = move |url: &'static str| {
        open.set(false);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            let _ = api::open_url(url).await;
            progress.end_simulated(&sim);
        });
    };

    view! {
        <div class="relative">
            <IconButton
                title=move || t("quick_routes.title")
                on_click=Callback::new(move |_| open.update(|o| *o = !*o))
            >
                <Icon name=IconName::QuickRoutes class="size-4" />
            </IconButton>

            <Show when=move || open.get()>
                <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>

                <div class="absolute end-0 mt-2 z-50 w-[460px] overflow-hidden bg-surface border border-border rounded-xl shadow-xl">
                    <div class="grid grid-cols-[12rem_minmax(0,1fr)]">
                        <div class="p-1 border-r border-border bg-bg/60">
                            {APP_SECTIONS.iter().copied().map(|section| {
                                view! {
                                    <button
                                        type="button"
                                        class="w-full flex items-center gap-x-2 py-2 px-2.5 rounded-lg text-sm text-content hover:bg-surface focus:outline-none focus:bg-surface"
                                        on:click=move |_| {
                                            current_section.set(section);
                                            open.set(false);
                                        }
                                    >
                                        <Icon name=section.icon() class="size-4" />
                                        <span class="min-w-0 truncate text-left">{move || t(section.label_key())}</span>
                                    </button>
                                }
                            }).collect_view()}
                        </div>

                        <div class="p-1">
                            {QUICK_ROUTE_LINKS.iter().copied().map(|item| {
                                view! {
                                    <button
                                        type="button"
                                        title=item.url
                                        class="w-full flex items-center justify-between gap-x-3 py-2 px-2.5 rounded-lg text-sm text-content hover:bg-bg focus:outline-none focus:bg-bg"
                                        on:click=move |_| open_route(item.url)
                                    >
                                        <span class="min-w-0 truncate text-left">{move || t(item.label_key)}</span>
                                        <span class="shrink-0 text-xs text-muted">"↗"</span>
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn PlaceholderSettingsButton() -> impl IntoView {
    view! {
        <IconButton title=move || t("settings.placeholder") on_click=Callback::new(|_| {})>
            <Icon name=IconName::SettingsPlaceholder class="size-4" />
        </IconButton>
    }
}

/// Language picker: an icon button that opens a dropdown listing every
/// supported locale (driven by `Locale::ALL`, so adding languages needs no
/// markup changes here). Selecting one applies it and persists the choice to
/// the backend. Styled after Preline's dropdown (rounded, bordered, shadowed
/// panel with hover rows and a check on the active item). Open/close is driven
/// by a local signal — no Preline JS — and a transparent full-screen backdrop
/// catches click-outside to close.
#[component]
fn LanguageToggle(
    progress: ProgressHandle,
    #[allow(unused_variables)] pending_count: RwSignal<usize>,
) -> impl IntoView {
    let locale = i18n::locale();
    let open = RwSignal::new(false);

    let select = move |next: Locale| {
        locale.set(next);
        open.set(false);
        let code = next.code();
        let sim = progress.begin_simulated();
        spawn_local(async move {
            let _ = api::set_language(code).await;
            progress.end_simulated(&sim);
        });
    };

    view! {
        <div class="relative">
            <IconButton
                title=move || t("settings.language")
                on_click=Callback::new(move |_| open.update(|o| *o = !*o))
            >
                <Icon name=IconName::Globe class="size-4" />
            </IconButton>

            <Show when=move || open.get()>
                // Click-outside backdrop: a transparent full-screen layer behind
                // the menu that closes it when clicked.
                <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>

                // Dropdown panel (Preline dropdown spec).
                <div class="absolute end-0 mt-2 z-50 w-44 max-h-80 overflow-y-auto p-1 bg-surface border border-border rounded-xl shadow-xl">
                    {Locale::ALL.iter().copied().map(|loc| {
                        let active = move || locale.get() == loc;
                        view! {
                            <button
                                type="button"
                                class="w-full flex items-center gap-x-3 py-2 px-2.5 rounded-lg text-sm text-content hover:bg-bg focus:outline-none focus:bg-bg"
                                on:click=move |_| select(loc)
                            >
                                {move || {
                                    let class = if active() {
                                        "size-4 text-primary"
                                    } else {
                                        "size-4 text-primary opacity-0"
                                    };
                                    view! { <Icon name=IconName::Check class=class /> }
                                }}
                                <span class="grow text-left">{loc.native_name()}</span>
                            </button>
                        }
                    }).collect_view()}
                </div>
            </Show>
        </div>
    }
}

/// Theme toggle: cycles System → Light → Dark and shows an icon for the current
/// mode (auto / sun / moon).
#[component]
fn ThemeToggle() -> impl IntoView {
    let theme = theme::theme();
    let toggle = move |_| {
        let next = match theme.get_untracked() {
            Theme::System => Theme::Light,
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::System,
        };
        theme.set(next);
    };
    view! {
        <IconButton title=move || t("settings.theme") on_click=Callback::new(toggle)>
            {move || match theme.get() {
                Theme::Light => view! { <Icon name=IconName::Sun class="size-4" /> }.into_view(),
                Theme::Dark => view! { <Icon name=IconName::Moon class="size-4" /> }.into_view(),
                Theme::System => view! { <Icon name=IconName::Monitor class="size-4" /> }.into_view(),
            }}
        </IconButton>
    }
}

fn should_auto_collapse_sidebar() -> bool {
    web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|width| width.as_f64())
        .map(|width| width <= SIDEBAR_AUTO_COLLAPSE_WIDTH)
        .unwrap_or(false)
}

/// Read the webview's language (e.g. `zh-CN`) and map it to a supported locale.
/// The persisted preference, if any, overrides this during bootstrap.
fn detect_locale() -> Locale {
    web_sys::window()
        .and_then(|w| w.navigator().language())
        .map(|code| Locale::from_code(&code))
        .unwrap_or(Locale::En)
}
