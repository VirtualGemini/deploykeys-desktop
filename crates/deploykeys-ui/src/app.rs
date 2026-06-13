//! Root component: owns screen state, bootstraps the persisted session, and
//! drives the GitHub device flow (request code, then poll on an interval).

use crate::api;
use crate::i18n::{self, t, Locale};
use crate::icons::{Icon, IconName};
use crate::screens::oauth::OAuth;
use crate::theme::{self, Theme};
use leptos::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, PartialEq)]
enum Screen {
    Main,
    OAuth {
        user_code: String,
        verification_uri: String,
    },
}

/// A pending device-flow session: the code to poll with and the cadence.
#[derive(Clone)]
struct AuthSession {
    device_code: String,
    interval_secs: u64,
}

#[component]
pub fn App() -> impl IntoView {
    // Provide the reactive locale and theme at the root. `provide_context` must
    // run inside a reactive owner, so they live here rather than in `main`. The
    // startup locale guess comes from the webview language; the persisted
    // preference (if any) overrides it once the session bootstrap below runs.
    // Theme defaults to System (follows the OS prefers-color-scheme).
    i18n::provide_locale(detect_locale());
    theme::provide_theme(Theme::System);

    let screen = RwSignal::new(Screen::Main);
    let signing_in = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let account = RwSignal::new(None::<api::Account>);

    // Bootstrap: load persisted language + session. The app opens directly on
    // the main screen; a found session just populates the account (showing the
    // signed-in identity), it no longer gates which screen is shown.
    spawn_local(async move {
        if let Ok(Some(code)) = api::get_language().await {
            i18n::locale().set(Locale::from_code(&code));
        }
        if let Ok(Some(acct)) = api::get_session().await {
            account.set(Some(acct));
        }
    });

    let start_auth = move |_| {
        if signing_in.get() {
            return;
        }
        signing_in.set(true);
        error.set(None);
        spawn_local(async move {
            match api::start_github_auth().await {
                Ok(code) => {
                    screen.set(Screen::OAuth {
                        user_code: code.user_code.clone(),
                        verification_uri: code.verification_uri.clone(),
                    });
                    signing_in.set(false);
                    poll_loop(
                        AuthSession {
                            device_code: code.device_code,
                            interval_secs: code.interval.max(1),
                        },
                        screen,
                        error,
                        account,
                    );
                }
                Err(e) => {
                    error.set(Some(e));
                    signing_in.set(false);
                }
            }
        });
    };

    let cancel_auth = move |_| {
        screen.set(Screen::Main);
        error.set(None);
    };

    let open_url = move |url: String| {
        spawn_local(async move {
            let _ = api::open_url(&url).await;
        });
    };

    let copy = move |text: String| {
        copy_to_clipboard(&text);
    };

    view! {
        {move || match screen.get() {
            Screen::Main => view! {
                <Main
                    account=account
                    signing_in=signing_in
                    error=error
                    on_sign_in=Callback::new(start_auth)
                />
            }.into_view(),
            Screen::OAuth { user_code, verification_uri } => view! {
                <OAuth
                    user_code=user_code
                    verification_uri=verification_uri
                    on_open=Callback::new(open_url)
                    on_copy=Callback::new(copy)
                    on_cancel=Callback::new(cancel_auth)
                />
            }.into_view(),
        }}
    }
}

/// Schedule one poll after `interval_secs`, handling each outcome and
/// re-scheduling until the flow terminates. The screen signal guards against a
/// cancelled flow resurrecting itself: if the user left the OAuth screen, the
/// loop stops.
fn poll_loop(
    session: AuthSession,
    screen: RwSignal<Screen>,
    error: RwSignal<Option<String>>,
    account: RwSignal<Option<api::Account>>,
) {
    spawn_local(async move {
        sleep_secs(session.interval_secs).await;

        // Bail if the user navigated away (cancelled) while we were waiting.
        if !matches!(screen.get_untracked(), Screen::OAuth { .. }) {
            return;
        }

        match api::poll_github_auth(&session.device_code).await {
            Ok(api::Poll::Pending) => poll_loop(session, screen, error, account),
            Ok(api::Poll::SlowDown) => {
                let next = AuthSession {
                    interval_secs: session.interval_secs + 5,
                    ..session
                };
                poll_loop(next, screen, error, account)
            }
            Ok(api::Poll::Authorized { account: acct }) => {
                account.set(Some(acct));
                screen.set(Screen::Main);
            }
            Err(e) => {
                error.set(Some(e));
                screen.set(Screen::Main);
            }
        }
    });
}

