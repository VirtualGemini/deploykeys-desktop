//! Repositories list screen.
//!
//! Renders the locally-synced repositories as a table with a name search box
//! and visibility + language filters. Filtering is entirely client-side over
//! the synced rows; "Refresh" re-syncs from GitHub and reloads.
//! The list is gated on being signed in: signing out clears it, and a
//! signed-out "Refresh" routes to the sign-in screen instead of erroring.

use crate::api::{self, Repo, SshKey};
use crate::i18n::t;
use crate::icons::{Icon, IconName};
use crate::page_size::page_size;
use crate::progress::ProgressHandle;
use crate::screens::keys::FormSelectDropdown;
use leptos::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
/// Sentinel value for the "no language" bucket in the language filter.
const OTHER: &str = "\u{1}other";

#[derive(Clone, Copy)]
struct TableDragState {
    pointer_id: i32,
    start_x: i32,
    start_y: i32,
    scroll_left: i32,
    scroll_top: i32,
}

#[component]
pub fn Repos(
    #[allow(unused_variables)] pending_count: RwSignal<usize>,
    account: RwSignal<Option<api::Account>>,
    on_sign_in_hint: Callback<()>,
) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let table_scroll_ref = NodeRef::<html::Div>::new();
    let table_drag = RwSignal::new(None::<TableDragState>);
    let repos = RwSignal::new(Vec::<Repo>::new());
    let loading = RwSignal::new(false);
    let syncing = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let bind_repo = RwSignal::new(None::<Repo>);
    let bind_keys = RwSignal::new(Vec::<SshKey>::new());
    let bind_keys_loading = RwSignal::new(false);
    let bind_submitting = RwSignal::new(false);
    let bind_error = RwSignal::new(None::<String>);
    let bind_selected_key = RwSignal::new(None::<i64>);
    let bind_writable = RwSignal::new(false);

    let query = RwSignal::new(String::new());
    // String filter values: "" = all; for language, OTHER = repos with no language.
    let visibility = RwSignal::new("all".to_string());
    let language = RwSignal::new(String::new());

    let page = RwSignal::new(1_usize);

    let signed_in = Signal::derive(move || account.get().is_some());

    // Load (and, when empty, auto-sync) whenever the session appears; clear when
    // it goes away. Driven by `account` so sign-in/sign-out reflect immediately.
    create_effect(move |_| {
        if account.get().is_none() {
            repos.set(Vec::new());
            error.set(None);
            loading.set(false);
            return;
        }
        loading.set(true);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::list_repositories().await {
                Ok(list) if !list.is_empty() => repos.set(list),
                Ok(_) => {
                    // Empty locally: do a first sync. Failure stays quiet here —
                    // the empty state guides the user to Refresh, which surfaces
                    // the real error.
                    syncing.set(true);
                    if let Err(e) = api::sync_repositories().await {
                        leptos::logging::warn!("Initial repo sync failed: {e}");
                    } else if let Ok(list) = api::list_repositories().await {
                        repos.set(list);
                    }
                    syncing.set(false);
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
            progress.end_simulated(&sim);
        });
    });

    // Sync from GitHub. When signed out, spotlight the sidebar sign-in button
    // (a gentle hint) instead of calling the backend and surfacing a bare error.
    let sync = move || {
        if account.get_untracked().is_none() {
            on_sign_in_hint.call(());
            return;
        }
        if syncing.get_untracked() {
            return;
        }
        syncing.set(true);
        let sim = progress.begin_simulated();
        error.set(None);
        spawn_local(async move {
            if let Err(e) = api::sync_repositories().await {
                error.set(Some(e));
            }
            if let Ok(list) = api::list_repositories().await {
                repos.set(list);
            }
            syncing.set(false);
            progress.end_simulated(&sim);
        });
    };

    // Options for the language filter: each distinct language, plus "Other" when
    // some repo has no language.
    let language_options = Signal::derive(move || {
        let list = repos.get();
        let mut langs: Vec<String> = list.iter().filter_map(|r| r.language.clone()).collect();
        langs.sort();
        langs.dedup();
        let mut opts = vec![(String::new(), t("repos.all_languages").to_string())];
        for l in langs {
            opts.push((l.clone(), l));
        }
        if list.iter().any(|r| r.language.is_none()) {
            opts.push((OTHER.to_string(), t("repos.other").to_string()));
        }
        opts
    });

    let visibility_options = Signal::derive(move || {
        vec![
            ("all".to_string(), t("repos.all").to_string()),
            ("public".to_string(), t("repos.public").to_string()),
            ("private".to_string(), t("repos.private").to_string()),
        ]
    });

    // name contains query (case-insensitive) AND visibility AND language.
    let filtered = Signal::derive(move || {
        let q = query.get().to_lowercase();
        let vis = visibility.get();
        let lang = language.get();
        repos
            .get()
            .into_iter()
            .filter(|r| {
                (q.is_empty() || r.full_name.to_lowercase().contains(&q))
                    && match vis.as_str() {
                        "public" => !r.private,
                        "private" => r.private,
                        _ => true,
                    }
                    && if lang.is_empty() {
                        true
                    } else if lang == OTHER {
                        r.language.is_none()
                    } else {
                        r.language.as_deref() == Some(lang.as_str())
                    }
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

    // Correct the page index when the result set or page size changes.
    create_effect(move |_| {
        let max = page_count.get().max(1);
        page.update(|p| *p = (*p).clamp(1, max));
    });

    // Persist page size changes to the backend.
    create_effect(move |_| {
        let size = page_size().get();
        spawn_local(async move {
            if let Err(e) = api::set_page_size(size).await {
                leptos::logging::warn!("Failed to persist page size: {e}");
            }
        });
    });

    let set_query = move |value: String| {
        query.set(value);
        page.set(1);
    };
    let set_visibility = move |value: String| {
        visibility.set(value);
        page.set(1);
    };
    let set_language = move |value: String| {
        language.set(value);
        page.set(1);
    };
    let clear_table_drag = move |pointer_id: i32| {
        if let Some(drag) = table_drag.get_untracked() {
            if drag.pointer_id == pointer_id {
                table_drag.set(None);
            }
        }
    };
    let open_bind_dialog = move |repo: Repo| {
        bind_repo.set(Some(repo));
        bind_keys.set(Vec::new());
        bind_selected_key.set(None);
        bind_writable.set(false);
        bind_error.set(None);
        bind_keys_loading.set(true);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::list_ssh_keys().await {
                Ok(list) => {
                    bind_selected_key.set(list.first().map(|key| key.id));
                    bind_keys.set(list);
                }
                Err(e) => bind_error.set(Some(e)),
            }
            bind_keys_loading.set(false);
            progress.end_simulated(&sim);
        });
    };
    let submit_bind_key = move || {
        if bind_submitting.get_untracked() {
            return;
        }
        let Some(repo) = bind_repo.get_untracked() else {
            return;
        };
        let Some(ssh_key_id) = bind_selected_key.get_untracked() else {
            bind_error.set(Some(t("repos.bind_key_required").to_string()));
            return;
        };

        bind_submitting.set(true);
        bind_error.set(None);
        let writable = bind_writable.get_untracked();
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::bind_deploy_key(repo.id, ssh_key_id, writable).await {
                Ok(()) => bind_repo.set(None),
                Err(e) => bind_error.set(Some(e)),
            }
            bind_submitting.set(false);
            progress.end_simulated(&sim);
        });
    };

    view! {
        <div class="flex flex-col gap-5 h-full">
            <div class="flex items-center justify-between gap-3">
                <h1 class="text-2xl font-semibold text-content">{move || t("nav.repos")}</h1>
                // Always visible: when signed out, clicking it spotlights sign-in.
                <button
                    type="button"
                    class="shrink-0 py-2 px-4 text-sm font-medium rounded-lg border border-border bg-primary-soft text-primary hover:opacity-80 focus:outline-none transition-opacity disabled:opacity-50"
                    prop:disabled=move || syncing.get()
                    on:click=move |_| sync()
                >
                    {move || t("repos.sync")}
                </button>
            </div>

            <Show
                when=move || signed_in.get()
                fallback=move || view! {
                    // Signed out: normal (empty) page with a centered, gentle hint.
                    <div class="flex flex-1 items-center justify-center py-16 text-center">
                        <p class="text-sm text-muted">{move || t("repos.sign_in_required")}</p>
                    </div>
                }
            >
                // Filter bar: search + visibility + language. Heights aligned.
                <div class="flex flex-wrap items-center gap-2">
                    <input
                        type="text"
                        class="flex-1 min-w-[12rem] py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none"
                        placeholder=move || t("repos.search_placeholder")
                        prop:value=move || query.get()
                        on:input=move |ev| set_query(event_target_value(&ev))
                    />
                    <FilterDropdown
                        options=visibility_options
                        selected=Signal::derive(move || visibility.get())
                        on_select=Callback::new(set_visibility)
                    />
                    <FilterDropdown
                        options=language_options
                        selected=Signal::derive(move || language.get())
                        fixed_height=true
                        on_select=Callback::new(set_language)
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
                        when=move || !repos.get().is_empty()
                        fallback=move || view! { <p class="text-sm text-muted">{move || t("repos.empty")}</p> }
                    >
                        <Show
                            when=move || !paged.get().is_empty()
                            fallback=move || view! { <p class="text-sm text-muted">{move || t("repos.no_match")}</p> }
                        >
                            <div class="flex flex-col flex-1 min-h-0">
                                <div class="relative flex-1 min-h-0 flex flex-col">
                                    <div class="min-h-0 flex-1 rounded-lg border border-border bg-surface overflow-hidden">
                                        <div
                                            node_ref=table_scroll_ref
                                            class="h-full overflow-x-auto overflow-y-auto min-h-0 cursor-grab"
                                            class:cursor-grabbing=move || table_drag.get().is_some()
                                            class:select-none=move || table_drag.get().is_some()
                                            style="touch-action: none;"
                                            on:pointerdown=move |ev| {
                                                if ev.pointer_type() == "mouse" && ev.button() != 0 {
                                                    return;
                                                }

                                                let started_on_interactive = ev
                                                    .target()
                                                    .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                                                    .and_then(|element| {
                                                        element
                                                            .closest("button, input, select, textarea, a, [data-no-drag-scroll]")
                                                            .ok()
                                                            .flatten()
                                                    })
                                                    .is_some();
                                                if started_on_interactive {
                                                    return;
                                                }

                                                if let Some(scroller) = table_scroll_ref.get() {
                                                    let _ = scroller.set_pointer_capture(ev.pointer_id());
                                                    table_drag.set(Some(TableDragState {
                                                        pointer_id: ev.pointer_id(),
                                                        start_x: ev.client_x(),
                                                        start_y: ev.client_y(),
                                                        scroll_left: scroller.scroll_left(),
                                                        scroll_top: scroller.scroll_top(),
                                                    }));
                                                    ev.prevent_default();
                                                }
                                            }
                                            on:pointermove=move |ev| {
                                                if let Some(drag) = table_drag.get_untracked() {
                                                    if drag.pointer_id != ev.pointer_id() {
                                                        return;
                                                    }
                                                    if let Some(scroller) = table_scroll_ref.get() {
                                                        let delta_x = ev.client_x() - drag.start_x;
                                                        let delta_y = ev.client_y() - drag.start_y;
                                                        scroller.set_scroll_left(drag.scroll_left - delta_x);
                                                        scroller.set_scroll_top(drag.scroll_top - delta_y);
                                                        ev.prevent_default();
                                                    }
                                                }
                                            }
                                            on:pointerup=move |ev| clear_table_drag(ev.pointer_id())
                                            on:pointercancel=move |ev| clear_table_drag(ev.pointer_id())
                                        >
                                            <table class="min-w-[46rem] w-full table-fixed border-collapse text-sm">
                                                <thead class="sticky top-0 z-10 bg-surface">
                                                    <tr class="border-b border-border">
                                                        <th class="w-[20rem] min-w-[20rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                            {move || t("repos.repository")}
                                                        </th>
                                                        <th class="w-[8rem] min-w-[8rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                            {move || t("repos.visibility")}
                                                        </th>
                                                        <th class="w-[10rem] min-w-[10rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                            {move || t("repos.language")}
                                                        </th>
                                                        <th class="sticky right-0 z-20 w-[8rem] min-w-[8rem] bg-surface text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle relative">
                                                            <span class="pointer-events-none absolute inset-y-0 left-0 w-px bg-border"></span>
                                                            {move || t("repos.actions")}
                                                        </th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    <For
                                                        each=move || paged.get()
                                                        key=|r| r.full_name.clone()
                                                        children=move |r| {
                                                            let repo_for_bind = r.clone();
                                                            let Repo {
                                                                full_name,
                                                                private,
                                                                language,
                                                                ..
                                                            } = r;
                                                            let language_text = language.unwrap_or_else(|| "—".to_string());
                                                            view! {
                                                                <tr class="group border-b border-border last:border-b-0 hover:bg-bg align-middle">
                                                                    <td class="w-[20rem] min-w-[20rem] max-w-[20rem] px-3 py-2 align-middle">
                                                                        <TruncatedCellText
                                                                            display=full_name.clone()
                                                                            tooltip=full_name
                                                                            class="font-medium text-content"
                                                                        />
                                                                    </td>
                                                                    <td class="w-[8rem] min-w-[8rem] px-3 py-2 whitespace-nowrap align-middle">
                                                                        <span class="inline-flex items-center text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">
                                                                            {move || if private { t("repos.private") } else { t("repos.public") }}
                                                                        </span>
                                                                    </td>
                                                                    <td class="w-[10rem] min-w-[10rem] max-w-[10rem] px-3 py-2 align-middle">
                                                                        <div class="inline-flex max-w-full items-center gap-1.5 text-muted">
                                                                            {language_text.ne("—").then({
                                                                                let language_text = language_text.clone();
                                                                                move || {
                                                                                    let dot = format!("background-color: {}", language_color(&language_text));
                                                                                    view! { <span class="inline-block size-2.5 shrink-0 rounded-full" style=dot></span> }
                                                                                }
                                                                            })}
                                                                            <TruncatedCellText
                                                                                display=language_text.clone()
                                                                                tooltip=language_text
                                                                                class="text-muted"
                                                                            />
                                                                        </div>
                                                                    </td>
                                                                    <td class="sticky right-0 z-[1] min-w-[8rem] bg-surface px-3 py-2 group-hover:bg-bg relative align-middle">
                                                                        <span class="pointer-events-none absolute inset-y-0 left-0 w-px bg-border"></span>
                                                                        <div class="inline-flex min-w-max items-center gap-1.5">
                                                                            <button
                                                                                type="button"
                                                                                title=move || t("repos.clone_repository")
                                                                                aria-label=move || t("repos.clone_repository")
                                                                                class="inline-flex items-center justify-center size-8 rounded-md text-primary hover:bg-primary-soft focus:outline-none"
                                                                            >
                                                                                <Icon name=IconName::Download class="size-4" />
                                                                            </button>
                                                                            <button
                                                                                type="button"
                                                                                title=move || t("repos.bind_key")
                                                                                aria-label=move || t("repos.bind_key")
                                                                                class="inline-flex items-center justify-center size-8 rounded-md text-content hover:bg-primary-soft dark:hover:bg-primary-soft/60 focus:outline-none"
                                                                                on:click=move |_| open_bind_dialog(repo_for_bind.clone())
                                                                            >
                                                                                <Icon name=IconName::Key class="size-4" />
                                                                            </button>
                                                                            <button
                                                                                type="button"
                                                                                title=move || t("repos.more_actions")
                                                                                aria-label=move || t("repos.more_actions")
                                                                                class="inline-flex items-center justify-center size-8 rounded-md text-content hover:bg-primary-soft dark:hover:bg-primary-soft/60 focus:outline-none"
                                                                            >
                                                                                <Icon name=IconName::MoreVertical class="size-4" />
                                                                            </button>
                                                                        </div>
                                                                    </td>
                                                                </tr>
                                                            }
                                                        }
                                                    />
                                                </tbody>
                                            </table>
                                        </div>
                                    </div>
                                </div>
                                <div class="shrink-0 h-4" aria-hidden="true"></div>
                                <PaginationBar
                                    page=page
                                    page_count=page_count
                                    total=filtered_count
                                />
                            </div>
                        </Show>
                    </Show>
                </Show>
            </Show>
            <BindKeyDialog
                repo=bind_repo
                keys=bind_keys
                loading=bind_keys_loading
                submitting=bind_submitting
                error=bind_error
                selected_key=bind_selected_key
                writable=bind_writable
                on_submit=Callback::new(move |_| submit_bind_key())
            />
        </div>
    }
}

