//! Connection management screen.
//!
//! Lists the connections the app can operate through and lets the user pick the
//! single active one. Keys and repository actions run against whichever
//! connection is connected. For now the only connection is the local machine
//! (this device); server connections arrive in a later stage.
//!
//! The page layout mirrors the SSH Keys screen exactly (title row, search +
//! filters, scrollable table, pagination) so the two list screens feel
//! identical.

use crate::connection::{connection_state, Connection, ConnectionKind};
use crate::i18n::t;
use crate::icons::{Icon, IconName};
use crate::page_size::page_size;
use crate::screens::keys::{FilterDropdown, PaginationBar};
use crate::toast::ToastHandle;
use leptos::*;
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

fn kind_code(kind: ConnectionKind) -> &'static str {
    match kind {
        ConnectionKind::Local => "local",
    }
}

#[component]
pub fn Connect(#[allow(unused_variables)] pending_count: RwSignal<usize>) -> impl IntoView {
    let state = connection_state();
    let connections = state.connections;
    let connected_id = state.connected_id;

    let query = RwSignal::new(String::new());
    let type_filter = RwSignal::new(String::new());
    let status_filter = RwSignal::new(String::new());
    let page = RwSignal::new(1_usize);
    let table_scroll_ref = NodeRef::<html::Div>::new();
    let table_drag = RwSignal::new(None::<TableDragState>);

    let type_options = Signal::derive(move || {
        vec![
            (String::new(), t("connect.filter_type_all").to_string()),
            ("local".to_string(), t("connect.type_local").to_string()),
        ]
    });
    let status_options = Signal::derive(move || {
        vec![
            (String::new(), t("connect.filter_status_all").to_string()),
            (
                "connected".to_string(),
                t("connect.status_connected").to_string(),
            ),
            (
                "offline".to_string(),
                t("connect.status_offline").to_string(),
            ),
        ]
    });

    // Filtered connections by search query + type + status before paging.
    let filtered = Signal::derive(move || {
        let q = query.get().trim().to_lowercase();
        let type_f = type_filter.get();
        let status_f = status_filter.get();
        let connected = connected_id.get();
        connections
            .get()
            .into_iter()
            .filter(|c| {
                let name = t(c.kind.name_key()).to_lowercase();
                let typ = t(c.kind.type_key()).to_lowercase();
                let matches_query = q.is_empty() || name.contains(&q) || typ.contains(&q);
                let matches_type = type_f.is_empty() || kind_code(c.kind) == type_f;
                let is_connected = connected.as_deref() == Some(c.id.as_str());
                let matches_status = status_f.is_empty()
                    || (status_f == "connected" && is_connected)
                    || (status_f == "offline" && !is_connected);
                matches_query && matches_type && matches_status
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

    // Correct the page index when filters or the shared page size change.
    create_effect(move |_| {
        let max = page_count.get().max(1);
        page.update(|p| *p = (*p).clamp(1, max));
    });

    // Persist page size changes to the backend, matching the keys/repos pages.
    create_effect(move |_| {
        let size = page_size().get();
        spawn_local(async move {
            if let Err(e) = crate::api::set_page_size(size).await {
                leptos::logging::warn!("Failed to persist page size: {e}");
            }
        });
    });

    let set_query = move |value: String| {
        query.set(value);
        page.set(1);
    };
    let set_type_filter = move |value: String| {
        type_filter.set(value);
        page.set(1);
    };
    let set_status_filter = move |value: String| {
        status_filter.set(value);
        page.set(1);
    };
    let clear_table_drag = move |pointer_id: i32| {
        if let Some(drag) = table_drag.get_untracked() {
            if drag.pointer_id == pointer_id {
                table_drag.set(None);
            }
        }
    };

    view! {
        <div class="flex flex-col gap-5 h-full">
            <div class="flex items-center justify-between gap-3">
                <h1 class="text-2xl font-semibold text-content">{move || t("connect.title")}</h1>
                <button
                    type="button"
                    title=move || t("connect.add_help")
                    class="shrink-0 py-2 px-4 text-sm font-medium rounded-lg border border-border bg-primary text-on-primary hover:bg-primary-hover focus:outline-none transition-colors disabled:opacity-50"
                    prop:disabled=true
                >
                    {move || t("connect.add")}
                </button>
            </div>

            <div class="flex items-center gap-2 min-w-0">
                <div class="flex-1 min-w-0">
                    <input
                        type="text"
                        class="w-full min-w-0 py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none"
                        placeholder=move || t("connect.search_placeholder")
                        prop:value=move || query.get()
                        on:input=move |ev| set_query(event_target_value(&ev))
                    />
                </div>
                <FilterDropdown
                    options=type_options
                    selected=Signal::derive(move || type_filter.get())
                    on_select=Callback::new(set_type_filter)
                />
                <FilterDropdown
                    options=status_options
                    selected=Signal::derive(move || status_filter.get())
                    on_select=Callback::new(set_status_filter)
                />
            </div>

            <Show
                when=move || !filtered.get().is_empty()
                fallback=move || view! { <p class="text-sm text-muted">{move || t("connect.no_match")}</p> }
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
                                <table class="min-w-[35rem] w-full table-fixed border-collapse text-sm">
                                    <thead class="sticky top-0 z-10 bg-surface">
                                        <tr class="border-b border-border">
                                            <th class="min-w-[12rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                {move || t("connect.name")}
                                            </th>
                                            <th class="w-[5rem] min-w-[5rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                {move || t("connect.type")}
                                            </th>
                                            <th class="w-[10rem] min-w-[10rem] text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle">
                                                {move || t("connect.status")}
                                            </th>
                                            <th class="sticky right-0 z-20 w-[8rem] min-w-[8rem] bg-surface text-start font-medium text-muted px-3 py-2 whitespace-nowrap align-middle relative">
                                                <span class="pointer-events-none absolute inset-y-0 left-0 w-px bg-border"></span>
                                                {move || t("connect.actions")}
                                            </th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <For
                                            each=move || paged.get()
                                            key=|c| c.id.clone()
                                            children=move |conn| view! { <ConnectionRow conn=conn /> }
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
        </div>
    }
}

#[component]
fn ConnectionRow(conn: Connection) -> impl IntoView {
    let state = connection_state();
    let id = conn.id.clone();
    let name_key = conn.kind.name_key();
    let type_key = conn.kind.type_key();

    let is_connected = {
        let id = id.clone();
        Signal::derive(move || state.is_connected(&id))
    };

    let toggle = {
        let id = id.clone();
        move |_| {
            let toast = ToastHandle::expect();
            let name = t(name_key);
            if state.is_connected(&id) {
                state.disconnect(&id);
                toast.success(t("connect.disconnect_success").replace("{}", name));
            } else {
                state.connect(&id);
                toast.success(t("connect.connect_success").replace("{}", name));
            }
        }
    };

    view! {
        <tr class="group border-b border-border hover:bg-bg align-middle">
            <td class="min-w-[12rem] px-3 py-2 align-middle">
                <div class="flex items-center gap-2.5 min-w-0">
                    <Icon name=IconName::Monitor class="size-4 shrink-0 text-muted" />
                    <span class="block max-w-full truncate whitespace-nowrap font-medium text-content">
                        {move || t(name_key)}
                    </span>
                </div>
            </td>
            <td class="w-[5rem] min-w-[5rem] px-3 py-2 whitespace-nowrap align-middle">
                <span class="inline-flex items-center text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">
                    {move || t(type_key)}
                </span>
            </td>
            <td class="w-[10rem] min-w-[10rem] px-3 py-2 whitespace-nowrap align-middle">
                <span class=move || {
                    if is_connected.get() {
                        "inline-flex items-center gap-1.5 text-[11px] py-0.5 px-2 rounded-full border border-green-200 bg-green-50 text-green-700 dark:border-green-900 dark:bg-green-950 dark:text-green-300"
                    } else {
                        "inline-flex items-center gap-1.5 text-[11px] py-0.5 px-2 rounded-full border border-border text-muted"
                    }
                }>
                    <span class=move || {
                        if is_connected.get() {
                            "size-1.5 rounded-full bg-green-500"
                        } else {
                            "size-1.5 rounded-full bg-muted"
                        }
                    }></span>
                    {move || if is_connected.get() { t("connect.status_connected") } else { t("connect.status_offline") }}
                </span>
            </td>
            <td class="sticky right-0 z-[1] min-w-[8rem] bg-surface px-3 py-2 group-hover:bg-bg relative align-middle">
                <span class="pointer-events-none absolute inset-y-0 left-0 w-px bg-border"></span>
                <div class="inline-flex min-w-max items-center gap-1.5">
                    <button
                        type="button"
                        title=move || if is_connected.get() { t("connect.disconnect") } else { t("connect.connect") }
                        aria-label=move || if is_connected.get() { t("connect.disconnect") } else { t("connect.connect") }
                        class=move || {
                            if is_connected.get() {
                                "inline-flex items-center justify-center size-8 rounded-md text-green-600 hover:bg-green-50 dark:text-green-400 dark:hover:bg-green-950 focus:outline-none transition-colors"
                            } else {
                                "inline-flex items-center justify-center size-8 rounded-md text-muted hover:bg-bg hover:text-content focus:outline-none transition-colors"
                            }
                        }
                        on:click=toggle
                    >
                        <Icon name=IconName::Power class="size-4" />
                    </button>
                    <button
                        type="button"
                        title=move || t("connect.local_locked")
                        aria-label=move || t("connect.edit")
                        class="inline-flex items-center justify-center size-8 rounded-md text-content hover:bg-primary-soft dark:hover:bg-primary-soft/60 focus:outline-none disabled:opacity-50 disabled:pointer-events-none"
                        prop:disabled=true
                    >
                        <Icon name=IconName::Edit class="size-4" />
                    </button>
                    <button
                        type="button"
                        title=move || t("connect.local_locked")
                        aria-label=move || t("connect.delete")
                        class="inline-flex items-center justify-center size-8 rounded-md text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950 focus:outline-none disabled:opacity-50 disabled:pointer-events-none"
                        prop:disabled=true
                    >
                        <Icon name=IconName::Delete class="size-4" />
                    </button>
                </div>
            </td>
        </tr>
    }
}