/// The main app screen (repos / connect / keys / forge), with a top nav. The
/// top-right corner shows the signed-in identity + sign out when authenticated,
/// or a "sign in with GitHub" button that starts the device flow otherwise.
#[component]
fn Main(
    account: RwSignal<Option<api::Account>>,
    #[prop(into)] signing_in: Signal<bool>,
    #[prop(into)] error: Signal<Option<String>>,
    on_sign_in: Callback<()>,
) -> impl IntoView {
    let sign_out = move |_| {
        // No backend sign-out command yet; just drop local state.
        account.set(None);
    };

    // Command palette open state (toggled by ⌘K / Ctrl+K, or clicking the
    // header trigger). When open, a full-screen modal overlay with a centered
    // search box + filtered action list appears.
    let palette_open = RwSignal::new(false);

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
                class="flex items-center shrink-0 h-14 pl-20 pr-4 gap-3 bg-surface border-b border-border select-none"
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

                // Right: search box + language toggle + theme toggle. Sizing and
                // shape follow Preline's search-trigger and icon-button specs; the
                // interactions are wired to our own i18n/theme signals.
                <CommandPaletteTrigger on_open=Callback::new(move |_| palette_open.set(true)) />
                <LanguageToggle />
                <ThemeToggle />
            </header>

            // Command palette modal (shown when palette_open is true).
            <CommandPalette open=palette_open />

            // Body: sidebar (left) + content (right).
            <div class="flex-1 flex min-h-0">
                // Left sidebar: vertical section nav at the top, account controls
                // (sign in / signed-in identity + sign out) pinned to the bottom.
                <aside class="shrink-0 w-60 bg-surface border-r border-border flex flex-col overflow-y-auto">
                    <nav class="flex flex-col gap-1 py-3 px-2">
                        <NavItem icon=IconName::Folder label=move || t("nav.repos") active=true />
                        <NavItem icon=IconName::Server label=move || t("nav.connect") active=false />
                        <NavItem icon=IconName::Key label=move || t("nav.keys") active=false />
                        <NavItem icon=IconName::Key label=move || t("nav.forge") active=false />
                    </nav>

                    // Spacer pushes the account block to the bottom.
                    <div class="flex-1"></div>

                    // Account block: bottom of the sidebar, above a divider.
                    <div class="shrink-0 p-3 border-t border-border">
                        {move || match account.get() {
                            Some(acct) => view! {
                                <div class="flex items-center gap-2">
                                    <div class="flex items-center justify-center size-8 shrink-0 rounded-full bg-primary-soft text-primary text-sm font-semibold uppercase">
                                        {acct.login.chars().next().unwrap_or('?').to_string()}
                                    </div>
                                    <span class="flex-1 min-w-0 truncate text-sm text-content">{format!("@{}", acct.login)}</span>
                                    <button
                                        type="button"
                                        title=move || t("common.sign_out")
                                        class="shrink-0 flex justify-center items-center size-8 rounded-lg text-muted hover:bg-bg hover:text-content focus:outline-none transition-colors"
                                        on:click=sign_out
                                    >
                                        <Icon name=IconName::SignOut class="size-4" />
                                    </button>
                                </div>
                            }.into_view(),
                            None => view! {
                                <button
                                    type="button"
                                    class="w-full inline-flex justify-center items-center gap-x-2 py-2 px-4 text-sm font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none disabled:opacity-50 disabled:pointer-events-none transition-colors"
                                    prop:disabled=signing_in
                                    on:click=move |_| on_sign_in.call(())
                                >
                                    <Icon name=IconName::Github class="size-4" />
                                    {move || if signing_in.get() { t("welcome.signing_in") } else { t("welcome.sign_in") }}
                                </button>
                            }.into_view(),
                        }}
                    </div>
                </aside>

                // Right content area — the section content comes in a later phase.
                <main class="flex-1 min-w-0 overflow-y-auto px-8 py-8">
                    <div class="max-w-4xl mx-auto">
                        <h1 class="text-2xl font-semibold text-content">{move || t("nav.repos")}</h1>
                        <p class="mt-2 text-sm text-muted">{move || t("screen.placeholder_phase4")}</p>
                        <Show when=move || error.get().is_some()>
                            <div class="w-full mt-4 p-3 text-sm rounded-lg border border-red-200 bg-red-50 text-red-700 text-left dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                                {move || error.get().unwrap_or_default()}
                            </div>
                        </Show>
                    </div>
                </main>
            </div>
        </div>
    }
}