#[component]
fn BindKeyDialog(
    repo: RwSignal<Option<Repo>>,
    keys: RwSignal<Vec<SshKey>>,
    loading: RwSignal<bool>,
    submitting: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    selected_key: RwSignal<Option<i64>>,
    writable: RwSignal<bool>,
    on_submit: Callback<()>,
) -> impl IntoView {
    let close = move || {
        if !submitting.get_untracked() {
            repo.set(None);
        }
    };
    let cached_repo_name = RwSignal::new(String::new());
    create_effect(move |_| {
        if let Some(repo) = repo.get() {
            cached_repo_name.set(repo.full_name);
        }
    });

    view! {
            <div
                class=move || {
                    if repo.get().is_some() {
                        "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-100 transition-opacity duration-300"
                    } else {
                        "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-0 pointer-events-none transition-opacity duration-300"
                    }
                }
                on:click=move |_| close()
            >
                <div
                    class=move || {
                        if repo.get().is_some() {
                            "w-full max-w-xl bg-surface border border-border rounded-xl shadow-2xl overflow-visible scale-100 transition-transform duration-300"
                        } else {
                            "w-full max-w-xl bg-surface border border-border rounded-xl shadow-2xl overflow-visible scale-95 transition-transform duration-300"
                        }
                    }
                    on:click=|ev| ev.stop_propagation()
                >
                    <div class="flex items-start justify-between gap-4 px-6 py-5">
                        <div class="min-w-0">
                            <h2 class="text-base font-semibold text-content">{move || t("repos.bind_dialog_title")}</h2>
                            <p class="mt-1 text-sm text-muted truncate">{move || cached_repo_name.get()}</p>
                        </div>
                        <button
                            type="button"
                            title=move || t("common.cancel")
                            aria-label=move || t("common.cancel")
                            class="inline-flex items-center justify-center size-8 rounded-md text-muted hover:bg-bg hover:text-content focus:outline-none disabled:opacity-50"
                            prop:disabled=move || submitting.get()
                            on:click=move |_| close()
                        >
                            <Icon name=IconName::Close class="size-4" />
                        </button>
                    </div>

                    <div class="px-6 pb-5 space-y-4">
                        <Show when=move || error.get().is_some()>
                            <div class="w-full p-3 text-sm rounded-lg border border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                                {move || error.get().unwrap_or_default()}
                            </div>
                        </Show>

                        <div>
                            <div class="mb-2 text-sm font-medium text-content">{move || t("repos.bind_dialog_key_label")}</div>
                            <Show
                                when=move || !loading.get()
                                fallback=move || view! { <div class="h-24 rounded-lg border border-border bg-bg"></div> }
                            >
                                <Show
                                    when=move || !keys.get().is_empty()
                                    fallback=move || view! {
                                        <p class="text-sm text-muted rounded-lg border border-border bg-bg px-3 py-4">
                                            {move || t("repos.bind_dialog_no_keys")}
                                        </p>
                                    }
                                >
                                    <FormSelectDropdown
                                        options=Signal::derive(move || {
                                            keys.get()
                                                .into_iter()
                                                .map(|key| (key.id.to_string(), key.directory.clone()))
                                                .collect::<Vec<_>>()
                                        })
                                        selected=Signal::derive(move || {
                                            selected_key.get().map(|id| id.to_string()).unwrap_or_default()
                                        })
                                        on_select=Callback::new(move |value: String| {
                                            if let Ok(id) = value.parse::<i64>() {
                                                selected_key.set(Some(id));
                                            }
                                        })
                                        disabled=Signal::derive(move || repo.get().is_none() || submitting.get())
                                    />
                                </Show>
                            </Show>
                        </div>

                        <div>
                            <FieldLabelWithHelp
                                label=Signal::derive(move || t("repos.bind_dialog_authorization_label"))
                                help=Signal::derive(move || t("repos.bind_dialog_authorization_help"))
                            />
                            <div class="grid grid-cols-2 gap-2">
                                <label class="flex items-center justify-between gap-3 rounded-lg border border-border bg-bg px-3 py-3">
                                    <span class="min-w-0 truncate text-sm font-medium text-content">{move || t("repos.bind_dialog_pull")}</span>
                                    <input
                                        type="checkbox"
                                        class="size-4 shrink-0 accent-primary"
                                        prop:checked=true
                                        prop:disabled=true
                                    />
                                </label>
                                <label class="flex items-center justify-between gap-3 rounded-lg border border-border bg-bg px-3 py-3">
                                    <span class="min-w-0 truncate text-sm font-medium text-content">{move || t("repos.bind_dialog_push")}</span>
                                    <input
                                        type="checkbox"
                                        class="size-4 shrink-0 accent-primary"
                                        prop:checked=move || writable.get()
                                        prop:disabled=move || submitting.get()
                                        on:change=move |ev| {
                                            let checked = ev
                                                .target()
                                                .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
                                                .map(|input| input.checked())
                                                .unwrap_or(false);
                                            writable.set(checked);
                                        }
                                    />
                                </label>
                            </div>
                        </div>
                    </div>

                    <div class="flex justify-end gap-2 px-6 py-4 border-t border-border">
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-bg text-content hover:text-primary focus:outline-none disabled:opacity-50"
                            prop:disabled=move || submitting.get()
                            on:click=move |_| close()
                        >
                            {move || t("common.cancel")}
                        </button>
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none disabled:opacity-50"
                            prop:disabled=move || loading.get() || submitting.get() || selected_key.get().is_none()
                            on:click=move |_| on_submit.call(())
                        >
                            {move || if submitting.get() { t("repos.bind_dialog_submitting") } else { t("repos.bind_dialog_submit") }}
                        </button>
                    </div>
                </div>
            </div>
    }
}

