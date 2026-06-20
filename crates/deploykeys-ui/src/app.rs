//! Root component: owns screen state, bootstraps the persisted session, and
//! drives Personal Access Token sign-in.

use crate::api;
use crate::connection;
use crate::i18n::{self, t, Locale};
use crate::icons::{Icon, IconName};
use crate::page_size::{self, DEFAULT_PAGE_SIZE};
use crate::progress::ProgressHandle;
use crate::screens::connect::Connect;
use crate::screens::keys::Keys;
use crate::screens::repos::Repos;
use crate::screens::signin::SignIn;
use crate::tauri;
use crate::theme::{self, Theme};
use crate::toast::{ToastHandle, ToastViewport};
use leptos::*;
use wasm_bindgen::JsCast;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum TutorialStep {
    SignIn,
    Sync,
    Connect,
    CreateKey,
    BindKey,
    CloneRepo,
    ConnectRepo,
    TestRepo,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    KeyStorage,
    ConfigFile,
}

const APP_SECTIONS: &[AppSection] = &[AppSection::Repos, AppSection::Connect, AppSection::Keys];
const TUTORIAL_STEPS: &[TutorialStep] = &[
    TutorialStep::SignIn,
    TutorialStep::Sync,
    TutorialStep::Connect,
    TutorialStep::CreateKey,
    TutorialStep::BindKey,
    TutorialStep::CloneRepo,
    TutorialStep::ConnectRepo,
    TutorialStep::TestRepo,
];
const TUTORIAL_STORAGE_KEY: &str = "deploykeys.beginner_tutorial_seen";

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

impl TutorialStep {
    fn index(self) -> usize {
        TUTORIAL_STEPS
            .iter()
            .position(|step| *step == self)
            .unwrap_or(0)
    }

    const fn section(self) -> AppSection {
        match self {
            TutorialStep::SignIn
            | TutorialStep::Sync
            | TutorialStep::BindKey
            | TutorialStep::CloneRepo
            | TutorialStep::ConnectRepo
            | TutorialStep::TestRepo => AppSection::Repos,
            TutorialStep::Connect => AppSection::Connect,
            TutorialStep::CreateKey => AppSection::Keys,
        }
    }

    const fn icon(self) -> IconName {
        match self {
            TutorialStep::SignIn => IconName::Github,
            TutorialStep::Sync | TutorialStep::CloneRepo => IconName::Download,
            TutorialStep::Connect | TutorialStep::ConnectRepo | TutorialStep::TestRepo => {
                IconName::Server
            }
            TutorialStep::CreateKey | TutorialStep::BindKey => IconName::Key,
        }
    }

    const fn title_key(self) -> &'static str {
        match self {
            TutorialStep::SignIn => "tutorial.signin.title",
            TutorialStep::Sync => "tutorial.sync.title",
            TutorialStep::Connect => "tutorial.connect.title",
            TutorialStep::CreateKey => "tutorial.create_key.title",
            TutorialStep::BindKey => "tutorial.bind_key.title",
            TutorialStep::CloneRepo => "tutorial.clone_repo.title",
            TutorialStep::ConnectRepo => "tutorial.connect_repo.title",
            TutorialStep::TestRepo => "tutorial.test_repo.title",
        }
    }

    const fn body_key(self) -> &'static str {
        match self {
            TutorialStep::SignIn => "tutorial.signin.body",
            TutorialStep::Sync => "tutorial.sync.body",
            TutorialStep::Connect => "tutorial.connect.body",
            TutorialStep::CreateKey => "tutorial.create_key.body",
            TutorialStep::BindKey => "tutorial.bind_key.body",
            TutorialStep::CloneRepo => "tutorial.clone_repo.body",
            TutorialStep::ConnectRepo => "tutorial.connect_repo.body",
            TutorialStep::TestRepo => "tutorial.test_repo.body",
        }
    }

    const fn target_selector(self) -> &'static str {
        match self {
            TutorialStep::SignIn => "[data-tutorial-target='sign-in']",
            TutorialStep::Sync => "[data-tutorial-target='sync-repos']",
            TutorialStep::Connect => "[data-tutorial-target='connect-environment']",
            TutorialStep::CreateKey => "[data-tutorial-target='create-key']",
            TutorialStep::BindKey => "[data-tutorial-target='bind-key']",
            TutorialStep::CloneRepo => "[data-tutorial-target='clone-repo']",
            TutorialStep::ConnectRepo => "[data-tutorial-target='connect-repo']",
            TutorialStep::TestRepo => "[data-tutorial-target='test-repo']",
        }
    }
}

#[derive(Clone, Copy)]
struct TutorialTargetRect {
    top: f64,
    left: f64,
    width: f64,
    height: f64,
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
    // Keep <html lang> in sync with the locale signal so every switch point
    // (bootstrap + the three entry points) updates it via one root effect
    // rather than repeating the call at each site.
    install_locale_sync_effect();
    theme::provide_theme(Theme::System);
    page_size::provide_page_size(DEFAULT_PAGE_SIZE);
    connection::provide_connection_state();
    let connection = connection::connection_state();