#[component]
fn NavItem(
    icon: IconName,
    #[prop(into)] label: Signal<&'static str>,
    active: bool,
) -> impl IntoView {
    // Sidebar item: full-width, left-aligned row with icon + label.
    let class = if active {
        "w-full flex items-center gap-2.5 py-2 px-3 text-sm font-medium rounded-lg bg-primary-soft text-primary"
    } else {
        "w-full flex items-center gap-2.5 py-2 px-3 text-sm font-medium rounded-lg text-muted hover:bg-bg hover:text-content transition-colors"
    };
    view! {
        <button type="button" class=class>
            <Icon name=icon class="size-4" />
            <span>{move || label.get()}</span>
        </button>
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
            class="hidden sm:inline-flex items-center justify-between gap-x-3 w-56 h-9 px-2.5 text-sm rounded-lg bg-bg border border-border text-muted hover:bg-surface focus:outline-none transition-colors"
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
/// Commands: nav items (repos/connect/keys/forge), toggle theme/language. Real
/// actions (create key, bind target) land once those screens exist.
#[component]
fn CommandPalette(open: RwSignal<bool>) -> impl IntoView {
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
    let all_commands = move || -> Vec<(&'static str, IconName, Box<dyn Fn()>)> {
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
            ("nav.forge", IconName::Key, Box::new(|| {})),
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
                    spawn_local(async move {
                        let _ = api::set_language(code).await;
                    });
                    open.set(false);
                }),
            ),
        ]
    };

    // Filtered commands: if query empty, show history; else substring match.
    let filtered = move || -> Vec<(String, Option<IconName>, Box<dyn Fn()>)> {
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
                                            class="flex items-center gap-3 py-2.5 px-3 rounded-lg text-sm cursor-pointer transition-colors"
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
            class="shrink-0 flex justify-center items-center size-9 rounded-lg text-muted hover:bg-bg hover:text-content focus:outline-none transition-colors"
            on:click=move |_| on_click.call(())
        >
            {children()}
        </button>
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
fn LanguageToggle() -> impl IntoView {
    let locale = i18n::locale();
    let open = RwSignal::new(false);

    let select = move |next: Locale| {
        locale.set(next);
        open.set(false);
        let code = next.code();
        spawn_local(async move {
            let _ = api::set_language(code).await;
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
                                class="w-full flex items-center gap-x-3 py-2 px-2.5 rounded-lg text-sm text-content hover:bg-bg focus:outline-none focus:bg-bg transition-colors"
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

/// Read the webview's language (e.g. `zh-CN`) and map it to a supported locale.
/// The persisted preference, if any, overrides this during bootstrap.
fn detect_locale() -> Locale {
    web_sys::window()
        .and_then(|w| w.navigator().language())
        .map(|code| Locale::from_code(&code))
        .unwrap_or(Locale::En)
}

/// Browser clipboard write via the async Clipboard API. Best-effort.
fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let _ = clipboard.write_text(text);
    }
}

/// Resolve after roughly `secs` seconds using `setTimeout`.
async fn sleep_secs(secs: u64) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let window = web_sys::window().expect("window exists");
        let _ = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, (secs * 1000) as i32);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
