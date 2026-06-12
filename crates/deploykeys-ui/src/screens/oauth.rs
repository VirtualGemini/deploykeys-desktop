//! GitHub device-flow screen. Shows the verification URL and user code with
//! copy / open-in-browser actions, styled after Preline's card + input-group.

use crate::i18n::t;
use leptos::*;

#[component]
pub fn OAuth(
    /// The verification URL and user code are fixed for the lifetime of this
    /// screen (the whole view is re-rendered when the device flow restarts), so
    /// they are plain owned values rather than reactive signals.
    user_code: String,
    verification_uri: String,
    on_open: Callback<String>,
    on_copy: Callback<String>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="min-h-screen w-full flex flex-col items-center justify-center px-6 bg-bg gap-5">
            <div class="w-full max-w-md bg-surface border border-border rounded-2xl shadow-xl p-7 flex flex-col gap-5">
                <div class="flex flex-col gap-1">
                    <h2 class="text-xl font-semibold text-content">{move || t("oauth.title")}</h2>
                    <p class="text-sm text-muted">{move || t("oauth.instruction")}</p>
                </div>

                // Verification URL field with open + copy actions.
                <div class="flex flex-col gap-1.5">
                    <label class="text-xs font-medium text-muted">{move || t("oauth.step_visit")}</label>
                    <div class="flex items-stretch gap-2">
                        <button
                            type="button"
                            class="flex-1 min-w-0 text-left py-2.5 px-3 text-sm rounded-lg border border-border bg-bg text-primary truncate hover:opacity-80 focus:outline-none transition-opacity"
                            on:click={
                                let uri = verification_uri.clone();
                                move |_| on_open.call(uri.clone())
                            }
                        >
                            {verification_uri.clone()}
                        </button>
                        <button
                            type="button"
                            class="shrink-0 py-2.5 px-3 text-xs font-medium rounded-lg border border-border bg-primary-soft text-primary hover:opacity-80 focus:outline-none transition-opacity"
                            on:click={
                                let uri = verification_uri.clone();
                                move |_| on_copy.call(uri.clone())
                            }
                        >
                            {move || t("oauth.copy")}
                        </button>
                    </div>
                </div>

                // User code field with copy action.
                <div class="flex flex-col gap-1.5">
                    <label class="text-xs font-medium text-muted">{move || t("oauth.step_code")}</label>
                    <div class="flex items-center gap-2">
                        <div class="flex-1 py-2.5 px-3 rounded-lg border border-border bg-bg font-mono text-2xl tracking-widest text-content">
                            {user_code.clone()}
                        </div>
                        <button
                            type="button"
                            class="shrink-0 py-2.5 px-3 text-xs font-medium rounded-lg border border-border bg-primary-soft text-primary hover:opacity-80 focus:outline-none transition-opacity"
                            on:click={
                                let code = user_code.clone();
                                move |_| on_copy.call(code.clone())
                            }
                        >
                            {move || t("oauth.copy")}
                        </button>
                    </div>
                </div>

                <div class="flex items-center gap-2 text-sm text-muted">
                    <span class="inline-block size-2 rounded-full bg-primary animate-pulse"></span>
                    {move || t("oauth.waiting")}
                </div>
            </div>

            <button
                type="button"
                class="py-2 px-5 text-sm rounded-lg border border-border text-muted hover:bg-surface focus:outline-none transition-colors"
                on:click=move |_| on_cancel.call(())
            >
                {move || t("common.cancel")}
            </button>
        </div>
    }
}
