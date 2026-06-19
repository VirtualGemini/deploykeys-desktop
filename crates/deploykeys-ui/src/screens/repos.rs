//! Repositories list screen.
//!
//! Renders the locally-synced repositories as a table with a name search box
//! and visibility + language filters. Filtering is entirely client-side over
//! the synced rows; "Refresh" re-syncs from GitHub and reloads.
//! The list is gated on being signed in: signing out clears it, and a
//! signed-out "Refresh" routes to the sign-in screen instead of erroring.

use crate::api::{self, Repo, SshKey};
use crate::connection::{connection_state, ConnectionKind};
use crate::i18n::t;
use crate::icons::{Icon, IconName};
use crate::page_size::page_size;
use crate::progress::ProgressHandle;
use crate::screens::keys::FormSelectDropdown;
use crate::toast::ToastHandle;
use leptos::*;
use std::time::Duration;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum RepoRemoteAction {
    Connect,
    Test,
}

#[derive(Clone, Copy)]
struct MoreMenuState {
    repo_id: i64,
    top: f64,
    left: f64,
}

#[component]
pub fn Repos(
    #[allow(unused_variables)] pending_count: RwSignal<usize>,
    account: RwSignal<Option<api::Account>>,
    on_sign_in_hint: Callback<()>,
) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let toast = ToastHandle::expect();
    let conn = connection_state();
    let has_connection = Signal::derive(move || conn.has_active());
    let table_scroll_ref = NodeRef::<html::Div>::new();
    let table_drag = RwSignal::new(None::<TableDragState>);
    let repos = RwSignal::new(Vec::<Repo>::new());
    let loading = RwSignal::new(false);
    let syncing = RwSignal::new(false);
    let clone_picker_running = RwSignal::new(None::<i64>);
    let clone_tasks = RwSignal::new(Vec::<api::CloneTask>::new());
    let clone_list_open = RwSignal::new(false);
    let clone_expanded_task_id = RwSignal::new(None::<u64>);
    let more_repo_open = RwSignal::new(None::<MoreMenuState>);
    let remote_action_running = RwSignal::new(None::<(i64, RepoRemoteAction)>);
    let bind_repo = RwSignal::new(None::<Repo>);
    let bind_keys = RwSignal::new(Vec::<SshKey>::new());
    let bind_keys_loading = RwSignal::new(false);
    let bind_submitting = RwSignal::new(false);
    let bind_selected_key = RwSignal::new(None::<i64>);
    let bind_writable = RwSignal::new(false);
    let remote_clone_repo = RwSignal::new(None::<Repo>);

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
                Err(e) => toast.error(e),
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
        spawn_local(async move {
            match api::sync_repositories().await {
                Ok(_) => toast.success(t("repos.sync_success")),
                Err(e) => toast.error(e),
            }
            if let Ok(list) = api::list_repositories().await {
                repos.set(list);
            }
            syncing.set(false);
            progress.end_simulated(&sim);
        });
    };

    spawn_local(async move {
        match api::list_clone_tasks().await {
            Ok(tasks) => clone_tasks.set(tasks),
            Err(e) => leptos::logging::warn!("Failed to load clone tasks: {e}"),
        }
    });
    if let Ok(interval) = set_interval_with_handle(
        move || {
            spawn_local(async move {
                match api::list_clone_tasks().await {
                    Ok(tasks) => clone_tasks.set(tasks),
                    Err(e) => leptos::logging::warn!("Failed to refresh clone tasks: {e}"),
                }
            });
        },
        Duration::from_millis(700),
    ) {
        on_cleanup(move || interval.clear());
    }

    let clone_repo = move |repo: Repo| {
        if clone_picker_running.get_untracked().is_some() {
            return;
        }
        let active_is_remote = conn
            .connected_id
            .get_untracked()
            .and_then(|id| {
                conn.connections
                    .get_untracked()
                    .into_iter()
                    .find(|connection| connection.id == id)
            })
            .is_some_and(|connection| connection.kind == ConnectionKind::Remote);
        if active_is_remote {
            remote_clone_repo.set(Some(repo));
            return;
        }
        clone_picker_running.set(Some(repo.id));
        let dialog_title = t("repos.clone_dialog_title").replace("{}", &repo.full_name);
        spawn_local(async move {
            match api::clone_repository(repo.id, dialog_title).await {
                Ok(Some(task)) => {
                    let task_id = task.id;
                    upsert_clone_task(clone_tasks, task);
                    clone_expanded_task_id.set(Some(task_id));
                    clone_list_open.set(true);
                }
                Ok(None) => {}
                Err(e) => toast.error(e),
            }
            clone_picker_running.set(None);
        });
    };
    let running_clone_count = Signal::derive(move || {
        clone_tasks
            .get()
            .iter()
            .filter(|task| task.status == "running")
            .count()
    });

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
        count.div_ceil(size)
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
        more_repo_open.set(None);
        bind_repo.set(Some(repo));
        bind_keys.set(Vec::new());
        bind_selected_key.set(None);
        bind_writable.set(false);
        bind_keys_loading.set(true);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::list_ssh_keys().await {
                Ok(list) => {
                    bind_selected_key.set(list.first().map(|key| key.id));
                    bind_keys.set(list);
                }
                Err(e) => toast.error(e),
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
            toast.error(t("repos.bind_key_required"));
            return;
        };

        bind_submitting.set(true);
        let writable = bind_writable.get_untracked();
        let success = t("repos.bind_success").to_string();
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::bind_deploy_key(repo.id, ssh_key_id, writable).await {
                Ok(()) => {
                    bind_repo.set(None);
                    toast.success(success);
                }
                Err(e) => toast.error(e),
            }
            bind_submitting.set(false);
            progress.end_simulated(&sim);
        });
    };
    let connect_repo = move |repo_id: i64| {
        if remote_action_running.get_untracked().is_some() {
            return;
        }
        more_repo_open.set(None);
        remote_action_running.set(Some((repo_id, RepoRemoteAction::Connect)));
        let success = t("repos.connect_success").to_string();
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::connect_repository_remote(repo_id).await {
                Ok(_) => toast.success(success),
                Err(e) => toast.error(e),
            }
            remote_action_running.set(None);
            progress.end_simulated(&sim);
        });
    };
    let test_repo = move |repo_id: i64| {
        if remote_action_running.get_untracked().is_some() {
            return;
        }
        more_repo_open.set(None);
        remote_action_running.set(Some((repo_id, RepoRemoteAction::Test)));
        let success = t("repos.test_success").to_string();
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::test_repository_remote(repo_id).await {
                Ok(_) => toast.success(success),
                Err(e) => toast.error(e),
            }
            remote_action_running.set(None);
            progress.end_simulated(&sim);
        });
    };

    view! {
        <div class="flex flex-col gap-5 h-full">
            <div class="flex items-center justify-between gap-3">
                <h1 class="text-2xl font-semibold text-content">{move || t("nav.repos")}</h1>
                <div class="flex shrink-0 items-center gap-2">
                    <button
                        type="button"
                        title=move || t("repos.clone_tasks")
                        aria-label=move || t("repos.clone_tasks")
                        class="relative inline-flex size-9 items-center justify-center overflow-visible rounded-lg border border-border bg-surface text-content hover:bg-bg hover:text-primary focus:outline-none"
                        on:click=move |_| clone_list_open.set(true)
                    >
                        <Icon name=IconName::Download class="size-4" />
                        <Show when=move || { running_clone_count.get() > 0 }>
                            <span class="pointer-events-none absolute -left-1.5 -top-1.5 z-20 min-w-4 rounded-full bg-primary px-1 text-center text-[10px] font-semibold leading-4 text-on-primary shadow-sm">
                                {move || running_clone_count.get().min(99).to_string()}
                            </span>
                        </Show>
                    </button>
                    // Always visible: when signed out, clicking it spotlights sign-in.
                    <button
                        type="button"
                        class="py-2 px-4 text-sm font-medium rounded-lg border border-border bg-primary-soft text-primary hover:opacity-80 focus:outline-none transition-opacity disabled:opacity-50"
                        prop:disabled=move || syncing.get()
                        on:click=move |_| sync()
                    >
                        {move || t("repos.sync")}
                    </button>
                </div>
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
                                                            let repo_id = r.id;
                                                            let repo_for_clone = r.clone();
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
                                                                                title=move || if has_connection.get() { t("repos.clone_repository").to_string() } else { t("connect.required_hint").to_string() }
                                                                                aria-label=move || t("repos.clone_repository")
                                                                                class=move || {
                                                                                    let base = "relative inline-flex items-center justify-center size-8 overflow-hidden rounded-md text-primary hover:bg-primary-soft focus:outline-none disabled:pointer-events-none";
                                                                                    if clone_picker_running.get() == Some(repo_id) {
                                                                                        base.to_string()
                                                                                    } else if clone_picker_running.get().is_some() || !has_connection.get() {
                                                                                        format!("{base} opacity-50")
                                                                                    } else {
                                                                                        base.to_string()
                                                                                    }
                                                                                }
                                                                                prop:disabled=move || clone_picker_running.get().is_some() || !has_connection.get()
                                                                                on:click=move |_| clone_repo(repo_for_clone.clone())
                                                                            >
                                                                                <Icon name=IconName::Download class="size-4" />
                                                                            </button>
                                                                            <button
                                                                                type="button"
                                                                                title=move || if has_connection.get() { t("repos.bind_key").to_string() } else { t("connect.required_hint").to_string() }
                                                                                aria-label=move || t("repos.bind_key")
                                                                                class="inline-flex items-center justify-center size-8 rounded-md text-content hover:bg-primary-soft dark:hover:bg-primary-soft/60 focus:outline-none disabled:opacity-50 disabled:pointer-events-none"
                                                                                prop:disabled=move || !has_connection.get()
                                                                                on:click=move |_| open_bind_dialog(repo_for_bind.clone())
                                                                            >
                                                                                <Icon name=IconName::Key class="size-4" />
                                                                            </button>
                                                                            <button
                                                                                type="button"
                                                                                data-more-actions-button=""
                                                                                title=move || if has_connection.get() { t("repos.more_actions").to_string() } else { t("connect.required_hint").to_string() }
                                                                                aria-label=move || t("repos.more_actions")
                                                                                class="inline-flex items-center justify-center size-8 rounded-md text-content hover:bg-primary-soft dark:hover:bg-primary-soft/60 focus:outline-none disabled:opacity-50 disabled:pointer-events-none"
                                                                                prop:disabled=move || !has_connection.get()
                                                                                on:click=move |ev| {
                                                                                    ev.stop_propagation();
                                                                                    let (top, left) = more_menu_position(&ev);
                                                                                    more_repo_open.update(|open| {
                                                                                        *open = if open.as_ref().map(|menu| menu.repo_id) == Some(repo_id) {
                                                                                            None
                                                                                        } else {
                                                                                            Some(MoreMenuState { repo_id, top, left })
                                                                                        };
                                                                                    });
                                                                                }
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
                selected_key=bind_selected_key
                writable=bind_writable
                on_submit=Callback::new(move |_| submit_bind_key())
            />
            <CloneTasksDialog
                open=clone_list_open
                tasks=clone_tasks
                selected_task_id=clone_expanded_task_id
                on_clear=Callback::new(move |_| {
                    spawn_local(async move {
                        match api::clear_clone_tasks().await {
                            Ok(tasks) => clone_tasks.set(tasks),
                            Err(e) => toast.error(e),
                        }
                    });
                })
            />
            <RemoteCloneDialog
                repo=remote_clone_repo
                running=clone_picker_running
                tasks=clone_tasks
                selected_task_id=clone_expanded_task_id
                list_open=clone_list_open
            />
            <RepoMoreActionsDropdown
                menu=more_repo_open
                running=remote_action_running
                on_connect=Callback::new(connect_repo)
                on_test=Callback::new(test_repo)
            />
        </div>
    }
}