#[component]
fn FieldLabelWithHelp(
    #[prop(into)] label: Signal<&'static str>,
    #[prop(into)] help: Signal<&'static str>,
) -> impl IntoView {
    view! {
        <div class="mb-2 flex items-center gap-1.5">
            <label class="text-sm font-medium text-content">
                {move || label.get()}
            </label>
            <span class="group/help relative inline-flex">
                <button
                    type="button"
                    class="inline-flex size-4 items-center justify-center rounded-full border border-border text-[10px] font-semibold leading-none text-muted hover:border-primary hover:text-primary focus:outline-none focus:ring-1 focus:ring-primary"
                    aria-label=move || help.get()
                >
                    "?"
                </button>
                <span class="pointer-events-none absolute bottom-full left-1/2 z-[80] mb-2 hidden w-[min(22rem,calc(100vw-3rem))] -translate-x-1/2 whitespace-normal break-words rounded-lg border border-border bg-surface px-3 py-2 text-left text-xs leading-5 text-content shadow-xl group-hover/help:block group-focus-within/help:block">
                    {move || help.get()}
                </span>
            </span>
        </div>
    }
}

#[component]
fn TruncatedCellText(
    #[prop(into)] display: String,
    #[prop(optional, into)] tooltip: String,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let text_ref = NodeRef::<html::Span>::new();
    let bubble_open = RwSignal::new(false);
    let bubble = if tooltip.is_empty() {
        display.clone()
    } else {
        tooltip
    };
    let show_bubble = !bubble.trim().is_empty() && bubble != "—";
    let title_text = if show_bubble {
        bubble.clone()
    } else {
        String::new()
    };
    let bubble_text = bubble.clone();
    let update_bubble = move || {
        let truncated = text_ref
            .get()
            .map(|element| element.scroll_width() > element.client_width())
            .unwrap_or(false);
        bubble_open.set(show_bubble && truncated);
    };

    view! {
        <div class="group/cell relative flex max-w-full">
            <span
                node_ref=text_ref
                class=format!("block max-w-full truncate whitespace-nowrap {class}")
                title=title_text
                on:mouseenter=move |_| update_bubble()
                on:mouseleave=move |_| bubble_open.set(false)
            >
                {display}
            </span>
            <Show when=move || bubble_open.get()>
                <div class="pointer-events-none absolute left-1/2 top-0 z-30 hidden w-max max-w-[min(28rem,calc(100vw-4rem))] -translate-x-1/2 -translate-y-[calc(100%+0.5rem)] whitespace-normal break-words rounded-lg border border-border bg-surface px-3 py-2 text-left text-xs leading-5 text-content shadow-xl group-hover/cell:block">
                    {bubble_text.clone()}
                </div>
            </Show>
        </div>
    }
}

