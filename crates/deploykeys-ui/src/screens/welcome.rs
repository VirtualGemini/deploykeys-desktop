//! Sign-in landing screen. A centered hero card with the brand, tagline, and a
//! GitHub sign-in button styled after Preline's solid-button component.

use crate::i18n::t;
use leptos::*;

#[component]
pub fn Welcome(
    /// True while the device code request is in flight; disables the button.
    #[prop(into)]
    signing_in: Signal<bool>,
    /// Any error to surface beneath the button.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Fired when the user clicks sign in.
    on_sign_in: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="min-h-screen w-full flex flex-col items-center justify-center px-6 bg-gradient-to-b from-white to-slate-50">
            <div class="w-full max-w-md flex flex-col items-center text-center gap-6">
                <div class="flex flex-col items-center gap-3">
                    <div class="flex items-center justify-center size-16 rounded-2xl bg-blue-600/10 text-blue-600">
                        <svg class="size-9" viewBox="0 0 24 24" fill="currentColor" xmlns="http://www.w3.org/2000/svg">
                            <path d="M12 .5C5.73.5.5 5.73.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.56 0-.28-.01-1.02-.02-2-3.2.7-3.88-1.54-3.88-1.54-.52-1.33-1.28-1.69-1.28-1.69-1.05-.72.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.77 2.7 1.26 3.36.96.1-.75.4-1.26.73-1.55-2.55-.29-5.23-1.28-5.23-5.69 0-1.26.45-2.29 1.19-3.1-.12-.29-.52-1.46.11-3.05 0 0 .97-.31 3.18 1.18a11.1 11.1 0 0 1 2.9-.39c.98 0 1.97.13 2.9.39 2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.76.11 3.05.74.81 1.19 1.84 1.19 3.1 0 4.42-2.69 5.39-5.25 5.68.41.36.78 1.06.78 2.14 0 1.55-.01 2.8-.01 3.18 0 .31.21.68.8.56A11.51 11.51 0 0 0 23.5 12C23.5 5.73 18.27.5 12 .5z"/>
                        </svg>
                    </div>
                    <h1 class="text-3xl font-semibold text-slate-800">{move || t("app.brand")}</h1>
                    <p class="text-sm text-slate-500">{move || t("app.tagline")}</p>
                </div>

                <button
                    type="button"
                    class="w-full inline-flex justify-center items-center gap-x-2 py-3 px-4 text-sm font-medium rounded-xl border border-transparent bg-blue-600 text-white shadow-sm hover:bg-blue-700 focus:outline-none focus:bg-blue-700 disabled:opacity-50 disabled:pointer-events-none transition-colors"
                    prop:disabled=signing_in
                    on:click=move |_| on_sign_in.call(())
                >
                    {move || if signing_in.get() { t("welcome.signing_in") } else { t("welcome.sign_in") }}
                </button>

                <Show when=move || error.get().is_some()>
                    <div class="w-full mt-1 p-3 text-sm rounded-lg border border-red-200 bg-red-50 text-red-700 text-left">
                        {move || error.get().unwrap_or_default()}
                    </div>
                </Show>
            </div>
        </div>
    }
}