fn more_menu_position(ev: &web_sys::MouseEvent) -> (f64, f64) {
    const MENU_WIDTH: f64 = 96.0;
    const MENU_HEIGHT: f64 = 84.0;
    const GAP: f64 = 6.0;
    const MARGIN: f64 = 8.0;

    let Some(element) = ev
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|target| {
            target
                .closest("[data-more-actions-button]")
                .ok()
                .flatten()
                .or_else(|| target.closest("button").ok().flatten())
                .or(Some(target))
        })
    else {
        return (MARGIN, MARGIN);
    };
    let rect = element.get_bounding_client_rect();
    let window = web_sys::window();
    let viewport_width = window
        .as_ref()
        .and_then(|window| window.inner_width().ok())
        .and_then(|value| value.as_f64())
        .unwrap_or(1024.0);
    let viewport_height = window
        .and_then(|window| window.inner_height().ok())
        .and_then(|value| value.as_f64())
        .unwrap_or(768.0);

    let max_left = (viewport_width - MENU_WIDTH - MARGIN).max(MARGIN);
    let left = (rect.right() - MENU_WIDTH).clamp(MARGIN, max_left);
    let preferred_top = rect.bottom() + GAP;
    let top = if preferred_top + MENU_HEIGHT > viewport_height - MARGIN {
        (rect.top() - MENU_HEIGHT - GAP).max(MARGIN)
    } else {
        preferred_top
    };

    (top, left)
}