/// A compact text-triggered dropdown styled after the language switcher, but
/// half the row height and sized to its content. `options` are `(value, label)`
/// pairs; selecting one fires `on_select` with the value.
#[component]
fn FilterDropdown(
    #[prop(into)] options: Signal<Vec<(String, String)>>,
    #[prop(into)] selected: Signal<String>,
    #[prop(optional)] fixed_height: bool,
    on_select: Callback<String>,
) -> impl IntoView {
    let open = RwSignal::new(false);

    // Trigger label = the display text of the currently-selected value.
    let label = Signal::derive(move || {
        let sel = selected.get();
        options
            .get()
            .into_iter()
            .find(|(value, _)| *value == sel)
            .map(|(_, display)| display)
            .unwrap_or_default()
    });
    let panel_class = if fixed_height {
        "absolute start-0 mt-1 z-50 w-max h-64 overflow-y-auto p-1 bg-surface border border-border rounded-xl shadow-xl"
    } else {
        "absolute start-0 mt-1 z-50 w-max max-h-80 overflow-y-auto p-1 bg-surface border border-border rounded-xl shadow-xl"
    };

    view! {
        <div class="relative">
            // Trigger aligns in height with the search input (py-2), text not icon.
            <button
                type="button"
                class="inline-flex items-center justify-center w-36 h-10 px-3 text-sm rounded-lg border border-border bg-bg text-content hover:bg-surface focus:outline-none whitespace-nowrap"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <span class="min-w-0 truncate">{move || label.get()}</span>
            </button>

            <Show when=move || open.get()>
                <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                // Panel: content-width, rows half the height of the language switcher.
                <div class=panel_class>
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

/// Shared classes for pager icon buttons (prev/next).
const PAGER_CELL: &str = "inline-flex items-center justify-center min-w-[2rem] h-8 px-2 text-sm rounded-md text-muted hover:bg-surface hover:text-content disabled:opacity-40 disabled:pointer-events-none";

/// Full pagination bar below the list: total · prev/next with page indicator ·
/// page-size · "go to page" input.
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

    let submit_goto = move || {
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

            <span class="text-sm text-border select-none" aria-hidden="true">|</span>

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

            <span class="text-sm text-border select-none" aria-hidden="true">|</span>

            <PageSizeSelector />

            <span class="text-sm text-border select-none" aria-hidden="true">|</span>

            <div class="flex items-center gap-1.5">
                <span class="text-sm text-muted">
                    {move || t("repos.go_to_page_before")}
                </span>
                <input
                    type="text"
                    class="w-12 h-8 px-2 text-sm text-center rounded-md border border-border bg-bg text-content focus:outline-none focus:ring-1 focus:ring-primary"
                    prop:value=move || goto_value.get()
                    on:input=move |ev| goto_value.set(event_target_value(&ev))
                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                        if ev.key() == "Enter" {
                            submit_goto();
                        }
                    }
                />
                <span class="text-sm text-muted">
                    {move || t("repos.go_to_page_after")}
                </span>
            </div>
        </div>
    }
}

