//! Personal Access Token sign-in screen. The user pastes a fine-grained PAT;
//! we hand it to the backend, which validates it (`GET /user`) and stores it in
//! the OS keyring. A button opens GitHub's token-creation page.

use crate::i18n::t;
use leptos::*;

/// GitHub's fine-grained PAT creation page.
const TOKEN_CREATE_URL: &str = "https://github.com/settings/personal-access-tokens/new";

#[component]
pub fn SignIn(
    #[prop(into)] signing_in: Signal<bool>,
    #[prop(into)] error: Signal<Option<String>>,
    on_submit: Callback<String>,
    on_open: Callback<String>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let token = RwSignal::new(String::new());

    let submit = move || on_submit.call(token.get_untracked());

    view! {
        <div class="min-h-screen w-full flex flex-col items-center justify-center px-6 bg-bg gap-5">
            <div class="w-full max-w-md bg-surface border border-border rounded-2xl shadow-xl p-7 flex flex-col gap-5">
                <div class="flex flex-col gap-1">
                    <h2 class="text-xl font-semibold text-content">{move || t("signin.title")}</h2>
                    <p class="text-sm text-muted">{move || t("signin.instruction")}</p>
                </div>

                <div class="flex flex-col gap-1.5">
                    <label class="text-xs font-medium text-muted">{move || t("signin.token_label")}</label>
                    <input
                        type="password"
                        class="w-full py-2.5 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none font-mono"
                        placeholder="github_pat_..."
                        prop:value=move || token.get()
                        on:input=move |ev| token.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" {
                                submit();
                            }
                        }
                    />
                    <div class="text-xs text-muted space-y-1.5">
                        <p>
                            <span class="font-medium text-content">"Resource owner"</span>
                            <span>": "{move || t("signin.resource_owner_help")}</span>
                        </p>
                        <p>
                            <span class="font-medium text-content">"Repository access"</span>
                            <span>": "{move || t("signin.repository_access_help")}</span>
                        </p>
                        <p>
                            <span class="font-medium text-content">"Permissions"</span>
                            <span>": "{move || t("signin.permissions_help")}</span>
                        </p>
                    </div>
                    <button
                        type="button"
                        class="self-start text-xs text-primary hover:opacity-80 focus:outline-none transition-opacity"
                        on:click=move |_| on_open.call(TOKEN_CREATE_URL.to_string())
                    >
                        {move || t("signin.create_token")}
                    </button>
                </div>

                <Show when=move || error.get().is_some()>
                    <div class="w-full p-3 text-sm rounded-lg border border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                        {move || error.get().unwrap_or_default()}
                    </div>
                </Show>

                <button
                    type="button"
                    class="w-full py-2.5 px-4 text-sm font-medium rounded-lg bg-primary text-white hover:opacity-80 focus:outline-none transition-opacity disabled:opacity-50"
                    prop:disabled=move || signing_in.get()
                    on:click=move |_| submit()
                >
                    {move || t("signin.submit")}
                </button>
            </div>

            <button
                type="button"
                class="py-2 px-5 text-sm rounded-lg border border-border text-muted hover:bg-surface focus:outline-none"
                on:click=move |_| on_cancel.call(())
            >
                {move || t("common.cancel")}
            </button>
        </div>
    }
}