#[component]
fn RepoMoreActionsDropdown(
    menu: RwSignal<Option<MoreMenuState>>,
    running: RwSignal<Option<(i64, RepoRemoteAction)>>,
    on_connect: Callback<i64>,
    on_test: Callback<i64>,
) -> impl IntoView {
    let conn = connection_state();
    let disabled = Signal::derive(move || running.get().is_some() || !conn.has_active());
    view! {
        <Show when=move || menu.get().is_some()>
            <div
                class="fixed inset-0 z-40 cursor-default"
                on:click=move |_| menu.set(None)
            ></div>
            <div
                data-no-drag-scroll
                class="fixed z-50 min-w-24 rounded-lg border border-border bg-surface py-1 shadow-xl"
                style=move || {
                    menu.get()
                        .map(|menu| format!("top: {:.1}px; left: {:.1}px;", menu.top, menu.left))
                        .unwrap_or_default()
                }
            >
                <button
                    type="button"
                    class="block w-full px-3 py-1.5 text-left text-sm font-medium text-content hover:bg-bg hover:text-primary focus:outline-none disabled:opacity-50"
                    prop:disabled=move || disabled.get()
                    on:click=move |_| {
                        if let Some(menu) = menu.get_untracked() {
                            on_connect.call(menu.repo_id);
                        }
                    }
                >
                    {move || t("repos.connect_remote")}
                </button>
                <button
                    type="button"
                    class="block w-full px-3 py-1.5 text-left text-sm font-medium text-content hover:bg-bg hover:text-primary focus:outline-none disabled:opacity-50"
                    prop:disabled=move || disabled.get()
                    on:click=move |_| {
                        if let Some(menu) = menu.get_untracked() {
                            on_test.call(menu.repo_id);
                        }
                    }
                >
                    {move || t("repos.test_remote")}
                </button>
            </div>
        </Show>
    }
}