/// Page-size control: an upward-opening dropdown (the bar sits at the page
/// bottom) listing the presets (5/10/20), followed by the unit word. Updates
/// the global `page_size()` signal.
#[component]
fn PageSizeSelector() -> impl IntoView {
    let presets = [5usize, 10, 20];
    let current = page_size();
    let open = RwSignal::new(false);

    view! {
        <div class="flex items-center gap-1.5">
            <span class="text-sm text-muted">{move || t("repos.page_size")}</span>

            // Trigger + panel share a `relative` box so the panel aligns under the
            // trigger regardless of the label's width across locales.
            <div class="relative">
                // Trigger: shows the current page size; caret flips with open state.
                <button
                    type="button"
                    class="inline-flex items-center justify-center gap-1 min-w-[3.25rem] h-8 px-2.5 text-sm rounded-md text-content hover:bg-surface focus:outline-none"
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    <span>{move || current.get()}</span>
                </button>

                <Show when=move || open.get()>
                    <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                    // Panel opens upward (`bottom-full`) so it never runs off the bottom edge.
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

/// GitHub-style dot color for a primary language (Linguist palette), with a
/// neutral gray fallback for anything unmapped.
fn language_color(language: &str) -> &'static str {
    match language {
        "Rust" => "#dea584",
        "Go" => "#00ADD8",
        "Python" => "#3572A5",
        "JavaScript" => "#f1e05a",
        "TypeScript" => "#3178c6",
        "Java" => "#b07219",
        "Kotlin" => "#A97BFF",
        "Swift" => "#F05138",
        "C" => "#555555",
        "C++" => "#f34b7d",
        "C#" => "#178600",
        "Ruby" => "#701516",
        "PHP" => "#4F5D95",
        "Shell" => "#89e051",
        "HTML" => "#e34c26",
        "CSS" => "#563d7c",
        "SCSS" => "#c6538c",
        "Vue" => "#41b883",
        "Svelte" => "#ff3e00",
        "Dart" => "#00B4AB",
        "Scala" => "#c22d40",
        "Elixir" => "#6e4a7e",
        "Erlang" => "#B83998",
        "Haskell" => "#5e5086",
        "Clojure" => "#db5855",
        "Lua" => "#000080",
        "Perl" => "#0298c3",
        "R" => "#198CE7",
        "Julia" => "#a270ba",
        "Objective-C" => "#438eff",
        "Zig" => "#ec915c",
        "Nix" => "#7e7eff",
        "Dockerfile" => "#384d54",
        "Makefile" => "#427819",
        "PowerShell" => "#012456",
        "Vim Script" => "#199f4b",
        "TeX" => "#3D6117",
        _ => "#9ca3af",
    }
}

#[allow(dead_code)]
fn begin_pending(_pending_count: RwSignal<usize>) {}
#[allow(dead_code)]
fn end_pending(_pending_count: RwSignal<usize>) {}
