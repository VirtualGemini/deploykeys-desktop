//! SSH Keys management screen.
//!
//! A standalone local SSH key manager: create SSH key pairs with Ed25519/RSA,
//! list them in a table, copy public keys, and delete. Keys are stored in
//! `~/.ssh/deploykeys/<directory>/` with isolated directories per key.

use crate::api::{self, SshKey};
use crate::i18n::t;
use crate::icons::{Icon, IconName};
use crate::page_size::page_size;
use crate::progress::ProgressHandle;
use leptos::*;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn Keys(#[allow(unused_variables)] pending_count: RwSignal<usize>) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let keys = RwSignal::new(Vec::<SshKey>::new());
    let loading = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let query = RwSignal::new(String::new());
    let algorithm_filter = RwSignal::new(String::new());
    let created_from = RwSignal::new(String::new());
    let created_to = RwSignal::new(String::new());
    let page = RwSignal::new(1_usize);
    let missing_key_confirm = RwSignal::new(None::<i64>);

    // SSH keys are purely local — load them unconditionally on mount, with no
    // sign-in dependency.
    create_effect(move |_| {
        loading.set(true);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::list_ssh_keys().await {
                Ok(list) => keys.set(list),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
            progress.end_simulated(&sim);
        });
    });

    let refresh = move || {
        if loading.get_untracked() {
            return;
        }
        loading.set(true);
        error.set(None);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::list_ssh_keys().await {
                Ok(list) => keys.set(list),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
            progress.end_simulated(&sim);
        });
    };

    // Filtered keys by search query
    let filtered = Signal::derive(move || {
        let q = query.get().to_lowercase();
        let algorithm = algorithm_filter.get();
        let from = created_from.get();
        let to = created_to.get();
        keys.get()
            .into_iter()
            .filter(|k| {
                let matches_query = q.is_empty()
                    || k.directory.to_lowercase().contains(&q)
                    || k.comment.to_lowercase().contains(&q)
                    || k.remark.to_lowercase().contains(&q);
                let matches_algorithm = algorithm.is_empty() || k.algorithm == algorithm;
                let key_date = k.created_at.get(..10).unwrap_or("");
                let matches_from = from.is_empty() || key_date >= from.as_str();
                let matches_to = to.is_empty() || key_date <= to.as_str();

                matches_query && matches_algorithm && matches_from && matches_to
            })
            .collect::<Vec<_>>()
    });

    let filtered_count = Signal::derive(move || filtered.get().len());
    let page_count = Signal::derive(move || {
        let count = filtered_count.get();
        let size = page_size().get().max(1);
        (count + size - 1) / size
    });

    let safe_page = Signal::derive(move || page.get().clamp(1, page_count.get().max(1)));
    let paged = Signal::derive(move || {
        let size = page_size().get().max(1);
        let start = (safe_page.get() - 1) * size;
        filtered
            .get()
            .into_iter()
            .skip(start)
            .take(size)
            .collect::<Vec<_>>()
    });

    // Correct the page index when filters or the shared page size change.
    create_effect(move |_| {
        let max = page_count.get().max(1);
        page.update(|p| *p = (*p).clamp(1, max));
    });

    // Persist page size changes to the backend, matching the repository page.
    create_effect(move |_| {
        let size = page_size().get();
        spawn_local(async move {
            if let Err(e) = api::set_page_size(size).await {
                leptos::logging::warn!("Failed to persist page size: {e}");
            }
        });
    });

    let create_dialog_open = RwSignal::new(false);

    let algorithm_options = Signal::derive(move || {
        vec![
            (String::new(), t("keys.filter_algorithm_all").to_string()),
            ("ed25519".to_string(), "Ed25519".to_string()),
            ("rsa2048".to_string(), "RSA 2048".to_string()),
            ("rsa4096".to_string(), "RSA 4096".to_string()),
        ]
    });

    let set_query = move |value: String| {
        query.set(value);
        page.set(1);
    };
    let set_algorithm_filter = move |value: String| {
        algorithm_filter.set(value);
        page.set(1);
    };
    let reset_page = Callback::new(move |_| page.set(1));

    let copy_public_key = move |id: i64| {
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::ssh_key_files_exist(id).await {
                Ok(true) => {}
                Ok(false) => {
                    missing_key_confirm.set(Some(id));
                    progress.end_simulated(&sim);
                    return;
                }
                Err(e) => {
                    error.set(Some(e));
                    progress.end_simulated(&sim);
                    return;
                }
            }

            match api::get_public_key_content(id).await {
                Ok(content) => {
                    // Copy to clipboard using the Clipboard API
                    if let Some(window) = web_sys::window() {
                        let navigator = window.navigator().clipboard();
                        let promise = navigator.write_text(&content);
                        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                        leptos::logging::log!("Public key copied to clipboard");
                    }
                }
                Err(e) => {
                    leptos::logging::error!("Failed to read public key: {}", e);
                    error.set(Some(e));
                }
            }
            progress.end_simulated(&sim);
        });
    };

    let delete_key = move |id: i64| {
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::delete_ssh_key(id).await {
                Ok(_) => {
                    refresh();
                }
                Err(e) => {
                    leptos::logging::error!("Failed to delete key: {}", e);
                    error.set(Some(e));
                }
            }
            progress.end_simulated(&sim);
        });
    };

    view! {
        <div class="flex flex-col gap-5 h-full">
            <div class="flex items-center justify-between gap-3">
                <h1 class="text-2xl font-semibold text-content">{move || t("keys.title")}</h1>
                <button
                    type="button"
                    class="shrink-0 py-2 px-4 text-sm font-medium rounded-lg border border-border bg-primary text-on-primary hover:bg-primary-hover focus:outline-none transition-colors disabled:opacity-50"
                    on:click=move |_| create_dialog_open.set(true)
                >
                    {move || t("keys.create")}
                </button>
            </div>

            <div class="flex flex-wrap items-center gap-2">
                <input
                    type="text"
                    class="flex-1 min-w-[12rem] py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none"
                    placeholder=move || t("keys.search_placeholder")
                    prop:value=move || query.get()
                    on:input=move |ev| set_query(event_target_value(&ev))
                />
                <FilterDropdown
                    options=algorithm_options
                    selected=Signal::derive(move || algorithm_filter.get())
                    on_select=Callback::new(set_algorithm_filter)
                />
                <DateRangeDropdown
                    from=created_from
                    to=created_to
                    on_change=reset_page
                />
            </div>

                <Show when=move || error.get().is_some()>
                    <div class="w-full p-3 text-sm rounded-lg border border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                        {move || error.get().unwrap_or_default()}
                    </div>
                </Show>

                <Show
                    when=move || !loading.get()
                    fallback=move || view! { <div class="min-h-24" aria-hidden="true"></div> }
                >
                    <Show
                        when=move || !keys.get().is_empty()
                        fallback=move || view! { <p class="text-sm text-muted">{move || t("keys.empty")}</p> }
                    >
                        <Show
                            when=move || !filtered.get().is_empty()
                        fallback=move || view! { <p class="text-sm text-muted">{move || t("keys.no_match")}</p> }
                    >
                            <div class="flex flex-col gap-4 flex-1 min-h-0">
                                <div class="flex-1 overflow-auto min-h-0 rounded-lg border border-border bg-surface">
                                    <table class="w-full border-collapse text-sm">
                                        <thead class="sticky top-0 z-10 bg-surface">
                                            <tr class="border-b border-border">
                                                <th class="text-start font-medium text-muted px-4 py-2.5 whitespace-nowrap">
                                                    {move || t("keys.directory")}
                                                </th>
                                                <th class="text-start font-medium text-muted px-4 py-2.5 whitespace-nowrap">
                                                    {move || t("keys.algorithm")}
                                                </th>
                                                <th class="text-start font-medium text-muted px-4 py-2.5 whitespace-nowrap">
                                                    {move || t("keys.comment")}
                                                </th>
                                                <th class="text-start font-medium text-muted px-4 py-2.5 whitespace-nowrap">
                                                    {move || t("keys.remark")}
                                                </th>
                                                <th class="text-start font-medium text-muted px-4 py-2.5 whitespace-nowrap">
                                                    {move || t("keys.created_at")}
                                                </th>
                                                <th class="text-end font-medium text-muted px-4 py-2.5 whitespace-nowrap">
                                                    {move || t("keys.actions")}
                                                </th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            <For
                                                each=move || paged.get()
                                                key=|k| k.id
                                                children=move |key| {
                                                let key_id = key.id;
                                                let key_directory = key.directory.clone();
                                                let directory = key.directory.clone();
                                                let algorithm = key.algorithm.clone();
                                                let comment = key.comment.clone();
                                                let created_at = key.created_at.clone();
                                                let remark = key.remark.clone();
                                                let remark_for_dialog = remark.clone();
                                                let delete_confirm_open = RwSignal::new(false);
                                                let edit_open = RwSignal::new(false);
                                                view! {
                                                    <tr class="border-b border-border last:border-b-0 hover:bg-bg align-top">
                                                        <td class="px-4 py-3">
                                                            <span class="font-medium text-content font-mono break-all">{directory}</span>
                                                        </td>
                                                        <td class="px-4 py-3 whitespace-nowrap">
                                                            <span class="text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">
                                                                {algorithm}
                                                            </span>
                                                        </td>
                                                        <td class="px-4 py-3">
                                                            <span class="text-content font-mono break-all">{comment}</span>
                                                        </td>
                                                        <td class="px-4 py-3 max-w-[20rem]">
                                                            <span class="text-muted break-words line-clamp-2">
                                                                {if remark.is_empty() { "—".to_string() } else { remark }}
                                                            </span>
                                                        </td>
                                                        <td class="px-4 py-3 text-muted whitespace-nowrap">{created_at}</td>
                                                        <td class="px-4 py-3">
                                                            <div class="inline-flex gap-2">
                                                                <button
                                                                    type="button"
                                                                    class="py-1 px-3 text-xs font-medium rounded-md text-primary hover:bg-primary-soft focus:outline-none"
                                                                    on:click=move |_| copy_public_key(key_id)
                                                                >
                                                                    {move || t("keys.copy_public_key")}
                                                                </button>
                                                                <button
                                                                    type="button"
                                                                    class="py-1 px-3 text-xs font-medium rounded-md text-content hover:bg-bg focus:outline-none"
                                                                    on:click=move |_| edit_open.set(true)
                                                                >
                                                                    {move || t("keys.edit")}
                                                                </button>
                                                                <button
                                                                    type="button"
                                                                    class="py-1 px-3 text-xs font-medium rounded-md text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950 focus:outline-none"
                                                                    on:click=move |_| delete_confirm_open.set(true)
                                                                >
                                                                    {move || t("keys.delete")}
                                                                </button>
                                                            </div>
                                                        </td>
                                                    </tr>

                                                    <Show when=move || delete_confirm_open.get()>
                                                        <div class="fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4">
                                                            <div class="w-full max-w-md bg-surface border border-border rounded-xl shadow-2xl overflow-hidden">
                                                                <div class="px-6 py-5">
                                                                    <h2 class="text-base font-semibold text-content">{move || t("keys.delete_confirm_title")}</h2>
                                                                    <p class="mt-2 text-sm text-muted">{move || t("keys.delete_confirm_message")}</p>
                                                                </div>
                                                                <div class="flex justify-end gap-2 px-6 py-4 border-t border-border">
                                                                    <button
                                                                        type="button"
                                                                        class="px-4 py-2 text-sm font-medium rounded-lg bg-bg text-content hover:text-primary focus:outline-none"
                                                                        on:click=move |_| delete_confirm_open.set(false)
                                                                    >
                                                                        {move || t("common.cancel")}
                                                                    </button>
                                                                    <button
                                                                        type="button"
                                                                        class="px-4 py-2 text-sm font-medium rounded-lg bg-red-600 text-white hover:bg-red-700 focus:outline-none"
                                                                        on:click=move |_| {
                                                                            delete_confirm_open.set(false);
                                                                            delete_key(key_id);
                                                                        }
                                                                    >
                                                                        {move || t("common.confirm")}
                                                                    </button>
                                                                </div>
                                                            </div>
                                                        </div>
                                                    </Show>

                                                    <EditKeyDialog
                                                        open=edit_open
                                                        key_id=key_id
                                                        current_directory=key_directory
                                                        current_remark=remark_for_dialog
                                                        on_missing=Callback::new(move |_| missing_key_confirm.set(Some(key_id)))
                                                        on_done=Callback::new(move |_| refresh())
                                                    />
                                                }
                                            }
                                            />
                                        </tbody>
                                    </table>
                                </div>
                                <PaginationBar
                                    page=page
                                    page_count=page_count
                                    total=filtered_count
                                />
                            </div>
                        </Show>
                    </Show>
                </Show>

            <CreateKeyDialog
                open=create_dialog_open
                on_created=Callback::new(move |_| refresh())
            />

            <Show when=move || missing_key_confirm.get().is_some()>
                <div class="fixed inset-0 z-[110] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4">
                    <div class="w-full max-w-md bg-surface border border-border rounded-xl shadow-2xl overflow-hidden">
                        <div class="px-6 py-5">
                            <h2 class="text-base font-semibold text-content">{move || t("keys.missing_confirm_title")}</h2>
                            <p class="mt-2 text-sm text-muted">{move || t("keys.missing_confirm_message")}</p>
                        </div>
                        <div class="flex justify-end gap-2 px-6 py-4 border-t border-border">
                            <button
                                type="button"
                                class="px-4 py-2 text-sm font-medium rounded-lg bg-bg text-content hover:text-primary focus:outline-none"
                                on:click=move |_| missing_key_confirm.set(None)
                            >
                                {move || t("common.cancel")}
                            </button>
                            <button
                                type="button"
                                class="px-4 py-2 text-sm font-medium rounded-lg bg-red-600 text-white hover:bg-red-700 focus:outline-none"
                                on:click=move |_| {
                                    if let Some(id) = missing_key_confirm.get_untracked() {
                                        missing_key_confirm.set(None);
                                        delete_key(id);
                                    }
                                }
                            >
                                {move || t("common.confirm")}
                            </button>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn FilterDropdown(
    #[prop(into)] options: Signal<Vec<(String, String)>>,
    #[prop(into)] selected: Signal<String>,
    on_select: Callback<String>,
) -> impl IntoView {
    let open = RwSignal::new(false);

    let label = Signal::derive(move || {
        let sel = selected.get();
        options
            .get()
            .into_iter()
            .find(|(value, _)| *value == sel)
            .map(|(_, display)| display)
            .unwrap_or_default()
    });

    view! {
        <div class="relative">
            <button
                type="button"
                class="inline-flex items-center justify-center w-36 h-10 px-3 text-sm rounded-lg border border-border bg-bg text-content hover:bg-surface focus:outline-none whitespace-nowrap"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <span class="min-w-0 truncate">{move || label.get()}</span>
            </button>

            <Show when=move || open.get()>
                <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                <div class="absolute start-0 mt-1 z-50 w-max max-h-80 overflow-y-auto p-1 bg-surface border border-border rounded-xl shadow-xl">
                    <For
                        each=move || options.get()
                        key=|(value, _)| value.clone()
                        children=move |(value, display)| {
                            let is_selected = {
                                let value = value.clone();
                                move || selected.get() == value
                            };
                            let choose = {
                                let value = value.clone();
                                move |_| {
                                    on_select.call(value.clone());
                                    open.set(false);
                                }
                            };
                            view! {
                                <button
                                    type="button"
                                    class="w-full flex items-center gap-x-2 py-1 px-2.5 rounded-lg text-sm text-content hover:bg-bg focus:outline-none focus:bg-bg whitespace-nowrap"
                                    on:click=choose
                                >
                                    {move || {
                                        let class = if is_selected() {
                                            "size-3.5 text-primary"
                                        } else {
                                            "size-3.5 text-primary opacity-0"
                                        };
                                        view! { <Icon name=IconName::Check class=class /> }
                                    }}
                                    <span class="grow text-left">{display}</span>
                                </button>
                            }
                        }
                    />
                </div>
            </Show>
        </div>
    }
}

#[component]
fn DateRangeDropdown(
    from: RwSignal<String>,
    to: RwSignal<String>,
    on_change: Callback<()>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let label = Signal::derive(move || {
        let from_value = from.get();
        let to_value = to.get();
        match (from_value.is_empty(), to_value.is_empty()) {
            (true, true) => t("keys.filter_date_all").to_string(),
            (false, true) => format!("{} {}", t("keys.filter_created_from"), from_value),
            (true, false) => format!("{} {}", t("keys.filter_created_to"), to_value),
            (false, false) => format!("{} - {}", from_value, to_value),
        }
    });

    let clear = move |_| {
        from.set(String::new());
        to.set(String::new());
        on_change.call(());
    };

    view! {
        <div class="relative">
            <button
                type="button"
                class="inline-flex items-center justify-center w-44 h-10 px-3 text-sm rounded-lg border border-border bg-bg text-content hover:bg-surface focus:outline-none whitespace-nowrap"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <span class="min-w-0 truncate">{move || label.get()}</span>
            </button>

            <Show when=move || open.get()>
                <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                <div class="absolute start-0 mt-1 z-50 w-72 p-3 bg-surface border border-border rounded-xl shadow-xl">
                    <div class="grid gap-3">
                        <label class="grid gap-1.5">
                            <span class="text-xs text-muted">{move || t("keys.filter_created_from")}</span>
                            <input
                                type="date"
                                class="w-full h-9 px-2.5 text-sm rounded-lg border border-border bg-bg text-content focus:outline-none focus:ring-1 focus:ring-primary"
                                prop:value=move || from.get()
                                on:input=move |ev| {
                                    from.set(event_target_value(&ev));
                                    on_change.call(());
                                }
                            />
                        </label>
                        <label class="grid gap-1.5">
                            <span class="text-xs text-muted">{move || t("keys.filter_created_to")}</span>
                            <input
                                type="date"
                                class="w-full h-9 px-2.5 text-sm rounded-lg border border-border bg-bg text-content focus:outline-none focus:ring-1 focus:ring-primary"
                                prop:value=move || to.get()
                                on:input=move |ev| {
                                    to.set(event_target_value(&ev));
                                    on_change.call(());
                                }
                            />
                        </label>
                    </div>
                    <div class="flex justify-between gap-2 mt-3 pt-3 border-t border-border">
                        <button
                            type="button"
                            class="px-3 py-1.5 text-sm rounded-lg text-muted hover:bg-bg focus:outline-none"
                            on:click=clear
                        >
                            {move || t("search.clear")}
                        </button>
                        <button
                            type="button"
                            class="px-3 py-1.5 text-sm rounded-lg bg-primary-soft text-primary hover:opacity-80 focus:outline-none"
                            on:click=move |_| open.set(false)
                        >
                            {move || t("common.confirm")}
                        </button>
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Shared classes for pager icon buttons (prev/next).
const PAGER_CELL: &str = "inline-flex items-center justify-center min-w-[2rem] h-8 px-2 text-sm rounded-md text-muted hover:bg-surface hover:text-content disabled:opacity-40 disabled:pointer-events-none";

/// Full pagination bar below the list: total, prev/next with page indicator,
/// page-size, and "go to page" input. Kept in sync with the Repos page.
#[component]
fn PaginationBar(
    page: RwSignal<usize>,
    page_count: Signal<usize>,
    total: Signal<usize>,
) -> impl IntoView {
    let safe_page = Signal::derive(move || page.get().clamp(1, page_count.get().max(1)));
    let at_first = Signal::derive(move || safe_page.get() <= 1);
    let at_last = Signal::derive(move || safe_page.get() >= page_count.get().max(1));
    let page_count_max = Signal::derive(move || page_count.get().max(1));

    let go = move |p: usize| {
        let max = page_count_max.get_untracked();
        page.set(p.clamp(1, max));
    };

    let goto_value = RwSignal::new(String::new());

    let submit_goto = move |_| {
        let Ok(n) = goto_value.get_untracked().trim().parse::<usize>() else {
            goto_value.set(String::new());
            return;
        };
        let max = page_count_max.get_untracked();
        page.set(n.clamp(1, max));
        goto_value.set(String::new());
    };

    view! {
        <div class="shrink-0 flex flex-wrap items-center justify-center gap-x-4 gap-y-3 min-h-16 border-t border-border">
            <span class="text-sm text-muted">
                {move || t("repos.total").replace("{}", &total.get().to_string())}
            </span>

            <div class="flex items-center gap-1.5">
                <button
                    type="button"
                    class=PAGER_CELL
                    prop:disabled=move || at_first.get()
                    on:click=move |_| go(safe_page.get_untracked().saturating_sub(1).max(1))
                >
                    <Icon name=IconName::ChevronLeft class="size-4" />
                </button>

                <span class="text-sm text-muted select-none">
                    {move || safe_page.get()}
                </span>

                <button
                    type="button"
                    class=PAGER_CELL
                    prop:disabled=move || at_last.get()
                    on:click=move |_| go(safe_page.get_untracked() + 1)
                >
                    <Icon name=IconName::ChevronRight class="size-4" />
                </button>
            </div>

            <PageSizeSelector />

            <div class="flex items-center gap-1.5">
                <span class="text-sm text-muted">
                    {move || t("repos.go_to_page_before")}
                </span>
                <input
                    type="text"
                    class="w-12 h-8 px-2 text-sm text-center rounded-md border border-border bg-bg text-content focus:outline-none focus:ring-1 focus:ring-primary"
                    prop:value=move || goto_value.get()
                    on:input=move |ev| goto_value.set(event_target_value(&ev))
                    on:keydown=submit_goto
                />
                <span class="text-sm text-muted">
                    {move || t("repos.go_to_page_after")}
                </span>
            </div>
        </div>
    }
}

/// Page-size control copied from the Repos page so both list screens share the
/// same pagination behavior and persisted global page-size preference.
#[component]
fn PageSizeSelector() -> impl IntoView {
    let presets = [5usize, 10, 20];
    let current = page_size();
    let open = RwSignal::new(false);

    view! {
        <div class="flex items-center gap-1.5">
            <span class="text-sm text-muted">{move || t("repos.page_size")}</span>

            <div class="relative">
                <button
                    type="button"
                    class="inline-flex items-center justify-center gap-1 min-w-[3.25rem] h-8 px-2.5 text-sm rounded-md text-content hover:bg-surface focus:outline-none"
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    <span>{move || current.get()}</span>
                </button>

                <Show when=move || open.get()>
                    <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                    <div class="absolute start-0 bottom-full mb-1 z-50 w-full min-w-[3.25rem] p-1 bg-surface border border-border rounded-xl shadow-xl">
                        {presets
                            .iter()
                            .map(|&size| {
                                let is_active = move || current.get() == size;
                                view! {
                                    <button
                                        type="button"
                                        class=move || {
                                            if is_active() {
                                                "w-full flex items-center justify-center h-8 px-2.5 rounded-lg text-sm bg-primary-soft text-primary"
                                            } else {
                                                "w-full flex items-center justify-center h-8 px-2.5 rounded-lg text-sm text-content hover:bg-bg"
                                            }
                                        }
                                        on:click=move |_| {
                                            current.set(size);
                                            open.set(false);
                                        }
                                    >
                                        {size}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                </Show>
            </div>

            <span class="text-sm text-muted">{move || t("repos.page_size_unit")}</span>
        </div>
    }
}

#[component]
fn CreateKeyDialog(open: RwSignal<bool>, on_created: Callback<()>) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let directory = RwSignal::new(String::new());
    let comment = RwSignal::new(String::new());
    let remark = RwSignal::new(String::new());
    let algorithm = RwSignal::new("ed25519".to_string());
    let creating = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Reset form when dialog opens
    create_effect(move |_| {
        if open.get() {
            directory.set(String::new());
            comment.set(String::new());
            remark.set(String::new());
            algorithm.set("ed25519".to_string());
            error.set(None);
        }
    });

    let submit = move |_| {
        let directory_val = directory.get_untracked().trim().to_string();
        let comment_val = comment.get_untracked().trim().to_string();
        let remark_val = remark.get_untracked().trim().to_string();
        let algorithm_val = algorithm.get_untracked();

        if directory_val.is_empty() || comment_val.is_empty() {
            error.set(Some(t("keys.required").to_string()));
            return;
        }

        creating.set(true);
        error.set(None);
        let sim = progress.begin_simulated();

        spawn_local(async move {
            match api::create_ssh_key(directory_val, algorithm_val, comment_val, remark_val).await {
                Ok(_) => {
                    open.set(false);
                    on_created.call(());
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            creating.set(false);
            progress.end_simulated(&sim);
        });
    };

    view! {
        <div class=move || {
            if open.get() {
                "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-100 transition-opacity duration-300"
            } else {
                "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-0 pointer-events-none transition-opacity duration-300"
            }
        }>
            <div
                class=move || {
                    if open.get() {
                        "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-100 transition-transform duration-300"
                    } else {
                        "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-95 transition-transform duration-300"
                    }
                }
                on:click=|ev| ev.stop_propagation()
            >
                <div class="px-6 py-5 border-b border-border">
                    <h2 class="text-base font-semibold text-content">{move || t("keys.dialog_title_create")}</h2>
                </div>

                    <div class="px-6 py-5 space-y-4">
                        <Show when=move || error.get().is_some()>
                            <div class="p-3 text-sm rounded-lg border border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                                {move || error.get().unwrap_or_default()}
                            </div>
                        </Show>

                        // Directory
                        <div>
                            <label class="block text-sm font-medium text-content mb-1.5">
                                {move || t("keys.dialog_directory_label")}
                            </label>
                            <input
                                type="text"
                                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary font-mono"
                                placeholder=move || t("keys.dialog_directory_placeholder")
                                prop:value=move || directory.get()
                                on:input=move |ev| directory.set(event_target_value(&ev))
                                prop:disabled=move || creating.get()
                            />
                            <p class="mt-1.5 text-xs text-muted">{move || t("keys.dialog_directory_help")}</p>
                        </div>

                        // Key Comment
                        <div>
                            <label class="block text-sm font-medium text-content mb-1.5">
                                {move || t("keys.dialog_comment_label")}
                            </label>
                            <input
                                type="email"
                                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary font-mono"
                                placeholder=move || t("keys.dialog_comment_placeholder")
                                prop:value=move || comment.get()
                                on:input=move |ev| comment.set(event_target_value(&ev))
                                prop:disabled=move || creating.get()
                            />
                            <p class="mt-1.5 text-xs text-muted">{move || t("keys.dialog_comment_help")}</p>
                        </div>

                        // Remark
                        <div>
                            <label class="block text-sm font-medium text-content mb-1.5">
                                {move || t("keys.dialog_remark_label")}
                            </label>
                            <textarea
                                class="w-full min-h-20 py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary resize-y"
                                placeholder=move || t("keys.dialog_remark_placeholder")
                                prop:value=move || remark.get()
                                on:input=move |ev| remark.set(event_target_value(&ev))
                                prop:disabled=move || creating.get()
                            />
                        </div>

                        // Algorithm
                        <div>
                            <label class="block text-sm font-medium text-content mb-1.5">
                                {move || t("keys.dialog_algorithm_label")}
                            </label>
                            <select
                                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content focus:outline-none focus:ring-1 focus:ring-primary"
                                prop:value=move || algorithm.get()
                                on:change=move |ev| algorithm.set(event_target_value(&ev))
                                prop:disabled=move || creating.get()
                            >
                                <option value="ed25519">{move || t("keys.algorithm_ed25519")}</option>
                                <option value="rsa2048">{move || t("keys.algorithm_rsa2048")}</option>
                                <option value="rsa4096">{move || t("keys.algorithm_rsa4096")}</option>
                            </select>
                            <p class="mt-1.5 text-xs text-muted">{move || t("keys.dialog_algorithm_help")}</p>
                        </div>
                    </div>

                    <div class="flex justify-end gap-2 px-6 py-4 border-t border-border">
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-bg text-content hover:text-primary focus:outline-none disabled:opacity-50"
                            on:click=move |_| open.set(false)
                            prop:disabled=move || creating.get()
                        >
                            {move || t("keys.dialog_cancel")}
                        </button>
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none disabled:opacity-50"
                            on:click=submit
                            prop:disabled=move || creating.get()
                        >
                            {move || if creating.get() { t("keys.creating") } else { t("keys.dialog_submit") }}
                        </button>
                    </div>
                </div>
            </div>
    }
}

/// Modal for editing an existing key's directory and remark. Only these two
/// fields are editable: the key material and its comment (email) are embedded
/// in the on-disk key file and immutable after creation.
#[component]
fn EditKeyDialog(
    open: RwSignal<bool>,
    key_id: i64,
    current_directory: String,
    current_remark: String,
    on_missing: Callback<()>,
    on_done: Callback<()>,
) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let directory = RwSignal::new(String::new());
    let remark = RwSignal::new(String::new());
    let saving = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Seed the inputs with the current values whenever the dialog opens, and
    // clear transient error state so a previous failure does not linger.
    create_effect(move |_| {
        if open.get() {
            directory.set(current_directory.clone());
            remark.set(current_remark.clone());
            error.set(None);
        }
    });

    let submit = move |_| {
        let directory_val = directory.get_untracked().trim().to_string();
        let remark_val = remark.get_untracked().trim().to_string();

        if directory_val.is_empty() {
            error.set(Some(t("keys.required").to_string()));
            return;
        }

        saving.set(true);
        error.set(None);
        let sim = progress.begin_simulated();

        spawn_local(async move {
            match api::ssh_key_files_exist(key_id).await {
                Ok(true) => {}
                Ok(false) => {
                    open.set(false);
                    on_missing.call(());
                    saving.set(false);
                    progress.end_simulated(&sim);
                    return;
                }
                Err(e) => {
                    error.set(Some(e));
                    saving.set(false);
                    progress.end_simulated(&sim);
                    return;
                }
            }

            match api::update_ssh_key(key_id, directory_val, remark_val).await {
                Ok(_) => {
                    open.set(false);
                    on_done.call(());
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            saving.set(false);
            progress.end_simulated(&sim);
        });
    };

    view! {
        <div class=move || {
            if open.get() {
                "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-100 transition-opacity duration-300"
            } else {
                "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-0 pointer-events-none transition-opacity duration-300"
            }
        }>
            <div
                class=move || {
                    if open.get() {
                        "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-100 transition-transform duration-300"
                    } else {
                        "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-95 transition-transform duration-300"
                    }
                }
                on:click=|ev| ev.stop_propagation()
            >
                <div class="px-6 py-5 border-b border-border">
                    <h2 class="text-base font-semibold text-content">{move || t("keys.dialog_title_edit")}</h2>
                </div>

                    <div class="px-6 py-5 space-y-4">
                        <Show when=move || error.get().is_some()>
                            <div class="p-3 text-sm rounded-lg border border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                                {move || error.get().unwrap_or_default()}
                            </div>
                        </Show>

                        // Directory
                        <div>
                            <label class="block text-sm font-medium text-content mb-1.5">
                                {move || t("keys.dialog_directory_label")}
                            </label>
                            <input
                                type="text"
                                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary font-mono"
                                placeholder=move || t("keys.dialog_directory_placeholder")
                                prop:value=move || directory.get()
                                on:input=move |ev| directory.set(event_target_value(&ev))
                                prop:disabled=move || saving.get()
                            />
                            <p class="mt-1.5 text-xs text-muted">{move || t("keys.dialog_directory_help")}</p>
                        </div>

                        // Remark
                        <div>
                            <label class="block text-sm font-medium text-content mb-1.5">
                                {move || t("keys.dialog_remark_label")}
                            </label>
                            <textarea
                                class="w-full min-h-20 py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary resize-y"
                                placeholder=move || t("keys.dialog_remark_placeholder")
                                prop:value=move || remark.get()
                                on:input=move |ev| remark.set(event_target_value(&ev))
                                prop:disabled=move || saving.get()
                            />
                        </div>
                    </div>

                    <div class="flex justify-end gap-2 px-6 py-4 border-t border-border">
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-bg text-content hover:text-primary focus:outline-none disabled:opacity-50"
                            on:click=move |_| open.set(false)
                            prop:disabled=move || saving.get()
                        >
                            {move || t("common.cancel")}
                        </button>
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none disabled:opacity-50"
                            on:click=submit
                            prop:disabled=move || saving.get()
                        >
                            {move || if saving.get() { t("keys.dialog_editing") } else { t("keys.dialog_edit_submit") }}
                        </button>
                    </div>
                </div>
            </div>
    }
}