fn upsert_clone_task(tasks: RwSignal<Vec<api::CloneTask>>, next: api::CloneTask) {
    tasks.update(|tasks| {
        if let Some(task) = tasks.iter_mut().find(|task| task.id == next.id) {
            *task = next;
        } else {
            tasks.insert(0, next);
        }
    });
}

fn clone_status_label(status: &str) -> String {
    match status {
        "running" => t("repos.clone_status_running").to_string(),
        "succeeded" => t("repos.clone_status_succeeded").to_string(),
        "failed" => t("repos.clone_status_failed").to_string(),
        _ => status.to_string(),
    }
}

fn clone_status_class(status: &str) -> String {
    let base = "shrink-0 rounded-full border border-border px-2 py-0.5 text-[11px] font-medium";
    match status {
        "running" | "succeeded" => format!("{base} text-primary"),
        "failed" => format!("{base} text-red-600 dark:text-red-400"),
        _ => format!("{base} text-muted"),
    }
}

fn compact_clone_log(log: &str) -> String {
    log.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .filter_map(compact_clone_log_line)
        .map(|line| format!("{line}\n"))
        .collect()
}

fn compact_clone_log_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let keep = line.starts_with("Cloning into ")
        || line.starts_with("remote: Enumerating objects:")
        || line.starts_with("remote: Total ")
        || (line.starts_with("remote: Counting objects:") && line.contains("done."))
        || (line.starts_with("remote: Compressing objects:") && line.contains("done."))
        || (line.starts_with("Receiving objects:") && line.contains("done."))
        || (line.starts_with("Resolving deltas:") && line.contains("done."))
        || line.starts_with("fatal:")
        || line.starts_with("error:")
        || line.starts_with("ssh:")
        || line.contains("Permission denied")
        || line.contains("Host key verification failed")
        || line.contains("Repository not found");

    keep.then(|| line.to_string())
}

