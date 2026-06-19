//! SSH Keys management screen.
//!
//! A standalone local SSH key manager: create SSH key pairs with Ed25519/RSA,
//! list them in a table, copy public keys, and delete. Keys are stored in
//! `~/.ssh/deploykeys/<directory>/` with isolated directories per key.

use crate::api::{self, SshKey};
use crate::connection::connection_state;
use crate::i18n::t;
use crate::icons::{Icon, IconName};
use crate::page_size::page_size;
use crate::progress::ProgressHandle;
use crate::toast::ToastHandle;
use leptos::*;
use std::time::Duration;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, Copy)]
struct TableDragState {
    pointer_id: i32,
    start_x: i32,
    start_y: i32,
    scroll_left: i32,
    scroll_top: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CreatedAtSort {
    Desc,
    Asc,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyIconState {
    Copy,
    Copied,
}

#[component]
pub fn Keys(#[allow(unused_variables)] pending_count: RwSignal<usize>) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let toast = ToastHandle::expect();
    let conn = connection_state();
    let has_connection = Signal::derive(move || conn.has_active());
    let keys = RwSignal::new(Vec::<SshKey>::new());
    let loading = RwSignal::new(false);
    let query = RwSignal::new(String::new());
    let algorithm_filter = RwSignal::new(String::new());
    let created_from = RwSignal::new(String::new());
    let created_to = RwSignal::new(String::new());
    let page = RwSignal::new(1_usize);
    let missing_key_confirm = RwSignal::new(None::<i64>);
    let table_scroll_ref = NodeRef::<html::Div>::new();
    let table_drag = RwSignal::new(None::<TableDragState>);
    let created_at_sort = RwSignal::new(CreatedAtSort::Desc);

    // SSH keys live on the active connection's filesystem. With no connection
    // there is nothing to manage, so clear the list and skip loading; the load
    // (re)runs automatically whenever a connection is established.
    create_effect(move |_| {
        if !conn.has_active() {
            keys.set(Vec::new());
            loading.set(false);
            return;
        }
        loading.set(true);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::list_ssh_keys().await {
                Ok(list) => keys.set(list),
                Err(e) => toast.error(e),
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
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::list_ssh_keys().await {
                Ok(list) => keys.set(list),
                Err(e) => toast.error(e),
            }
            loading.set(false);
            progress.end_simulated(&sim);
        });
    };

    // Filtered keys by search query, then sorted by created time before paging.
    let filtered = Signal::derive(move || {
        let q = query.get().to_lowercase();
        let algorithm = algorithm_filter.get();
        let from = created_from.get();
        let to = created_to.get();
        let mut items = keys
            .get()
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
            .collect::<Vec<_>>();

        items.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        if created_at_sort.get() == CreatedAtSort::Desc {
            items.reverse();
        }

        items
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
    let set_created_at_sort = move |value: CreatedAtSort| {
        if created_at_sort.get_untracked() != value {
            created_at_sort.set(value);
            page.set(1);
        }
    };
    let toggle_created_at_sort = move |_| {
        let next = match created_at_sort.get_untracked() {
            CreatedAtSort::Desc => CreatedAtSort::Asc,
            CreatedAtSort::Asc => CreatedAtSort::Desc,
        };
        set_created_at_sort(next);
    };
    let reset_page = Callback::new(move |_| page.set(1));
    let clear_table_drag = move |pointer_id: i32| {
        if let Some(drag) = table_drag.get_untracked() {
            if drag.pointer_id == pointer_id {
                table_drag.set(None);
            }
        }
    };

    let copy_public_key =
        move |id: i64, icon_state: RwSignal<CopyIconState>, busy: RwSignal<bool>| {
            if busy.get_untracked() {
                return;
            }

            busy.set(true);
            icon_state.set(CopyIconState::Copy);

            let sim = progress.begin_simulated();
            spawn_local(async move {
                match api::ssh_key_files_exist(id).await {
                    Ok(true) => {}
                    Ok(false) => {
                        missing_key_confirm.set(Some(id));
                        progress.end_simulated(&sim);
                        busy.set(false);
                        return;
                    }
                    Err(e) => {
                        toast.error(e);
                        progress.end_simulated(&sim);
                        busy.set(false);
                        return;
                    }
                }

                match api::copy_public_key_to_clipboard(id).await {
                    Ok(()) => {
                        icon_state.set(CopyIconState::Copied);
                        toast.success(t("keys.copy_success"));
                        set_timeout(
                            move || icon_state.set(CopyIconState::Copy),
                            Duration::from_secs(1),
                        );
                    }
                    Err(e) => {
                        leptos::logging::error!("Failed to copy public key: {}", e);
                        toast.error(e);
                    }
                }
                progress.end_simulated(&sim);
                busy.set(false);
            });
        };

    let delete_key = move |id: i64| {
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::delete_ssh_key(id).await {
                Ok(_) => {
                    toast.success(t("keys.delete_success"));
                    refresh();
                }
                Err(e) => {
                    leptos::logging::error!("Failed to delete key: {}", e);
                    toast.error(e);
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
                    title=move || if has_connection.get() { String::new() } else { t("connect.required_hint").to_string() }
                    class="shrink-0 py-2 px-4 text-sm font-medium rounded-lg border border-border bg-primary text-on-primary hover:bg-primary-hover focus:outline-none transition-colors disabled:opacity-50 disabled:pointer-events-none"
                    prop:disabled=move || !has_connection.get()
                    on:click=move |_| create_dialog_open.set(true)
                >
                    {move || t("keys.create")}
                </button>
            </div>

            <Show
                when=move || has_connection.get()
                fallback=move || view! {
                    <div class="flex flex-1 items-center justify-center py-16 text-center">
                        <p class="text-sm text-muted">{move || t("keys.no_connection")}</p>
                    </div>
                }
            >
            <div class="flex items-center gap-2 min-w-0">
                <div class="flex-1 min-w-0">
                    <input
                        type="text"
                        class="w-full min-w-0 py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none"
                        placeholder=move || t("keys.search_placeholder")
                        prop:value=move || query.get()
                        on:input=move |ev| set_query(event_target_value(&ev))
                    />
                </div>
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
                                            <table class="min-w-[50rem] w-full table-fixed border-collapse text-sm">
                                            <thead class="sticky top-0 z-10 bg-surface">
                                                <tr class="border-b border-border">
                                                    <th class="w-[8rem] min-w-[8rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                        {move || t("keys.directory")}
                                                    </th>
                                                    <th class="w-[9rem] min-w-[9rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                        {move || t("keys.algorithm")}
                                                    </th>
                                                    <th class="w-[12rem] min-w-[12rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                        {move || t("keys.remark")}
                                                    </th>
                                                    <th class="w-[12rem] min-w-[12rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                        <div class="flex items-center gap-1.5">
                                                            <span>{move || t("keys.created_at")}</span>
                                                            <button
                                                                type="button"
                                                                title=move || {
                                                                    if created_at_sort.get() == CreatedAtSort::Asc {
                                                                        t("keys.sort_created_at_desc")
                                                                    } else {
                                                                        t("keys.sort_created_at_asc")
                                                                    }
                                                                }
                                                                aria-label=move || {
                                                                    if created_at_sort.get() == CreatedAtSort::Asc {
                                                                        t("keys.sort_created_at_desc")
                                                                    } else {
                                                                        t("keys.sort_created_at_asc")
                                                                    }
                                                                }
                                                                class="inline-flex size-5 items-center justify-center text-muted hover:text-content focus:outline-none"
                                                                on:click=toggle_created_at_sort
                                                            >
                                                                <span class="text-[10px] leading-none text-content">
                                                                    {move || if created_at_sort.get() == CreatedAtSort::Asc { "▲" } else { "▼" }}
                                                                </span>
                                                            </button>
                                                        </div>
                                                    </th>
                                                    <th class="sticky right-0 z-20 w-[8rem] min-w-[8rem] bg-surface text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle relative">
                                                        <span class="pointer-events-none absolute inset-y-0 left-0 w-px bg-border"></span>
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
                                                    let created_at = key.created_at.clone();
                                                    let remark = key.remark.clone();
                                                    let remark_for_dialog = remark.clone();
                                                    let delete_confirm_open = RwSignal::new(false);
                                                    let edit_open = RwSignal::new(false);
                                                    let copy_icon_state = RwSignal::new(CopyIconState::Copy);
                                                    let copy_busy = RwSignal::new(false);
                                                    view! {
                                                        <tr class="group border-b border-border last:border-b-0 hover:bg-bg align-middle">
                                                            <td class="w-[8rem] min-w-[8rem] max-w-[8rem] px-3 py-2 align-middle">
                                                                <TruncatedCellText
                                                                    display=directory.clone()
                                                                    tooltip=directory
                                                                    class="font-medium text-content font-mono"
                                                                />
                                                            </td>
                                                            <td class="w-[9rem] min-w-[9rem] px-3 py-2 whitespace-nowrap align-middle">
                                                                <span class="inline-flex items-center text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">
                                                                    {algorithm}
                                                                </span>
                                                            </td>
                                                            <td class="w-[12rem] min-w-[12rem] max-w-[12rem] px-3 py-2 align-middle">
                                                                <TruncatedCellText
                                                                    display=if remark.is_empty() { "—".to_string() } else { remark.clone() }
                                                                    tooltip=remark
                                                                    class="text-muted"
                                                                />
                                                            </td>
                                                            <td class="w-[12rem] min-w-[12rem] px-3 py-2 text-muted whitespace-nowrap align-middle">
                                                                <TruncatedCellText
                                                                    display=created_at.clone()
                                                                    tooltip=created_at
                                                                    class="text-muted"
                                                                />
                                                            </td>
                                                            <td class="sticky right-0 z-[1] min-w-[8rem] bg-surface px-3 py-2 group-hover:bg-bg relative align-middle">
                                                                <span class="pointer-events-none absolute inset-y-0 left-0 w-px bg-border"></span>
                                                                <div class="inline-flex min-w-max items-center gap-1.5">
                                                                    <button
                                                                        type="button"
                                                                        title=move || {
                                                                            if copy_icon_state.get() == CopyIconState::Copied {
                                                                                t("keys.copy_success")
                                                                            } else {
                                                                                t("keys.copy_public_key")
                                                                            }
                                                                        }
                                                                        aria-label=move || {
                                                                            if copy_icon_state.get() == CopyIconState::Copied {
                                                                                t("keys.copy_success")
                                                                            } else {
                                                                                t("keys.copy_public_key")
                                                                            }
                                                                        }
                                                                        class=move || {
                                                                            if copy_icon_state.get() == CopyIconState::Copied {
                                                                                "inline-flex items-center justify-center size-8 rounded-md text-emerald-600 hover:bg-primary-soft dark:text-emerald-400 dark:hover:bg-primary-soft/60 focus:outline-none transition-colors disabled:pointer-events-none disabled:opacity-100"
                                                                            } else {
                                                                                "inline-flex items-center justify-center size-8 rounded-md text-primary hover:bg-primary-soft focus:outline-none transition-colors disabled:pointer-events-none disabled:opacity-100"
                                                                            }
                                                                        }
                                                                        prop:disabled=move || copy_busy.get()
                                                                        on:click=move |_| copy_public_key(key_id, copy_icon_state, copy_busy)
                                                                    >
                                                                        <CopyPublicKeyIcon state=copy_icon_state />
                                                                    </button>
                                                                    <button
                                                                        type="button"
                                                                        title=move || t("keys.edit")
                                                                        aria-label=move || t("keys.edit")
                                                                        class="inline-flex items-center justify-center size-8 rounded-md text-content hover:bg-primary-soft dark:hover:bg-primary-soft/60 focus:outline-none"
                                                                        on:click=move |_| edit_open.set(true)
                                                                    >
                                                                        <Icon name=IconName::Edit class="size-4" />
                                                                    </button>
                                                                    <button
                                                                        type="button"
                                                                        title=move || t("keys.delete")
                                                                        aria-label=move || t("keys.delete")
                                                                        class="inline-flex items-center justify-center size-8 rounded-md text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950 focus:outline-none"
                                                                        on:click=move |_| delete_confirm_open.set(true)
                                                                    >
                                                                        <Icon name=IconName::Delete class="size-4" />
                                                                    </button>
                                                                </div>
                                                            </td>
                                                        </tr>

                                                        <div class=move || {
                                                            if delete_confirm_open.get() {
                                                                "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-100 transition-opacity duration-300"
                                                            } else {
                                                                "fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4 opacity-0 pointer-events-none transition-opacity duration-300"
                                                            }
                                                        }
                                                            on:click=move |_| delete_confirm_open.set(false)
                                                        >
                                                            <div
                                                                class=move || {
                                                                    if delete_confirm_open.get() {
                                                                        "w-full max-w-md bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-100 transition-transform duration-300"
                                                                    } else {
                                                                        "w-full max-w-md bg-surface border border-border rounded-xl shadow-2xl overflow-hidden scale-95 transition-transform duration-300"
                                                                    }
                                                                }
                                                                on:click=|ev| ev.stop_propagation()
                                                            >
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
fn CopyPublicKeyIcon(state: RwSignal<CopyIconState>) -> impl IntoView {
    view! {
        <span class="relative inline-flex size-4 items-center justify-center">
            <span class=move || {
                if state.get() == CopyIconState::Copy {
                    "absolute inset-0 inline-flex items-center justify-center opacity-100 scale-100 rotate-0 transition-all duration-200 ease-out"
                } else {
                    "absolute inset-0 inline-flex items-center justify-center opacity-0 scale-75 -rotate-45 transition-all duration-200 ease-out"
                }
            }>
                <Icon name=IconName::Copy class="size-4" />
            </span>
            <span class=move || {
                if state.get() == CopyIconState::Copied {
                    "absolute inset-0 inline-flex items-center justify-center opacity-100 scale-100 rotate-0 transition-all duration-200 ease-out"
                } else {
                    "absolute inset-0 inline-flex items-center justify-center opacity-0 scale-75 rotate-45 transition-all duration-200 ease-out"
                }
            }>
                <Icon name=IconName::CopyDone class="size-4" />
            </span>
        </span>
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

#[component]
pub fn FilterDropdown(
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
pub fn FormSelectDropdown(
    #[prop(into)] options: Signal<Vec<(String, String)>>,
    #[prop(into)] selected: Signal<String>,
    on_select: Callback<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    create_effect(move |_| {
        if disabled.get() {
            open.set(false);
        }
    });

    let label = Signal::derive(move || {
        let selected_value = selected.get();
        options
            .get()
            .into_iter()
            .find(|(value, _)| *value == selected_value)
            .map(|(_, display)| display)
            .unwrap_or_default()
    });

    view! {
        <div class="relative">
            <button
                type="button"
                class="w-full inline-flex items-center justify-between gap-2 py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content hover:bg-surface focus:outline-none focus:ring-1 focus:ring-primary disabled:opacity-50 disabled:pointer-events-none"
                prop:disabled=move || disabled.get()
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <span class="min-w-0 truncate text-left">{move || label.get()}</span>
                <span class=move || {
                    if open.get() {
                        "inline-flex text-muted rotate-90 transition-transform duration-200"
                    } else {
                        "inline-flex text-muted transition-transform duration-200"
                    }
                }>
                    <Icon name=IconName::ChevronRight class="size-4" />
                </span>
            </button>

            <Show when=move || open.get()>
                <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                <div class="absolute start-0 mt-1 z-50 w-full p-1 bg-surface border border-border rounded-xl shadow-xl">
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
                                    class="w-full flex items-center gap-x-2 py-2 px-2.5 rounded-lg text-sm text-content hover:bg-bg focus:outline-none focus:bg-bg"
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
    let draft_from = RwSignal::new(None::<SimpleDate>);
    let draft_to = RwSignal::new(None::<SimpleDate>);
    let calendar_month = RwSignal::new(initial_calendar_month(from, to));

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

    let open_picker = move |_| {
        draft_from.set(parse_date(&from.get_untracked()));
        draft_to.set(parse_date(&to.get_untracked()));
        calendar_month.set(initial_calendar_month(from, to));
        open.update(|o| *o = !*o);
    };

    let clear = move || {
        draft_from.set(None);
        draft_to.set(None);
        from.set(String::new());
        to.set(String::new());
        on_change.call(());
    };

    let apply = move || {
        from.set(
            draft_from
                .get_untracked()
                .map(format_date)
                .unwrap_or_default(),
        );
        to.set(
            draft_to
                .get_untracked()
                .map(format_date)
                .unwrap_or_default(),
        );
        open.set(false);
        on_change.call(());
    };

    let cancel = move || {
        open.set(false);
        draft_from.set(parse_date(&from.get_untracked()));
        draft_to.set(parse_date(&to.get_untracked()));
    };

    view! {
        <div class="relative shrink-0">
            <button
                type="button"
                class="inline-flex items-center justify-center w-36 max-w-full h-10 px-3 text-sm rounded-lg border border-border bg-bg text-content hover:bg-surface focus:outline-none whitespace-nowrap"
                on:click=open_picker
            >
                <span class="min-w-0 truncate">{move || label.get()}</span>
            </button>

            <Show when=move || open.get()>
                <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                <div class="absolute end-0 mt-1 z-50 w-[min(calc(100vw-2rem),15rem)] max-w-[calc(100vw-2rem)] bg-surface border border-border shadow-xl rounded-xl overflow-hidden">
                    <DateRangeCalendar
                        month=calendar_month
                        draft_from=draft_from
                        draft_to=draft_to
                    />

                    <div class="py-2.5 px-3 flex items-center justify-between gap-x-2 border-t border-border">
                        <button
                            type="button"
                            class="py-1.5 px-2.5 inline-flex items-center text-xs font-medium rounded-lg border border-border bg-bg text-muted hover:bg-surface focus:outline-none"
                            on:click=move |_| clear()
                        >
                            {move || t("search.clear")}
                        </button>
                        <div class="flex items-center justify-end gap-x-2">
                            <button
                                type="button"
                                class="py-1.5 px-2.5 inline-flex items-center text-xs font-medium rounded-lg border border-border bg-bg text-content hover:bg-surface focus:outline-none"
                                on:click=move |_| cancel()
                            >
                                {move || t("common.cancel")}
                            </button>
                            <button
                                type="button"
                                class="py-1.5 px-2.5 inline-flex items-center text-xs font-medium rounded-lg border border-transparent bg-primary text-on-primary hover:bg-primary-hover focus:outline-none"
                                on:click=move |_| apply()
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
fn DateRangeCalendar(
    month: RwSignal<YearMonth>,
    draft_from: RwSignal<Option<SimpleDate>>,
    draft_to: RwSignal<Option<SimpleDate>>,
) -> impl IntoView {
    let month_label = Signal::derive(move || {
        format!(
            "{} / {}",
            t(month_name_key(month.get().month)),
            month.get().year
        )
    });
    let days = Signal::derive(move || calendar_cells(month.get()));

    let select_date = move |date: SimpleDate| {
        let from = draft_from.get_untracked();
        let to = draft_to.get_untracked();

        match (from, to) {
            (None, _) | (Some(_), Some(_)) => {
                draft_from.set(Some(date));
                draft_to.set(None);
            }
            (Some(start), None) if date < start => {
                draft_from.set(Some(date));
                draft_to.set(Some(start));
            }
            (Some(start), None) if date == start => {
                draft_from.set(Some(date));
                draft_to.set(None);
            }
            (Some(_), None) => draft_to.set(Some(date)),
        }
    };

    view! {
        <div class="p-2.5 space-y-1">
            <div class="grid grid-cols-5 items-center gap-x-2 mx-1 pb-2">
                <div class="col-span-1">
                    <button
                        type="button"
                        class="size-6 flex justify-center items-center text-content hover:bg-bg rounded-full disabled:opacity-50 disabled:pointer-events-none focus:outline-none focus:bg-bg"
                        aria-label=move || t("date.previous_month")
                        on:click=move |_| month.update(|m| *m = m.previous())
                    >
                        <Icon name=IconName::ChevronLeft class="size-4" />
                    </button>
                </div>

                <div class="col-span-3 flex justify-center items-center">
                    <span class="text-sm font-medium text-content whitespace-nowrap">{move || month_label.get()}</span>
                </div>

                <div class="col-span-1 flex justify-end">
                    <button
                        type="button"
                        class="size-6 flex justify-center items-center text-content hover:bg-bg rounded-full disabled:opacity-50 disabled:pointer-events-none focus:outline-none focus:bg-bg"
                        aria-label=move || t("date.next_month")
                        on:click=move |_| month.update(|m| *m = m.next())
                    >
                        <Icon name=IconName::ChevronRight class="size-4" />
                    </button>
                </div>
            </div>

            <div class="grid grid-cols-7 pb-1">
                <For
                    each=week_labels
                    key=|label| *label
                    children=move |label| {
                        view! {
                            <span class="m-px block text-center text-[11px] text-muted">{label}</span>
                        }
                    }
                />
            </div>

            <div class="grid grid-cols-7">
                <For
                    each=move || days.get()
                    key=|cell| format!("{}-{}-{}", cell.date.year, cell.date.month, cell.date.day)
                    children=move |cell| {
                        let date = cell.date;
                        let is_current_month = cell.current_month;
                        let is_selected = move || {
                            draft_from.get() == Some(date) || draft_to.get() == Some(date)
                        };
                        let in_range = move || {
                            match (draft_from.get(), draft_to.get()) {
                                (Some(start), Some(end)) => start < date && date < end,
                                _ => false,
                            }
                        };

                        view! {
                            <div class=move || {
                                if in_range() {
                                    "bg-primary-soft"
                                } else {
                                    "bg-transparent"
                                }
                            }>
                                <button
                                    type="button"
                                    class=move || {
                                        let base = "m-px size-5 flex justify-center items-center border border-transparent text-xs rounded-full focus:outline-none disabled:opacity-40 disabled:pointer-events-none";
                                        if is_selected() {
                                            format!("{base} bg-primary text-on-primary hover:bg-primary-hover")
                                        } else if is_current_month {
                                            format!("{base} text-content hover:border-primary hover:text-primary focus:border-primary focus:text-primary")
                                        } else {
                                            format!("{base} text-muted opacity-50 hover:border-border")
                                        }
                                    }
                                    on:click=move |_| select_date(date)
                                >
                                    {date.day}
                                </button>
                            </div>
                        }
                    }
                />
            </div>
        </div>
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SimpleDate {
    year: i32,
    month: u32,
    day: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct YearMonth {
    year: i32,
    month: u32,
}

impl YearMonth {
    fn previous(self) -> Self {
        if self.month == 1 {
            Self {
                year: self.year - 1,
                month: 12,
            }
        } else {
            Self {
                year: self.year,
                month: self.month - 1,
            }
        }
    }

    fn next(self) -> Self {
        if self.month == 12 {
            Self {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Self {
                year: self.year,
                month: self.month + 1,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CalendarCell {
    date: SimpleDate,
    current_month: bool,
}

fn initial_calendar_month(from: RwSignal<String>, to: RwSignal<String>) -> YearMonth {
    parse_date(&from.get_untracked())
        .or_else(|| parse_date(&to.get_untracked()))
        .map(|date| YearMonth {
            year: date.year,
            month: date.month,
        })
        .unwrap_or_else(current_year_month)
}

fn current_year_month() -> YearMonth {
    let now = js_sys::Date::new_0();
    YearMonth {
        year: now.get_full_year() as i32,
        month: now.get_month() + 1,
    }
}

fn parse_date(value: &str) -> Option<SimpleDate> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    if day > days_in_month(year, month) {
        return None;
    }
    Some(SimpleDate { year, month, day })
}

fn format_date(date: SimpleDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

fn calendar_cells(month: YearMonth) -> Vec<CalendarCell> {
    let first_weekday = weekday_index_monday(month.year, month.month, 1);
    let current_days = days_in_month(month.year, month.month);
    let previous = month.previous();
    let previous_days = days_in_month(previous.year, previous.month);

    let mut cells = Vec::with_capacity(42);
    for i in 0..first_weekday {
        cells.push(CalendarCell {
            date: SimpleDate {
                year: previous.year,
                month: previous.month,
                day: previous_days - first_weekday + i + 1,
            },
            current_month: false,
        });
    }
    for day in 1..=current_days {
        cells.push(CalendarCell {
            date: SimpleDate {
                year: month.year,
                month: month.month,
                day,
            },
            current_month: true,
        });
    }
    let next = month.next();
    let mut day = 1;
    while cells.len() < 42 {
        cells.push(CalendarCell {
            date: SimpleDate {
                year: next.year,
                month: next.month,
                day,
            },
            current_month: false,
        });
        day += 1;
    }
    cells
}

fn week_labels() -> Vec<&'static str> {
    vec![
        t("date.week_monday"),
        t("date.week_tuesday"),
        t("date.week_wednesday"),
        t("date.week_thursday"),
        t("date.week_friday"),
        t("date.week_saturday"),
        t("date.week_sunday"),
    ]
}

fn month_name_key(month: u32) -> &'static str {
    match month {
        1 => "date.month_january",
        2 => "date.month_february",
        3 => "date.month_march",
        4 => "date.month_april",
        5 => "date.month_may",
        6 => "date.month_june",
        7 => "date.month_july",
        8 => "date.month_august",
        9 => "date.month_september",
        10 => "date.month_october",
        11 => "date.month_november",
        _ => "date.month_december",
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Monday = 0, Sunday = 6.
fn weekday_index_monday(year: i32, month: u32, day: u32) -> u32 {
    let (year, month) = if month < 3 {
        (year - 1, month + 12)
    } else {
        (year, month)
    };
    let k = year % 100;
    let j = year / 100;
    let h = (day as i32 + ((13 * (month as i32 + 1)) / 5) + k + (k / 4) + (j / 4) + (5 * j)) % 7;
    ((h + 5) % 7) as u32
}

/// Shared classes for pager icon buttons (prev/next).
const PAGER_CELL: &str = "inline-flex items-center justify-center min-w-[2rem] h-8 px-2 text-sm rounded-md text-muted hover:bg-surface hover:text-content disabled:opacity-40 disabled:pointer-events-none";

/// Full pagination bar below the list: total, prev/next with page indicator,
/// page-size, and "go to page" input. Kept in sync with the Repos page.
#[component]
pub fn PaginationBar(
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
fn FieldLabelWithHelp(
    #[prop(into)] label: Signal<&'static str>,
    #[prop(into)] help: Signal<&'static str>,
) -> impl IntoView {
    view! {
        <div class="mb-1.5 flex items-center gap-1.5">
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
                <span class="pointer-events-none absolute bottom-full left-1/2 z-[80] mb-2 hidden w-max -translate-x-1/2 whitespace-nowrap rounded-lg border border-border bg-surface px-3 py-2 text-left text-xs leading-5 text-content shadow-xl group-hover/help:block group-focus-within/help:block">
                    {move || help.get()}
                </span>
            </span>
        </div>
    }
}

#[component]
fn CreateKeyDialog(open: RwSignal<bool>, on_created: Callback<()>) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let toast = ToastHandle::expect();
    let directory = RwSignal::new(String::new());
    let comment = RwSignal::new(String::new());
    let remark = RwSignal::new(String::new());
    let algorithm = RwSignal::new("ed25519".to_string());
    let creating = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let algorithm_options = Signal::derive(move || {
        vec![
            (
                "ed25519".to_string(),
                t("keys.algorithm_ed25519").to_string(),
            ),
            (
                "rsa2048".to_string(),
                t("keys.algorithm_rsa2048").to_string(),
            ),
            (
                "rsa4096".to_string(),
                t("keys.algorithm_rsa4096").to_string(),
            ),
        ]
    });

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
                    toast.success(t("keys.create_success"));
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
                        "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-visible scale-100 transition-transform duration-300"
                    } else {
                        "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-visible scale-95 transition-transform duration-300"
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

                        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                            // Directory
                            <div class="min-w-0">
                                <FieldLabelWithHelp
                                    label=Signal::derive(move || t("keys.dialog_directory_label"))
                                    help=Signal::derive(move || t("keys.dialog_directory_help"))
                                />
                                <input
                                    type="text"
                                    class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary font-mono"
                                    placeholder=move || t("keys.dialog_directory_placeholder")
                                    prop:value=move || directory.get()
                                    on:input=move |ev| directory.set(event_target_value(&ev))
                                    prop:disabled=move || creating.get()
                                />
                            </div>

                            // Algorithm
                            <div class="min-w-0">
                                <FieldLabelWithHelp
                                    label=Signal::derive(move || t("keys.dialog_algorithm_label"))
                                    help=Signal::derive(move || t("keys.dialog_algorithm_help"))
                                />
                                <FormSelectDropdown
                                    options=algorithm_options
                                    selected=Signal::derive(move || algorithm.get())
                                    on_select=Callback::new(move |value| algorithm.set(value))
                                    disabled=Signal::derive(move || creating.get())
                                />
                            </div>
                        </div>

                        // Key Comment
                        <div>
                            <FieldLabelWithHelp
                                label=Signal::derive(move || t("keys.dialog_comment_label"))
                                help=Signal::derive(move || t("keys.dialog_comment_help"))
                            />
                            <input
                                type="email"
                                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary font-mono"
                                placeholder=move || t("keys.dialog_comment_placeholder")
                                prop:value=move || comment.get()
                                on:input=move |ev| comment.set(event_target_value(&ev))
                                prop:disabled=move || creating.get()
                            />
                        </div>

                        // Remark
                        <div>
                            <label class="block text-sm font-medium text-content mb-1.5">
                                {move || t("keys.dialog_remark_label")}
                            </label>
                            <input
                                type="text"
                                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary"
                                placeholder=move || t("keys.dialog_remark_placeholder")
                                prop:value=move || remark.get()
                                on:input=move |ev| remark.set(event_target_value(&ev))
                                prop:disabled=move || creating.get()
                            />
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
    let toast = ToastHandle::expect();
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
                    toast.success(t("keys.edit_success"));
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
                        "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-visible scale-100 transition-transform duration-300"
                    } else {
                        "w-full max-w-lg bg-surface border border-border rounded-xl shadow-2xl overflow-visible scale-95 transition-transform duration-300"
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
                            <FieldLabelWithHelp
                                label=Signal::derive(move || t("keys.dialog_directory_label"))
                                help=Signal::derive(move || t("keys.dialog_directory_help"))
                            />
                            <input
                                type="text"
                                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary font-mono"
                                placeholder=move || t("keys.dialog_directory_placeholder")
                                prop:value=move || directory.get()
                                on:input=move |ev| directory.set(event_target_value(&ev))
                                prop:disabled=move || saving.get()
                            />
                        </div>

                        // Remark
                        <div>
                            <label class="block text-sm font-medium text-content mb-1.5">
                                {move || t("keys.dialog_remark_label")}
                            </label>
                            <input
                                type="text"
                                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary"
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