    let screen = RwSignal::new(Screen::Main);
    let signing_in = RwSignal::new(false);
    let pending_count = RwSignal::new(0_usize);
    let progress = ProgressHandle::new();
    progress.provide();
    let progress_for_listener = progress;
    let toast = ToastHandle::new();
    toast.provide();
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
        if let Ok(list) = api::list_connections().await {
            connection.set_connections(
                list.into_iter()
                    .map(connection::Connection::from_dto)
                    .collect(),
            );
        }
        if let Ok(Some(value)) = api::get_active_connection().await {
            connection.apply_persisted(value);
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
        let sim = progress.begin_simulated();
        error.set(None);
        spawn_local(async move {
            match api::sign_in_with_token(token.trim()).await {
                Ok(acct) => {
                    account.set(Some(acct));
                    screen.set(Screen::Main);
                    toast.success(t("signin.success"));
                }
                Err(e) => error.set(Some(e)),
            }
            signing_in.set(false);
            progress.end_simulated(&sim);
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
        <ToastViewport />
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
    // The active connection is a persisted preference. Re-read it from the
    // backend whenever the user changes sections, so a disconnect made on the
    // Connect page is honored everywhere across navigation rather than masked
    // by stale in-memory state. Initial hydration happens in the App bootstrap,
    // so the first (mount) run is skipped.
    let connection = connection::connection_state();
    create_effect(move |prev: Option<AppSection>| {
        let section = current_section.get();
        if prev.is_some() {
            spawn_local(async move {
                if let Ok(Some(value)) = api::get_active_connection().await {
                    connection.apply_persisted(value);
                }
            });
        }
        section
    });

    let sign_out = move |_| {
        // Clear the persisted session on the backend, then drop local state.
        // Without the backend call the account row + keyring token survive, so
        // the session would reappear on the next launch.
        let sim = progress.begin_simulated();
        let toast = ToastHandle::expect();
        spawn_local(async move {
            let _ = api::sign_out().await;
            account.set(None);
            toast.success(t("sign_out.success"));
            progress.end_simulated(&sim);
        });
    };

    // Command palette open state (toggled by ⌘K / Ctrl+K, or clicking the
    // header trigger). When open, a full-screen modal overlay with a centered
    // search box + filtered action list appears.
    let palette_open = RwSignal::new(false);
    let settings_open = RwSignal::new(false);
    let tutorial_step =
        RwSignal::new(should_start_beginner_tutorial().then_some(TutorialStep::SignIn));

    create_effect(move |_| {
        if let Some(step) = tutorial_step.get() {
            current_section.set(step.section());
        }
    });

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
                // Brand text, right next to the traffic lights.
                <div class="flex items-center gap-1 ml-3 pointer-events-none">
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
                        <IconButton
                            title=Signal::derive(move || t("tutorial.open"))
                            on_click=Callback::new(move |_| tutorial_step.set(Some(TutorialStep::SignIn)))
                        >
                            <Icon name=IconName::TutorialHelp class="size-4" />
                        </IconButton>
                        <LanguageToggle progress=progress />
                        <ThemeToggle />
                        <QuickRoutesMenu
                            current_section=current_section
                            pending_count=pending_count
                            progress=progress
                            on_app_route=Callback::new(move |_| settings_open.set(false))
                        />
                        <SettingsButton on_open=Callback::new(move |_| settings_open.set(true)) />
                    </div>
                </div>
            </header>

            <HeaderLoading progress=progress />

            // Command palette modal (shown when palette_open is true).
            <CommandPalette open=palette_open pending_count=pending_count progress=progress />

            // Body: sidebar (left) + content (right). The settings page is
            // constrained to this area so it slides under, but never over, the
            // app header.
            <div class="relative flex-1 min-h-0 overflow-hidden">
                <div class="flex h-full min-h-0">
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
                                            data-tutorial-target="sign-in"
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
                                            data-tutorial-target="sign-in"
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
                            {move || if let Some(step) = tutorial_step.get() {
                                view! { <TutorialDemoScreen step=step /> }.into_view()
                            } else {
                                match current_section.get() {
                                    AppSection::Repos => view! {
                                        <Repos account=account pending_count=pending_count on_sign_in_hint=on_sign_in_hint />
                                    }.into_view(),
                                    AppSection::Connect => view! {
                                        <Connect pending_count=pending_count />
                                    }.into_view(),
                                    AppSection::Keys => view! {
                                        <Keys pending_count=pending_count />
                                    }.into_view(),
                                }
                            }}
                        </div>
                    </main>
                </div>

                <SettingsPage
                    open=settings_open
                    progress=progress
                    on_back=Callback::new(move |_| settings_open.set(false))
                />
                <TutorialGuide step=tutorial_step current_section=current_section />
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
fn TutorialGuide(
    step: RwSignal<Option<TutorialStep>>,
    current_section: RwSignal<AppSection>,
) -> impl IntoView {
    let current = Signal::derive(move || step.get().unwrap_or(TutorialStep::SignIn));
    let target_rect = RwSignal::new(None::<TutorialTargetRect>);

    let refresh_target = move |active: TutorialStep| {
        request_animation_frame(move || {
            target_rect.set(find_tutorial_target(active));
            highlight_tutorial_target(active);
        });
    };

    create_effect(move |_| {
        if let Some(active) = step.get() {
            current_section.set(active.section());
            refresh_target(active);
        } else {
            clear_tutorial_target_highlight();
        }
    });

    let resize_handle = window_event_listener(ev::resize, move |_| {
        if let Some(active) = step.get_untracked() {
            target_rect.set(find_tutorial_target(active));
        }
    });
    on_cleanup(move || {
        resize_handle.remove();
        clear_tutorial_target_highlight();
    });

    let go_previous = move |_| {
        let Some(active) = step.get_untracked() else {
            return;
        };
        let index = active.index();
        if index == 0 {
            return;
        }
        let previous = TUTORIAL_STEPS[index - 1];
        current_section.set(previous.section());
        step.set(Some(previous));
    };

    let go_next = move |_| {
        let Some(active) = step.get_untracked() else {
            return;
        };
        let index = active.index();
        if index + 1 >= TUTORIAL_STEPS.len() {
            mark_beginner_tutorial_seen();
            step.set(None);
            return;
        }
        let next = TUTORIAL_STEPS[index + 1];
        current_section.set(next.section());
        step.set(Some(next));
    };

    view! {
        <Show when=move || step.get().is_some()>
            <div class="pointer-events-none fixed inset-0 z-[90]">
                <div
                    class="pointer-events-auto fixed z-[92] w-[min(20rem,calc(100vw-2rem))] rounded-lg border border-border bg-surface shadow-2xl"
                    style=move || tutorial_bubble_style(current.get(), target_rect.get())
                >
                    <div class="flex items-start justify-between gap-3 px-4 pt-4">
                        <div class="flex min-w-0 gap-3">
                            <div class="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary-soft text-primary">
                                {move || view! { <Icon name=current.get().icon() class="size-4" /> }}
                            </div>
                            <div class="min-w-0">
                                <div class="text-[11px] font-medium uppercase text-muted">
                                    {move || {
                                        t("tutorial.step_count")
                                            .replace("{current}", &(current.get().index() + 1).to_string())
                                            .replace("{total}", &TUTORIAL_STEPS.len().to_string())
                                    }}
                                </div>
                                <h3 class="mt-0.5 text-sm font-semibold text-content">
                                    {move || t(current.get().title_key())}
                                </h3>
                            </div>
                        </div>
                        <button
                            type="button"
                            title=move || t("tutorial.close")
                            aria-label=move || t("tutorial.close")
                            class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted hover:bg-bg hover:text-content focus:outline-none"
                            on:click=move |_| {
                                mark_beginner_tutorial_seen();
                                step.set(None);
                            }
                        >
                            <Icon name=IconName::Close class="size-3.5" />
                        </button>
                    </div>

                    <div class="px-4 pb-4 pt-3">
                        <p class="text-sm leading-5 text-muted">
                            {move || t(current.get().body_key())}
                        </p>

                        <div class="mt-4 flex items-center gap-1">
                            {TUTORIAL_STEPS.iter().copied().map(|item| {
                                view! {
                                    <span
                                        class=move || {
                                            let active = current.get();
                                            if item == active {
                                                "h-1 flex-1 rounded-full bg-primary"
                                            } else if item.index() < active.index() {
                                                "h-1 flex-1 rounded-full bg-primary/40"
                                            } else {
                                                "h-1 flex-1 rounded-full bg-border"
                                            }
                                        }
                                    ></span>
                                }
                            }).collect_view()}
                        </div>

                        <div class="mt-4 flex items-center justify-between gap-2">
                            <button
                                type="button"
                                class="px-2 py-1.5 text-xs font-medium text-muted hover:text-content focus:outline-none"
                                on:click=move |_| {
                                    mark_beginner_tutorial_seen();
                                    step.set(None);
                                }
                            >
                                {move || t("tutorial.skip")}
                            </button>
                            <div class="flex items-center gap-2">
                                <button
                                    type="button"
                                    class="rounded-md border border-border bg-bg px-3 py-1.5 text-xs font-medium text-content hover:bg-surface focus:outline-none disabled:opacity-50"
                                    prop:disabled=move || current.get().index() == 0
                                    on:click=go_previous
                                >
                                    {move || t("tutorial.previous")}
                                </button>
                                <button
                                    type="button"
                                    class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-on-primary hover:bg-primary-hover focus:outline-none"
                                    on:click=go_next
                                >
                                    {move || {
                                        if current.get().index() + 1 >= TUTORIAL_STEPS.len() {
                                            t("tutorial.finish")
                                        } else {
                                            t("tutorial.next")
                                        }
                                    }}
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn TutorialDemoScreen(step: TutorialStep) -> impl IntoView {
    match step {
        TutorialStep::Connect => view! { <TutorialConnectDemo /> }.into_view(),
        TutorialStep::CreateKey => view! { <TutorialKeysDemo /> }.into_view(),
        _ => view! { <TutorialReposDemo step=step /> }.into_view(),
    }
}

#[component]
fn TutorialReposDemo(step: TutorialStep) -> impl IntoView {
    let show_repo_actions = matches!(
        step,
        TutorialStep::BindKey
            | TutorialStep::CloneRepo
            | TutorialStep::ConnectRepo
            | TutorialStep::TestRepo
    );
    let show_more_menu = matches!(step, TutorialStep::ConnectRepo | TutorialStep::TestRepo);

    view! {
        <div class="flex h-full flex-col gap-5">
            <div class="flex items-center justify-between gap-3">
                <h1 class="text-2xl font-semibold text-content">{move || t("nav.repos")}</h1>
                <div class="flex shrink-0 items-center gap-2">
                    <button
                        type="button"
                        class="relative inline-flex size-9 items-center justify-center rounded-lg border border-border bg-surface text-content"
                    >
                        <Icon name=IconName::Download class="size-4" />
                    </button>
                    <button
                        type="button"
                        data-tutorial-target="sync-repos"
                        class="py-2 px-4 text-sm font-medium rounded-lg border border-border bg-primary-soft text-content"
                    >
                        {move || t("repos.sync")}
                    </button>
                </div>
            </div>

            <div class="flex flex-wrap items-center gap-2">
                <div class="flex-1 min-w-[12rem] rounded-lg border border-border bg-bg px-3 py-2 text-sm text-muted">
                    {move || t("repos.search_placeholder")}
                </div>
                <div class="rounded-lg border border-border bg-bg px-3 py-2 text-sm text-muted">{move || t("repos.all")}</div>
                <div class="rounded-lg border border-border bg-bg px-3 py-2 text-sm text-muted">{move || t("repos.all_languages")}</div>
            </div>

            <div class="flex flex-col flex-1 min-h-0">
                <div class="relative flex-1 min-h-0 overflow-hidden rounded-lg border border-border bg-surface">
                    <table class="min-w-[34rem] w-full table-fixed border-collapse text-sm">
                        <thead class="bg-surface">
                            <tr class="border-b border-border">
                                <th class="w-[11rem] text-start font-medium text-muted px-3 py-2">{move || t("repos.repository")}</th>
                                <th class="w-[6rem] text-start font-medium text-muted px-3 py-2">{move || t("repos.visibility")}</th>
                                <th class="w-[7rem] text-start font-medium text-muted px-3 py-2">{move || t("repos.language")}</th>
                                <th class="w-[8rem] text-start font-medium text-muted px-3 py-2">{move || t("repos.actions")}</th>
                            </tr>
                        </thead>
                        <tbody>
                            <tr class="border-b border-border hover:bg-bg">
                                <td class="w-[11rem] px-3 py-2 font-medium text-content">"demo/app"</td>
                                <td class="w-[6rem] px-3 py-2">
                                    <span class="inline-flex items-center text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">{move || t("repos.public")}</span>
                                </td>
                                <td class="w-[7rem] px-3 py-2 text-muted">
                                    <span class="inline-flex items-center gap-1.5">
                                        <span class="inline-block size-2.5 rounded-full bg-[#dea584]"></span>
                                        "Rust"
                                    </span>
                                </td>
                                <td class="w-[8rem] px-3 py-2">
                                    <Show
                                        when=move || show_repo_actions
                                        fallback=move || view! { <span class="text-xs text-muted">{move || t("tutorial.demo_ready")}</span> }
                                    >
                                        <div class="relative inline-flex min-w-max items-center gap-1.5">
                                            <button
                                                type="button"
                                                data-tutorial-target="clone-repo"
                                                class="inline-flex items-center justify-center size-8 rounded-md text-content bg-primary-soft"
                                            >
                                                <Icon name=IconName::Download class="size-4" />
                                            </button>
                                            <button
                                                type="button"
                                                data-tutorial-target="bind-key"
                                                class="inline-flex items-center justify-center size-8 rounded-md text-content bg-primary-soft/70"
                                            >
                                                <Icon name=IconName::Key class="size-4" />
                                            </button>
                                            <button
                                                type="button"
                                                data-tutorial-target="repository-more-actions"
                                                class="inline-flex items-center justify-center size-8 rounded-md text-content bg-primary-soft/70"
                                            >
                                                <Icon name=IconName::MoreVertical class="size-4" />
                                            </button>
                                            <Show when=move || show_more_menu>
                                                <div class="absolute right-0 top-10 z-20 min-w-28 rounded-lg border border-border bg-surface p-1 shadow-xl">
                                                    <button
                                                        type="button"
                                                        data-tutorial-target="connect-repo"
                                                        class="flex w-full items-center justify-between rounded-md px-2.5 py-2 text-xs text-content bg-bg"
                                                    >
                                                        <span>{move || t("repos.connect_remote")}</span>
                                                        <Icon name=IconName::Server class="size-3.5" />
                                                    </button>
                                                    <button
                                                        type="button"
                                                        data-tutorial-target="test-repo"
                                                        class="mt-1 flex w-full items-center justify-between rounded-md px-2.5 py-2 text-xs text-content bg-bg"
                                                    >
                                                        <span>{move || t("repos.test_remote")}</span>
                                                        <Icon name=IconName::Check class="size-3.5" />
                                                    </button>
                                                </div>
                                            </Show>
                                        </div>
                                    </Show>
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    }
}

#[component]
fn TutorialConnectDemo() -> impl IntoView {
    view! {
        <div class="flex h-full flex-col gap-5">
            <div class="flex items-center justify-between gap-3">
                <h1 class="text-2xl font-semibold text-content">{move || t("connect.title")}</h1>
                <button type="button" class="shrink-0 py-2 px-4 text-sm font-medium rounded-lg border border-border bg-primary text-on-primary">
                    {move || t("connect.add")}
                </button>
            </div>
            <div class="flex items-center gap-2 min-w-0">
                <div class="flex-1 rounded-lg border border-border bg-bg px-3 py-2 text-sm text-muted">{move || t("connect.search_placeholder")}</div>
                <div class="rounded-lg border border-border bg-bg px-3 py-2 text-sm text-muted">{move || t("connect.filter_type_all")}</div>
                <div class="rounded-lg border border-border bg-bg px-3 py-2 text-sm text-muted">{move || t("connect.filter_status_all")}</div>
            </div>
            <div class="overflow-hidden rounded-lg border border-border bg-surface">
                <table class="min-w-[30rem] w-full table-fixed border-collapse text-sm">
                    <thead>
                        <tr class="border-b border-border">
                            <th class="text-start font-medium text-muted px-3 py-2">{move || t("connect.name")}</th>
                            <th class="w-[5rem] text-start font-medium text-muted px-3 py-2">{move || t("connect.type")}</th>
                            <th class="w-[8rem] text-start font-medium text-muted px-3 py-2">{move || t("connect.status")}</th>
                            <th class="w-[8rem] text-start font-medium text-muted px-3 py-2">{move || t("connect.actions")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr class="border-b border-border hover:bg-bg">
                            <td class="px-3 py-2 font-medium text-content">{move || t("connect.local_name")}</td>
                            <td class="px-3 py-2 text-muted">{move || t("connect.type_local")}</td>
                            <td class="px-3 py-2">
                                <span class="inline-flex items-center gap-1.5 text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">
                                    <span class="size-1.5 rounded-full bg-muted"></span>
                                    {move || t("connect.status_offline")}
                                </span>
                            </td>
                            <td class="px-3 py-2">
                                <button
                                    type="button"
                                    data-tutorial-target="connect-environment"
                                    class="inline-flex items-center justify-center size-8 rounded-md text-content bg-primary-soft"
                                >
                                    <Icon name=IconName::Power class="size-4" />
                                </button>
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
    }
}

#[component]
fn TutorialKeysDemo() -> impl IntoView {
    view! {
        <div class="flex h-full flex-col gap-5">
            <div class="flex items-center justify-between gap-3">
                <h1 class="text-2xl font-semibold text-content">{move || t("keys.title")}</h1>
                <button
                    type="button"
                    data-tutorial-target="create-key"
                    class="shrink-0 py-2 px-4 text-sm font-medium rounded-lg border border-border bg-primary-soft text-content"
                >
                    {move || t("keys.create")}
                </button>
            </div>
            <div class="flex items-center gap-2 min-w-0">
                <div class="flex-1 rounded-lg border border-border bg-bg px-3 py-2 text-sm text-muted">{move || t("keys.search_placeholder")}</div>
                <div class="rounded-lg border border-border bg-bg px-3 py-2 text-sm text-muted">{move || t("keys.filter_algorithm_all")}</div>
                <div class="rounded-lg border border-border bg-bg px-3 py-2 text-sm text-muted">{move || t("keys.filter_date_all")}</div>
            </div>
            <div class="overflow-hidden rounded-lg border border-border bg-surface">
                <table class="min-w-[36rem] w-full table-fixed border-collapse text-sm">
                    <thead>
                        <tr class="border-b border-border">
                            <th class="w-[8rem] text-start font-medium text-muted px-3 py-2">{move || t("keys.directory")}</th>
                            <th class="w-[9rem] text-start font-medium text-muted px-3 py-2">{move || t("keys.algorithm")}</th>
                            <th class="w-[8rem] text-start font-medium text-muted px-3 py-2">{move || t("keys.remark")}</th>
                            <th class="w-[9rem] text-start font-medium text-muted px-3 py-2">{move || t("keys.created_at")}</th>
                            <th class="w-[6rem] text-start font-medium text-muted px-3 py-2">{move || t("keys.actions")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr class="border-b border-border hover:bg-bg">
                            <td class="px-3 py-2 font-mono font-medium text-content">"prod-deploy"</td>
                            <td class="px-3 py-2"><span class="inline-flex items-center text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">"ed25519"</span></td>
                            <td class="px-3 py-2 text-muted">"production"</td>
                            <td class="px-3 py-2 text-muted">"2026-06-20"</td>
                            <td class="px-3 py-2 text-muted">"..."</td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
    }
}

fn should_start_beginner_tutorial() -> bool {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(TUTORIAL_STORAGE_KEY).ok().flatten())
        .is_none()
}

fn mark_beginner_tutorial_seen() {
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.set_item(TUTORIAL_STORAGE_KEY, "1");
    }
}

fn find_tutorial_target(step: TutorialStep) -> Option<TutorialTargetRect> {
    let element = find_tutorial_target_element(step)?;
    let rect = element.get_bounding_client_rect();
    Some(TutorialTargetRect {
        top: rect.top(),
        left: rect.left(),
        width: rect.width(),
        height: rect.height(),
    })
}

fn highlight_tutorial_target(step: TutorialStep) {
    clear_tutorial_target_highlight();
    let Some(element) = find_tutorial_target_element(step) else {
        return;
    };
    let _ = element.class_list().add_1("tutorial-active-target");
}

fn find_tutorial_target_element(step: TutorialStep) -> Option<web_sys::Element> {
    let document = web_sys::window()?.document()?;
    let elements = document.query_selector_all(step.target_selector()).ok()?;
    let mut fallback = None::<web_sys::Element>;

    for index in 0..elements.length() {
        let Some(element) = elements
            .item(index)
            .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        let rect = element.get_bounding_client_rect();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        let class_name = element.class_name();
        if fallback.is_none() {
            fallback = Some(element.clone());
        }
        if !class_name.contains("opacity-0") && !class_name.contains("pointer-events-none") {
            return Some(element);
        }
    }

    fallback
}

fn clear_tutorial_target_highlight() {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Ok(elements) = document.query_selector_all(".tutorial-active-target") else {
        return;
    };
    for index in 0..elements.length() {
        if let Some(element) = elements
            .item(index)
            .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
        {
            let _ = element.class_list().remove_1("tutorial-active-target");
        }
    }
}

fn tutorial_bubble_style(step: TutorialStep, rect: Option<TutorialTargetRect>) -> String {
    let target = rect.unwrap_or_else(|| tutorial_fallback_rect(step));
    let (viewport_width, viewport_height) = viewport_size();
    let bubble_width = 320.0;
    let bubble_height = 210.0;
    let gap = 14.0;
    let margin = 16.0;

    let fits_right = target.left + target.width + gap + bubble_width <= viewport_width - margin;
    let fits_left = target.left - gap - bubble_width >= margin;
    let left = if fits_right {
        target.left + target.width + gap
    } else if fits_left {
        target.left - gap - bubble_width
    } else {
        target
            .left
            .clamp(margin, (viewport_width - bubble_width - margin).max(margin))
    };
    let top = (target.top + target.height / 2.0 - bubble_height / 2.0)
        .clamp(68.0, (viewport_height - bubble_height - margin).max(68.0));

    format!("top:{top:.1}px;left:{left:.1}px;")
}

fn tutorial_fallback_rect(step: TutorialStep) -> TutorialTargetRect {
    let (viewport_width, viewport_height) = viewport_size();
    match step {
        TutorialStep::SignIn => TutorialTargetRect {
            top: (viewport_height - 74.0).max(80.0),
            left: 14.0,
            width: 188.0,
            height: 46.0,
        },
        TutorialStep::Sync => TutorialTargetRect {
            top: 86.0,
            left: (viewport_width - 164.0).max(220.0),
            width: 118.0,
            height: 46.0,
        },
        TutorialStep::Connect => TutorialTargetRect {
            top: 184.0,
            left: (viewport_width - 180.0).max(260.0),
            width: 128.0,
            height: 46.0,
        },
        TutorialStep::CreateKey => TutorialTargetRect {
            top: 86.0,
            left: (viewport_width - 184.0).max(220.0),
            width: 138.0,
            height: 46.0,
        },
        TutorialStep::BindKey
        | TutorialStep::CloneRepo
        | TutorialStep::ConnectRepo
        | TutorialStep::TestRepo => TutorialTargetRect {
            top: 214.0,
            left: (viewport_width - 178.0).max(220.0),
            width: 132.0,
            height: 46.0,
        },
    }
}

fn viewport_size() -> (f64, f64) {
    let Some(window) = web_sys::window() else {
        return (1024.0, 768.0);
    };
    let width = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(1024.0);
    let height = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(768.0);
    (width, height)
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
fn SidebarDivider(
    #[prop(into)] collapsed: Signal<bool>,
    on_toggle: Callback<()>,
    #[prop(into)] mouse_in_sidebar: Signal<bool>,
) -> impl IntoView {
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
                                           // When true, the palette shows the language list instead of commands — the
                                           // "Change language" command flips this on, and the search box then filters
                                           // locales. Picking a locale applies it and closes the whole palette.
    let language_mode = RwSignal::new(false);

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
                    // Switch the palette into language-picker mode: the existing
                    // search input then filters the 31 languages (via the shared
                    // LanguagePicker) instead of the command list, and picking
                    // one applies it through the same apply_locale path as the
                    // header/settings entry points. This replaces the old
                    // hardcoded En<->Zh toggle, which could not express "switch
                    // to any language" once the enum grew past two variants.
                    language_mode.set(true);
                    query.set(String::new());
                    selected.set(0);
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

    // In language mode the list is the query-filtered locales (same matching
    // rule as the shared picker), so keyboard nav applies to it too.
    let filtered_locales = move || -> Vec<Locale> {
        let q = query.get();
        Locale::ALL
            .iter()
            .copied()
            .filter(|&loc| locale_matches(loc, &q))
            .collect()
    };

    // Open: reset query, selected, language mode, focus input.
    create_effect(move |_| {
        if open.get() {
            query.set(String::new());
            selected.set(0);
            language_mode.set(false);
            request_animation_frame(move || {
                if let Some(input) = input_ref.get() {
                    let _ = input.focus();
                }
            });
        }
    });

    // Keyboard: ESC close (exits language mode first if active), ArrowUp/Down
    // navigate whichever list is showing, Enter runs the selection.
    let handle = window_event_listener(ev::keydown, move |ev| {
        if !open.get_untracked() {
            return;
        }
        let key = ev.key();
        if key.eq_ignore_ascii_case("escape") {
            ev.prevent_default();
            // In language mode, ESC steps back to the command list instead of
            // closing the palette outright.
            if language_mode.get_untracked() {
                language_mode.set(false);
                query.set(String::new());
                selected.set(0);
            } else {
                open.set(false);
            }
        } else if key == "ArrowDown" {
            ev.prevent_default();
            let len = if language_mode.get_untracked() {
                filtered_locales().len()
            } else {
                filtered().len()
            };
            if len > 0 {
                selected.update(|s| *s = (*s + 1).min(len - 1));
            }
        } else if key == "ArrowUp" {
            ev.prevent_default();
            selected.update(|s| *s = s.saturating_sub(1));
        } else if key == "Enter" {
            ev.prevent_default();
            if language_mode.get_untracked() {
                let locales = filtered_locales();
                let idx = selected.get_untracked();
                if idx < locales.len() {
                    apply_locale(locales[idx], progress);
                    language_mode.set(false);
                    query.set(String::new());
                    selected.set(0);
                    open.set(false);
                }
            } else {
                let items = filtered();
                let idx = selected.get_untracked();
                if idx < items.len() {
                    let (label, _icon, action) = &items[idx];
                    save_history(label);
                    action();
                }
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
                            placeholder=move || {
                                if language_mode.get() {
                                    t("settings.language_search_placeholder")
                                } else {
                                    t("palette.placeholder")
                                }
                            }
                            class="flex-1 text-base bg-transparent text-content placeholder:text-muted focus:outline-none"
                        />
                    </div>

                    // Results list (scrollable)
                    <div class="max-h-96 overflow-y-auto py-2 px-2">
                        {move || {
                            // Language sub-mode: render the filtered locales with
                            // the same keyboard-select/hover interaction as the
                            // command list, so arrows + Enter work here too.
                            if language_mode.get() {
                                let locales = filtered_locales();
                                if locales.is_empty() {
                                    view! {
                                        <div class="py-8 text-center text-sm text-muted">
                                            {move || t("settings.language_no_results")}
                                        </div>
                                    }.into_view()
                                } else {
                                    locales.into_iter().enumerate().map(|(idx, loc)| {
                                        let active = move || selected.get() == idx;
                                        let is_current = move || i18n::locale().get() == loc;
                                        view! {
                                            <div
                                                class="flex items-center gap-3 py-2.5 px-3 rounded-lg text-sm cursor-pointer"
                                                class:bg-primary=active
                                                class:text-on-primary=active
                                                class:text-content=move || !active()
                                                class:hover:bg-bg=move || !active()
                                                on:click=move |_| {
                                                    apply_locale(loc, progress);
                                                    language_mode.set(false);
                                                    query.set(String::new());
                                                    selected.set(0);
                                                    open.set(false);
                                                }
                                                on:mouseenter=move |_| selected.set(idx)
                                            >
                                                {move || {
                                                    let class = if is_current() {
                                                        "size-5 text-on-primary"
                                                    } else {
                                                        "size-5 text-on-primary opacity-0"
                                                    };
                                                    view! { <Icon name=IconName::Check class=class /> }
                                                }}
                                                <span class="flex-1">{loc.native_name()}</span>
                                                <span class="text-xs uppercase tracking-wide opacity-70">
                                                    {loc.code()}
                                                </span>
                                            </div>
                                        }
                                    }).collect_view()
                                }
                            } else {
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
    on_app_route: Callback<()>,
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
                                            on_app_route.call(());
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
fn SettingsButton(on_open: Callback<()>) -> impl IntoView {
    view! {
        <IconButton title=move || t("settings.title") on_click=on_open>
            <Icon name=IconName::SettingsPlaceholder class="size-4" />
        </IconButton>
    }
}

#[component]
fn SettingsPage(
    #[prop(into)] open: Signal<bool>,
    progress: ProgressHandle,
    on_back: Callback<()>,
) -> impl IntoView {
    let active_tab = RwSignal::new(SettingsTab::General);

    view! {
        <section class=move || {
            let base = "absolute inset-0 z-40 h-full bg-bg text-content transform-gpu will-change-transform transition-transform duration-500 [transition-timing-function:cubic-bezier(0.16,1,0.3,1)]";
            if open.get() {
                format!("{base} translate-x-0")
            } else {
                format!("{base} translate-x-full pointer-events-none")
            }
        }>
            <div class="h-full overflow-hidden px-4 sm:px-8 pt-6 pb-8">
                <div class="max-w-4xl mx-auto flex h-full min-h-0 flex-col">
                    <div class="shrink-0 flex items-center gap-3">
                        <button
                            type="button"
                            title=move || t("settings.back")
                            aria-label=move || t("settings.back")
                            class="shrink-0 flex items-center justify-center size-10 rounded-lg text-content hover:bg-surface hover:text-primary focus:outline-none"
                            on:click=move |_| on_back.call(())
                        >
                            <Icon name=IconName::SettingsBack class="size-6" />
                        </button>
                        <h1 class="text-xl font-semibold leading-7 text-content">{move || t("settings.title")}</h1>
                    </div>

                    <div
                        class="mt-8 grid flex-1 min-h-0 gap-4 sm:gap-6"
                        style:grid-template-columns=settings_grid_columns
                    >
                        <aside class="min-h-0 overflow-visible border-r border-border pr-3">
                            <nav class="flex flex-col gap-1">
                                <SettingsTabButton
                                    tab=SettingsTab::General
                                    active_tab=active_tab
                                    label=move || t("settings.general")
                                />
                                <SettingsTabButton
                                    tab=SettingsTab::KeyStorage
                                    active_tab=active_tab
                                    label=move || t("settings.key_storage")
                                />
                                <SettingsTabButton
                                    tab=SettingsTab::ConfigFile
                                    active_tab=active_tab
                                    label=move || t("settings.config_file")
                                />
                            </nav>
                        </aside>

                        <div class="min-h-0 min-w-0 overflow-visible">
                            {move || match active_tab.get() {
                                SettingsTab::General => view! {
                                    <div class="border-y border-border divide-y divide-border">
                                        <LanguageSettingRow progress=progress />
                                        <ThemeSettingRow />
                                    </div>
                                }.into_view(),
                                SettingsTab::KeyStorage => view! {
                                    <KeyStorageSettingsTab open=open progress=progress />
                                }.into_view(),
                                SettingsTab::ConfigFile => view! {
                                    <ConfigFileSettingsTab open=open progress=progress />
                                }.into_view(),
                            }}
                        </div>
                    </div>
                </div>
            </div>
        </section>
    }
}

#[component]
fn SettingsTabButton(
    tab: SettingsTab,
    active_tab: RwSignal<SettingsTab>,
    #[prop(into)] label: Signal<&'static str>,
) -> impl IntoView {
    let active = move || active_tab.get() == tab;
    view! {
        <button
            type="button"
            class=move || settings_tab_button_class(active())
            on:click=move |_| active_tab.set(tab)
        >
            {move || label.get()}
        </button>
    }
}

fn settings_tab_button_class(active: bool) -> &'static str {
    if active {
        "w-full flex h-10 items-center whitespace-nowrap rounded-lg bg-primary-soft px-3 text-sm font-medium text-primary focus:outline-none"
    } else {
        "w-full flex h-10 items-center whitespace-nowrap rounded-lg px-3 text-sm font-medium text-muted hover:bg-surface hover:text-content focus:outline-none"
    }
}

fn settings_grid_columns() -> String {
    let longest = [
        t("settings.general"),
        t("settings.key_storage"),
        t("settings.config_file"),
    ]
    .into_iter()
    .map(|label| label.chars().count())
    .max()
    .unwrap_or(0);
    let menu_rem = (3.0 + longest as f64 * 0.58).clamp(6.0, 14.0);
    format!("minmax(6rem, {menu_rem:.2}rem) minmax(16rem, 1fr)")
}

#[component]
fn KeyStorageSettingsTab(
    #[prop(into)] open: Signal<bool>,
    progress: ProgressHandle,
) -> impl IntoView {
    let toast = ToastHandle::expect();
    let loaded = RwSignal::new(false);
    let loading = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let storage_dir = RwSignal::new(String::new());

    let load_storage_dir = move || {
        if loading.get_untracked() {
            return;
        }
        loading.set(true);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::get_ssh_key_storage_dir().await {
                Ok(path) => storage_dir.set(path),
                Err(e) => toast.error(e),
            }
            loading.set(false);
            progress.end_simulated(&sim);
        });
    };

    create_effect(move |_| {
        if open.get() && !loaded.get_untracked() {
            loaded.set(true);
            load_storage_dir();
        }
    });

    let choose_directory = move |_| {
        if saving.get_untracked() {
            return;
        }
        spawn_local(async move {
            match api::pick_ssh_key_storage_dir().await {
                Ok(Some(path)) => {
                    storage_dir.set(path);
                }
                Ok(None) => {}
                Err(e) => toast.error(e),
            }
        });
    };

    let save_directory = move |_| {
        if saving.get_untracked() {
            return;
        }
        saving.set(true);
        let path = storage_dir.get_untracked();
        let sim = progress.begin_simulated();
        let toast = ToastHandle::expect();
        spawn_local(async move {
            match api::set_ssh_key_storage_dir(path).await {
                Ok(saved) => {
                    storage_dir.set(saved);
                    toast.success(t("settings.key_storage_saved"));
                }
                Err(e) => toast.error(e),
            }
            saving.set(false);
            progress.end_simulated(&sim);
        });
    };

    view! {
        <div class="max-w-3xl">
            <div class="flex flex-col gap-3 sm:flex-row sm:items-center">
                <input
                    type="text"
                    prop:value=move || storage_dir.get()
                    on:input=move |ev| storage_dir.set(event_target_value(&ev))
                    placeholder=move || t("settings.key_storage_placeholder")
                    class="min-w-0 flex-1 h-10 px-3 rounded-lg border border-border bg-surface text-sm text-content placeholder:text-muted focus:outline-none focus:border-primary"
                />
                <button
                    type="button"
                    class="h-10 px-3 rounded-lg border border-border bg-surface text-sm font-medium text-content hover:bg-bg focus:outline-none"
                    on:click=choose_directory
                >
                    {move || t("settings.choose_folder")}
                </button>
                <button
                    type="button"
                    class="h-10 px-4 rounded-lg bg-primary text-sm font-medium text-on-primary hover:bg-primary-hover focus:outline-none"
                    on:click=save_directory
                >
                    {move || t("settings.save")}
                </button>
            </div>
            <p class="mt-3 text-xs text-muted">{move || t("settings.key_storage_help")}</p>
        </div>
    }
}

#[component]
fn ConfigFileSettingsTab(
    #[prop(into)] open: Signal<bool>,
    progress: ProgressHandle,
) -> impl IntoView {
    let toast = ToastHandle::expect();
    let loaded = RwSignal::new(false);
    let loading = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let config_path = RwSignal::new(String::new());
    let content = RwSignal::new(String::new());
    let last_saved = RwSignal::new(String::new());

    let load_config = move || {
        if loading.get_untracked() {
            return;
        }
        loading.set(true);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::get_ssh_config_file().await {
                Ok(file) => {
                    config_path.set(file.path);
                    last_saved.set(file.content.clone());
                    content.set(file.content);
                }
                Err(e) => toast.error(e),
            }
            loading.set(false);
            progress.end_simulated(&sim);
        });
    };

    create_effect(move |_| {
        if open.get() && !loaded.get_untracked() {
            loaded.set(true);
            load_config();
        }
    });

    let reload_config = move |_| load_config();

    let save_config = move |_| {
        if saving.get_untracked() {
            return;
        }
        saving.set(true);
        let next = content.get_untracked();
        let sim = progress.begin_simulated();
        let toast = ToastHandle::expect();
        spawn_local(async move {
            match api::save_ssh_config_file(next).await {
                Ok(file) => {
                    config_path.set(file.path);
                    last_saved.set(file.content.clone());
                    content.set(file.content);
                    toast.success(t("settings.config_saved"));
                }
                Err(e) => toast.error(e),
            }
            saving.set(false);
            progress.end_simulated(&sim);
        });
    };

    view! {
        <div class="flex h-full min-h-0 min-w-0 flex-col overflow-hidden">
            <div class="shrink-0 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div class="min-w-0">
                    <p class="truncate text-sm text-muted">{move || {
                        let path = config_path.get();
                        if path.is_empty() {
                            t("settings.config_file").to_string()
                        } else {
                            path
                        }
                    }}</p>
                </div>
                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        class="h-9 px-3 rounded-lg border border-border bg-surface text-sm font-medium text-content hover:bg-bg focus:outline-none"
                        on:click=reload_config
                    >
                        {move || t("settings.reload")}
                    </button>
                    <button
                        type="button"
                        class="h-9 px-4 rounded-lg bg-primary text-sm font-medium text-on-primary hover:bg-primary-hover focus:outline-none"
                        on:click=save_config
                    >
                        {move || t("settings.save")}
                    </button>
                </div>
            </div>
            <textarea
                prop:value=move || content.get()
                on:input=move |ev| content.set(event_target_value(&ev))
                spellcheck="false"
                class="mt-4 min-h-0 w-full flex-1 resize-none overflow-auto rounded-lg border border-border bg-surface p-4 font-mono text-xs leading-5 text-content placeholder:text-muted focus:outline-none focus:border-primary [scrollbar-gutter:stable]"
                placeholder=move || t("settings.config_placeholder")
            ></textarea>
        </div>
    }
}

#[component]
fn LanguageSettingRow(progress: ProgressHandle) -> impl IntoView {
    let locale = i18n::locale();
    let open = RwSignal::new(false);
    let query = RwSignal::new(String::new());

    let select = move |next: Locale| {
        apply_locale(next, progress);
        open.set(false);
        query.set(String::new());
    };

    view! {
        <div class="flex flex-col gap-3 py-5 sm:flex-row sm:items-center sm:justify-between">
            <div class="min-w-0">
                <p class="text-sm font-medium text-content">{move || t("settings.language")}</p>
            </div>
            // Compact trigger button showing the current language name + globe,
            // opening the same shared picker as the header dropdown. Replacing
            // the old segmented buttons (which only scaled to 2–3 languages).
            <div class="relative w-full sm:w-auto sm:self-end">
                <button
                    type="button"
                    aria-haspopup="listbox"
                    aria-expanded=move || open.get()
                    class="inline-flex w-full sm:w-[min(100%,16rem)] h-9 items-center justify-between gap-2 rounded-lg border border-border bg-surface px-3 text-sm font-medium text-content hover:bg-bg focus:outline-none"
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    <span class="flex items-center gap-2 min-w-0">
                        <Icon name=IconName::Globe class="size-4 shrink-0 text-muted" />
                        <span class="truncate">{move || locale.get().native_name()}</span>
                    </span>
                    <Icon name=IconName::ChevronRight class="size-4 shrink-0 rotate-90 text-muted" />
                </button>

                <Show when=move || open.get()>
                    <div
                        class="fixed inset-0 z-40"
                        on:click=move |_| {
                            open.set(false);
                            query.set(String::new());
                        }
                    ></div>

                    // Right-aligned dropdown within the settings content area.
                    // It starts at the trigger-friendly settings width and grows
                    // when filtered language labels need more room, capped by
                    // the viewport so narrow screens do not overflow.
                    <div
                        class="absolute end-0 mt-2 z-50 max-h-[min(420px,calc(100vh-96px))] flex flex-col p-1 bg-surface border border-border rounded-xl shadow-xl"
                        style:width=move || language_menu_width(&query.get(), 20.0)
                    >
                        <div class="shrink-0 px-1 pb-1">
                            <input
                                type="text"
                                prop:value=move || query.get()
                                on:input=move |ev| query.set(event_target_value(&ev))
                                placeholder=move || t("settings.language_search_placeholder")
                                class="w-full h-8 px-2.5 text-sm bg-bg text-content placeholder:text-muted rounded-md border border-border focus:outline-none focus:border-primary"
                            />
                        </div>
                        <div class="min-h-0 flex-1 overflow-y-auto p-0.5">
                            <LanguagePicker query=query on_select=Callback::new(select) />
                        </div>
                    </div>
                </Show>
            </div>
        </div>
    }
}

#[component]
fn ThemeSettingRow() -> impl IntoView {
    let theme_signal = theme::theme();

    view! {
        <div class="flex flex-col gap-3 py-5 sm:flex-row sm:items-center sm:justify-between">
            <div class="min-w-0">
                <p class="text-sm font-medium text-content">{move || t("settings.theme")}</p>
            </div>
            <div class="inline-flex w-fit flex-wrap gap-1 rounded-lg border border-border bg-surface p-1">
                <button
                    type="button"
                    class=move || settings_theme_button_class(theme_signal.get() == Theme::System)
                    on:click=move |_| theme_signal.set(Theme::System)
                >
                    <Icon name=IconName::Monitor class="size-4" />
                    <span>{move || t("settings.theme_system")}</span>
                </button>
                <button
                    type="button"
                    class=move || settings_theme_button_class(theme_signal.get() == Theme::Light)
                    on:click=move |_| theme_signal.set(Theme::Light)
                >
                    <Icon name=IconName::Sun class="size-4" />
                    <span>{move || t("settings.theme_light")}</span>
                </button>
                <button
                    type="button"
                    class=move || settings_theme_button_class(theme_signal.get() == Theme::Dark)
                    on:click=move |_| theme_signal.set(Theme::Dark)
                >
                    <Icon name=IconName::Moon class="size-4" />
                    <span>{move || t("settings.theme_dark")}</span>
                </button>
            </div>
        </div>
    }
}

fn settings_theme_button_class(active: bool) -> &'static str {
    if active {
        "inline-flex h-8 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-on-primary"
    } else {
        "inline-flex h-8 items-center gap-2 rounded-md px-3 text-sm font-medium text-muted hover:bg-bg hover:text-content"
    }
}

/// Apply a locale everywhere it matters: set the reactive signal (which
/// re-renders every `t(...)` call), update `<html lang>`/`<html dir>` for the
/// new language/RTL state, persist the choice through `set_language`, and toast
/// the change. All three language entry points route through this so they stay
/// in lockstep — there is no third copy of the switch/persist/toast logic.
fn apply_locale(next: Locale, progress: ProgressHandle) {
    let locale = i18n::locale();
    if locale.get_untracked() == next {
        return;
    }
    locale.set(next);
    let code = next.code();
    let sim = progress.begin_simulated();
    let toast = ToastHandle::expect();
    spawn_local(async move {
        let saved = api::set_language(code).await;
        let current = locale.get_untracked();
        if current.code() != code {
            // Language changes can be clicked faster than IPC writes complete.
            // If this older save finishes after a newer selection, repair the
            // persisted value so restart restores the language the user ended on.
            let _ = api::set_language(current.code()).await;
            progress.end_simulated(&sim);
            return;
        }
        if saved.is_ok() {
            toast.success(t("settings.language_changed"));
        }
        progress.end_simulated(&sim);
    });
}

/// Mirror the reactive locale onto the document element so the browser (and
/// screen readers) know the active language. The app shell intentionally stays
/// LTR even for Arabic; setting `dir=rtl` on `<html>` mirrors every layout-level
/// logical class and makes the whole product chrome flip sides.
fn install_locale_sync_effect() {
    create_effect(move |_| {
        let loc = i18n::locale().get();
        apply_html_language(loc);
    });
}

/// Write `<html lang>` for a locale and keep global layout direction stable.
/// No-op if the document can't be reached (e.g. outside the browser).
fn apply_html_language(loc: Locale) {
    let Some(doc) = (|| web_sys::window()?.document())() else {
        return;
    };
    let html = doc.document_element();
    if let Some(html) = html {
        let _ = html.set_attribute("lang", loc.code());
        let _ = html.set_attribute("dir", "ltr");
    }
}

/// Does `query` match `loc`? Case-insensitive substring match against the
/// locale's native name, English name, code, and any search aliases. An empty
/// query matches everything (so the picker shows the full list by default).
fn locale_matches(loc: Locale, query: &str) -> bool {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return true;
    }
    if loc.native_name().to_ascii_lowercase().contains(&q) {
        return true;
    }
    if loc.english_name().to_ascii_lowercase().contains(&q) {
        return true;
    }
    if loc.code().to_ascii_lowercase().contains(&q) {
        return true;
    }
    loc.search_aliases()
        .iter()
        .any(|alias| alias.to_ascii_lowercase().contains(&q))
}

/// Width for the language menus: keep a stable base width, but expand when the
/// visible language labels or localized placeholder need more horizontal room.
/// The CSS `min()` cap keeps the menu inside the viewport on small screens.
fn language_menu_width(query: &str, base_rem: f64) -> String {
    let longest_label = Locale::ALL
        .iter()
        .copied()
        .filter(|&loc| locale_matches(loc, query))
        .map(|loc| loc.native_name().chars().count() + loc.code().chars().count())
        .max()
        .unwrap_or(0)
        .max(t("settings.language_search_placeholder").chars().count());
    let content_rem = 7.5 + (longest_label as f64 * 0.58);
    let width_rem = base_rem.max(content_rem).min(32.0);
    format!("min(calc(100vw - 2rem), {width_rem:.2}rem)")
}

/// Shared, searchable language list used by all three language entry points
/// (top dropdown, settings row, and the command palette's language sub-mode).
/// It only renders the list and reports a selection via `on_select` — the
/// caller owns open/close state and the apply/persist logic, so the same picker
/// can be hosted in a dropdown, a settings row, or an inline palette list.
///
/// Rows are compact (~30px): a fixed-width check slot keeps text from shifting
/// between selected/unselected, the native name leads, and a muted short code
/// sits on the right. An empty filter shows the dedicated "no results" string.
#[component]
fn LanguagePicker(
    #[prop(into)] query: Signal<String>,
    on_select: Callback<Locale>,
) -> impl IntoView {
    let locale = i18n::locale();
    let filtered = move || {
        let q = query.get();
        Locale::ALL
            .iter()
            .copied()
            .filter(|&loc| locale_matches(loc, &q))
            .collect::<Vec<_>>()
    };
    view! {
        {move || {
            let items = filtered();
            if items.is_empty() {
                view! {
                    <div class="py-8 text-center text-sm text-muted">
                        {move || t("settings.language_no_results")}
                    </div>
                }.into_view()
            } else {
                items.into_iter().map(|loc| {
                    let active = move || locale.get() == loc;
                    view! {
                        <button
                            type="button"
                            class="w-full flex items-center gap-x-2.5 h-8 px-2.5 rounded-lg text-sm text-content hover:bg-bg focus:outline-none focus:bg-bg"
                            on:click=move |_| on_select.call(loc)
                        >
                            {move || {
                                let class = if active() {
                                    "size-4 text-primary"
                                } else {
                                    "size-4 text-primary opacity-0"
                                };
                                view! { <Icon name=IconName::Check class=class /> }
                            }}
                            <span class="grow text-start truncate">{loc.native_name()}</span>
                            <span class="shrink-0 text-[10px] uppercase tracking-wide text-muted">
                                {loc.code()}
                            </span>
                        </button>
                    }
                }).collect_view()
            }
        }}
    }
}

/// Language toggle in the header: a globe icon button that opens a compact,
/// searchable dropdown listing every supported locale. Selecting one applies it
/// and persists the choice. Open/close is driven by a local signal and a
/// full-screen transparent backdrop catches click-outside.
#[component]
fn LanguageToggle(progress: ProgressHandle) -> impl IntoView {
    let open = RwSignal::new(false);
    let query = RwSignal::new(String::new());

    let select = move |next: Locale| {
        apply_locale(next, progress);
        open.set(false);
        query.set(String::new());
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
                // the menu that closes it (and clears the search) when clicked.
                <div
                    class="fixed inset-0 z-40"
                    on:click=move |_| {
                        open.set(false);
                        query.set(String::new());
                    }
                ></div>

                // Dropdown panel. It keeps the compact base width, expands for
                // longer language names, and is capped by the viewport.
                <div
                    class="absolute end-0 mt-2 z-50 max-h-[min(420px,calc(100vh-96px))] flex flex-col p-1 bg-surface border border-border rounded-xl shadow-xl"
                    style:width=move || language_menu_width(&query.get(), 16.0)
                >
                    // Sticky search input at the top of the panel.
                    <div class="shrink-0 px-1 pb-1">
                        <input
                            type="text"
                            prop:value=move || query.get()
                            on:input=move |ev| query.set(event_target_value(&ev))
                            placeholder=move || t("settings.language_search_placeholder")
                            class="w-full h-8 px-2.5 text-sm bg-bg text-content placeholder:text-muted rounded-md border border-border focus:outline-none focus:border-primary"
                        />
                    </div>
                    // Scrollable language list (shared picker).
                    <div class="min-h-0 flex-1 overflow-y-auto p-0.5">
                        <LanguagePicker query=query on_select=Callback::new(select) />
                    </div>
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