/// Seconds-since-epoch cutoff for a range key; `None` means "all time".
fn range_cutoff_secs(range: &str) -> Option<i64> {
    let now = (js_sys::Date::now() / 1000.0) as i64;
    let window = match range {
        "5h" => 5 * 3600,
        "12h" => 12 * 3600,
        "24h" => 24 * 3600,
        "7d" => 7 * 86_400,
        "15d" => 15 * 86_400,
        "30d" => 30 * 86_400,
        "180d" => 180 * 86_400,
        "365d" => 365 * 86_400,
        _ => return None,
    };
    Some(now - window)
}

fn clone_range_options() -> Vec<(String, String)> {
    [
        ("5h", "repos.clone_range_5h"),
        ("12h", "repos.clone_range_12h"),
        ("24h", "repos.clone_range_24h"),
        ("7d", "repos.clone_range_7d"),
        ("15d", "repos.clone_range_15d"),
        ("30d", "repos.clone_range_30d"),
        ("180d", "repos.clone_range_180d"),
        ("365d", "repos.clone_range_365d"),
        ("all", "repos.clone_range_all"),
    ]
    .into_iter()
    .map(|(value, key)| (value.to_string(), t(key).to_string()))
    .collect()
}

const DEFAULT_REMOTE_CLONE_DIR: &str = "~/apps";

