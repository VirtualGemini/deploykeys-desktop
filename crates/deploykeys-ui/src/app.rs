//! Root component: owns screen state, bootstraps the persisted session, and
//! drives the GitHub device flow (request code, then poll on an interval).

use crate::api;
use crate::i18n::{self, t, Locale};
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

/// The main app screen (repos / targets / keys / forge), with a top nav. The
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

    view! {
        <div class="min-h-screen w-full bg-bg text-content">
            <header class="sticky top-0 inset-x-0 z-50 bg-surface border-b border-border">
                <nav class="max-w-4xl mx-auto flex items-center justify-between px-6 h-14">
                    <div class="flex items-center gap-1">
                        <NavItem label=move || t("nav.repos") active=true />
                        <NavItem label=move || t("nav.targets") active=false />
                        <NavItem label=move || t("nav.keys") active=false />
                        <NavItem label=move || t("nav.forge") active=false />
                    </div>
                    <div class="flex items-center gap-3">
                        {move || match account.get() {
                            Some(acct) => view! {
                                <span class="text-sm text-muted">{format!("@{}", acct.login)}</span>
                                <button
                                    type="button"
                                    class="py-1.5 px-3 text-xs font-medium rounded-lg border border-border text-muted hover:bg-bg focus:outline-none transition-colors"
                                    on:click=sign_out
                                >
                                    {move || t("common.sign_out")}
                                </button>
                            }.into_view(),
                            None => view! {
                                <button
                                    type="button"
                                    class="py-1.5 px-3 text-xs font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none disabled:opacity-50 disabled:pointer-events-none transition-colors"
                                    prop:disabled=signing_in
                                    on:click=move |_| on_sign_in.call(())
                                >
                                    {move || if signing_in.get() { t("welcome.signing_in") } else { t("welcome.sign_in") }}
                                </button>
                            }.into_view(),
                        }}
                    </div>
                </nav>
            </header>
            <main class="max-w-4xl mx-auto px-6 py-12">
                <h1 class="text-2xl font-semibold text-content">{move || t("nav.repos")}</h1>
                <p class="mt-2 text-sm text-muted">{move || t("screen.placeholder_phase4")}</p>
                <Show when=move || error.get().is_some()>
                    <div class="w-full mt-4 p-3 text-sm rounded-lg border border-red-200 bg-red-50 text-red-700 text-left dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                        {move || error.get().unwrap_or_default()}
                    </div>
                </Show>
            </main>
        </div>
    }
}

#[component]
fn NavItem(#[prop(into)] label: Signal<&'static str>, active: bool) -> impl IntoView {
    let class = if active {
        "py-1.5 px-3 text-sm font-medium rounded-lg bg-bg text-content"
    } else {
        "py-1.5 px-3 text-sm font-medium rounded-lg text-muted hover:bg-bg transition-colors"
    };
    view! { <button type="button" class=class>{move || label.get()}</button> }
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