#[component]
fn RemoteCloneDialog(
    repo: RwSignal<Option<Repo>>,
    running: RwSignal<Option<i64>>,
    tasks: RwSignal<Vec<api::CloneTask>>,
    selected_task_id: RwSignal<Option<u64>>,
    list_open: RwSignal<bool>,
) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let toast = ToastHandle::expect();
    let visible_repo = RwSignal::new(None::<Repo>);
    let dialog_open = RwSignal::new(false);
    let input = RwSignal::new(DEFAULT_REMOTE_CLONE_DIR.to_string());

    create_effect(move |_| {
        if let Some(next_repo) = repo.get() {
            visible_repo.set(Some(next_repo));
            input.set(DEFAULT_REMOTE_CLONE_DIR.to_string());
            dialog_open.set(false);
            set_timeout(move || dialog_open.set(true), Duration::from_millis(16));
        } else if visible_repo.with_untracked(Option::is_some) {
            dialog_open.set(false);
            set_timeout(
                move || {
                    if repo.get_untracked().is_none() {
                        visible_repo.set(None);
                    }
                },
                Duration::from_millis(220),
            );
        }
    });

    let submit = move |_| {
        if running.get_untracked().is_some() {
            return;
        }
        let Some(repo_value) = repo.get_untracked() else {
            return;
        };
        let path = input.get_untracked().trim().to_string();
        if path.is_empty() {
            toast.error(t("repos.remote_clone_path_required"));
            return;
        }
        running.set(Some(repo_value.id));
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::clone_repository_to_path(repo_value.id, path).await {
                Ok(task) => {
                    let task_id = task.id;
                    upsert_clone_task(tasks, task);
                    selected_task_id.set(Some(task_id));
                    list_open.set(true);
                    repo.set(None);
                }
                Err(e) => toast.error(e),
            }
            running.set(None);
            progress.end_simulated(&sim);
        });
    };

    view! {
        <Show when=move || visible_repo.get().is_some()>
            <div
                class=move || {
                    if dialog_open.get() {
                        "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-100 transition-opacity duration-200"
                    } else {
                        "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-0 pointer-events-none transition-opacity duration-200"
                    }
                }
                on:click=move |_| repo.set(None)
            >
                <div
                    class=move || {
                        if dialog_open.get() {
                            "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-100 transition-transform duration-200"
                        } else {
                            "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-95 transition-transform duration-200"
                        }
                    }
                    on:click=|ev| ev.stop_propagation()
                >
                    <div class="px-6 py-5 border-b border-border">
                        <h2 class="text-base font-semibold text-content">{move || t("repos.remote_clone_title")}</h2>
                        <p class="mt-1 text-sm text-muted truncate">
                            {move || visible_repo.get().map(|repo| repo.full_name).unwrap_or_default()}
                        </p>
                    </div>
                    <div class="px-6 py-5 space-y-4">
                        <div>
                            <label class="block text-sm font-medium text-content mb-1.5">
                                {move || t("repos.remote_clone_directory")}
                            </label>
                            <input
                                type="text"
                                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary font-mono"
                                prop:value=move || input.get()
                                on:input=move |ev| input.set(event_target_value(&ev))
                                prop:disabled=move || running.get().is_some()
                            />
                        </div>
                    </div>
                    <div class="flex justify-end gap-2 px-6 py-4 border-t border-border">
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-bg text-content hover:text-primary focus:outline-none disabled:opacity-50"
                            on:click=move |_| repo.set(None)
                            prop:disabled=move || running.get().is_some()
                        >
                            {move || t("common.cancel")}
                        </button>
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none disabled:opacity-50"
                            on:click=submit
                            prop:disabled=move || input.get().trim().is_empty() || running.get().is_some()
                        >
                            {move || t("repos.clone_repository")}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn CloneTasksDialog(
    open: RwSignal<bool>,
    tasks: RwSignal<Vec<api::CloneTask>>,
    selected_task_id: RwSignal<Option<u64>>,
    on_clear: Callback<()>,
) -> impl IntoView {
    let search = RwSignal::new(String::new());
    let range = RwSignal::new("5h".to_string());

    let has_clearable = Signal::derive(move || {
        tasks
            .get()
            .iter()
            .any(|task| task.status.as_str() != "running")
    });

    let range_options = Signal::derive(clone_range_options);

    // name/path contains query (case-insensitive) AND started within the range.
    // `tasks` repolls every 700 ms, so the time cutoff re-evaluates continuously.
    let filtered = Signal::derive(move || {
        let q = search.get().to_lowercase();
        let cutoff = range_cutoff_secs(&range.get());
        tasks
            .get()
            .into_iter()
            .filter(|task| {
                let matches_query = q.is_empty()
                    || task.repo_full_name.to_lowercase().contains(&q)
                    || task.local_path.to_lowercase().contains(&q);
                let within_range = match cutoff {
                    Some(c) => task.started_at >= c,
                    None => true,
                };
                matches_query && within_range
            })
            .collect::<Vec<_>>()
    });

    // Keep the expanded task's terminal pinned to the bottom as output streams in.
    // Only one task is expanded at a time, so a single ref/effect suffices.
    let terminal_ref = NodeRef::<html::Div>::new();
    // Stick-to-bottom: true while the user is parked at the bottom of the log, so
    // streaming output follows; set false when they scroll up so it stops yanking.
    let stick = RwSignal::new(true);
    let expanded_task = create_memo(move |_| {
        let id = selected_task_id.get()?;
        tasks.with(|list| list.iter().find(|task| task.id == id).cloned())
    });
    // Memo dedups the 700 ms polls, so this only fires when the log actually grows,
    // and only scrolls when the user is already at the bottom.
    create_effect(move |_| {
        let _ = expanded_task.get();
        if !stick.get_untracked() {
            return;
        }
        set_timeout(
            move || {
                if let Some(node) = terminal_ref.get_untracked() {
                    node.set_scroll_top(node.scroll_height());
                }
            },
            Duration::from_millis(16),
        );
    });

    view! {
        <div
            class=move || {
                if open.get() {
                    "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-100 transition-opacity duration-300"
                } else {
                    "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-0 pointer-events-none transition-opacity duration-300"
                }
            }
            on:click=move |_| open.set(false)
        >
            <div
                class=move || {
                    if open.get() {
                        "w-full max-w-[40rem] h-[min(32rem,calc(100vh-6rem))] bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-100 transition-transform duration-300 flex flex-col"
                    } else {
                        "w-full max-w-[40rem] h-[min(32rem,calc(100vh-6rem))] bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-95 transition-transform duration-300 flex flex-col"
                    }
                }
                on:click=|ev| ev.stop_propagation()
            >
                <div class="flex items-start justify-between gap-4 px-4 py-3 border-b border-border">
                    <div class="min-w-0">
                        <h2 class="text-base font-semibold text-content">{move || t("repos.clone_tasks_title")}</h2>
                        <p class="mt-1 text-sm text-muted">{move || t("repos.clone_tasks_subtitle")}</p>
                    </div>
                    <button
                        type="button"
                        title=move || t("common.cancel")
                        aria-label=move || t("common.cancel")
                        class="inline-flex items-center justify-center size-8 rounded-md text-muted hover:bg-bg hover:text-content focus:outline-none"
                        on:click=move |_| open.set(false)
                    >
                        <Icon name=IconName::Close class="size-4" />
                    </button>
                </div>

                <div class="flex items-center gap-2 px-3 py-2 border-b border-border">
                    <input
                        type="text"
                        class="flex-1 min-w-0 h-10 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none"
                        placeholder=move || t("repos.clone_search_placeholder")
                        prop:value=move || search.get()
                        on:input=move |ev| search.set(event_target_value(&ev))
                    />
                    <FilterDropdown
                        options=range_options
                        selected=Signal::derive(move || range.get())
                        on_select=Callback::new(move |value| range.set(value))
                    />
                </div>

                <div class="min-h-0 flex-1 overflow-y-auto px-3 py-3">
                    <Show
                        when=move || !tasks.get().is_empty()
                        fallback=move || view! {
                            <div class="px-3 py-10 text-center text-sm text-muted">
                                {move || t("repos.clone_tasks_empty")}
                            </div>
                        }
                    >
                        <Show
                            when=move || !filtered.get().is_empty()
                            fallback=move || view! {
                                <div class="px-3 py-10 text-center text-sm text-muted">
                                    {move || t("repos.clone_no_match")}
                                </div>
                            }
                        >
                            <div class="space-y-1">
                                <For
                                    each=move || filtered.get()
                                    key=|task| task.id
                                    children=move |task| {
                                        let id = task.id;
                                        // Read live by id: <For> keeps existing children, so the
                                        // streaming log/status must come from the signal, not the snapshot.
                                        let task = create_memo(move |_| {
                                            tasks.with(|list| list.iter().find(|task| task.id == id).cloned())
                                        });
                                        view! {
                                            <div class="rounded-lg overflow-hidden">
                                                <button
                                                    type="button"
                                                    class="group w-full rounded-lg px-3 py-1 text-left hover:bg-bg focus:outline-none focus:bg-bg"
                                                    on:click=move |_| {
                                                        if selected_task_id.get_untracked() == Some(id) {
                                                            selected_task_id.set(None);
                                                        } else {
                                                            stick.set(true);
                                                            selected_task_id.set(Some(id));
                                                        }
                                                    }
                                                >
                                                    <div class="flex items-center justify-between gap-3">
                                                        <div class="min-w-0 truncate text-xs font-medium text-content">
                                                            {move || task.get().map(|task| task.repo_full_name).unwrap_or_default()}
                                                        </div>
                                                        <span class=move || {
                                                            task.get()
                                                                .map(|task| clone_status_class(&task.status))
                                                                .unwrap_or_default()
                                                        }>
                                                            {move || {
                                                                task.get()
                                                                    .map(|task| clone_status_label(&task.status))
                                                                    .unwrap_or_default()
                                                            }}
                                                        </span>
                                                    </div>
                                                </button>
                                                <Show when=move || selected_task_id.get() == Some(id)>
                                                    <div class="px-3 pb-2 pt-1">
                                                        <div class="truncate text-[11px] text-muted">
                                                            {move || task.get().map(|task| task.local_path).unwrap_or_default()}
                                                        </div>
                                                        <div
                                                            node_ref=terminal_ref
                                                            class="mt-1 max-h-48 overflow-y-auto overscroll-contain rounded-md border border-zinc-800 bg-zinc-950 px-3 py-2"
                                                            on:scroll=move |_| {
                                                                if let Some(node) = terminal_ref.get_untracked() {
                                                                    let distance = node.scroll_height() - node.scroll_top() - node.client_height();
                                                                    stick.set(distance <= 40);
                                                                }
                                                            }
                                                        >
                                                            <pre class="m-0 whitespace-pre-wrap break-words font-mono text-[12px] leading-5 text-blue-100">{move || {
                                                                task.get()
                                                                    .map(|task| compact_clone_log(&task.log))
                                                                    .filter(|log| !log.is_empty())
                                                                    .unwrap_or_else(|| t("repos.clone_no_log").to_string())
                                                            }}</pre>
                                                        </div>
                                                    </div>
                                                </Show>
                                            </div>
                                        }
                                    }
                                />
                            </div>
                        </Show>
                    </Show>
                </div>

                <div class="flex items-center justify-between gap-3 border-t border-border px-4 py-3">
                    <button
                        type="button"
                        class="px-3 py-2 text-sm font-medium rounded-lg text-muted hover:bg-bg hover:text-content focus:outline-none disabled:opacity-50"
                        prop:disabled=move || !has_clearable.get()
                        on:click=move |_| on_clear.call(())
                    >
                        {move || t("repos.clone_clear")}
                    </button>
                    <button
                        type="button"
                        class="px-4 py-2 text-sm font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none"
                        on:click=move |_| open.set(false)
                    >
                        {move || t("common.cancel")}
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]
fn BindKeyDialog(
    repo: RwSignal<Option<Repo>>,
    keys: RwSignal<Vec<SshKey>>,
    loading: RwSignal<bool>,
    submitting: RwSignal<bool>,
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
                            {move || t("repos.bind_dialog_submit")}
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
