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

use crate::api;
use crate::connection::{connection_state, Connection, ConnectionKind};
use crate::i18n::t;
use crate::icons::{Icon, IconName};
use crate::page_size::page_size;
use crate::progress::ProgressHandle;
use crate::screens::keys::{FilterDropdown, FormSelectDropdown, PaginationBar};
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
        ConnectionKind::Remote => "remote",
    }
}

#[component]
pub fn Connect(#[allow(unused_variables)] pending_count: RwSignal<usize>) -> impl IntoView {
    let state = connection_state();
    let progress = ProgressHandle::expect();
    let toast = ToastHandle::expect();
    let connections = state.connections;
    let connected_id = state.connected_id;

    let query = RwSignal::new(String::new());
    let type_filter = RwSignal::new(String::new());
    let status_filter = RwSignal::new(String::new());
    let page = RwSignal::new(1_usize);
    let table_scroll_ref = NodeRef::<html::Div>::new();
    let table_drag = RwSignal::new(None::<TableDragState>);
    let add_dialog_open = RwSignal::new(false);
    let loading = RwSignal::new(false);

    let refresh = move || {
        if loading.get_untracked() {
            return;
        }
        loading.set(true);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::list_connections().await {
                Ok(list) => state.set_connections(
                    list.into_iter()
                        .map(Connection::from_dto)
                        .collect::<Vec<_>>(),
                ),
                Err(e) => toast.error(e),
            }
            loading.set(false);
            progress.end_simulated(&sim);
        });
    };

    create_effect(move |_| refresh());

    let type_options = Signal::derive(move || {
        vec![
            (String::new(), t("connect.filter_type_all").to_string()),
            ("local".to_string(), t("connect.type_local").to_string()),
            ("remote".to_string(), t("connect.type_remote").to_string()),
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
                let name = connection_name(c).to_lowercase();
                let typ = connection_type_label(c.kind).to_lowercase();
                let host = c.host.clone().unwrap_or_default().to_lowercase();
                let user = c.username.clone().unwrap_or_default().to_lowercase();
                let matches_query = q.is_empty()
                    || name.contains(&q)
                    || typ.contains(&q)
                    || host.contains(&q)
                    || user.contains(&q);
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
                    on:click=move |_| add_dialog_open.set(true)
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
                                            children=move |conn| {
                                                view! {
                                                    <ConnectionRow
                                                        conn=conn
                                                        on_changed=Callback::new(move |_| refresh())
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

            <AddConnectionDialog
                open=add_dialog_open
                on_created=Callback::new(move |_| refresh())
            />
        </div>
    }
}

#[component]
fn ConnectionRow(conn: Connection, on_changed: Callback<()>) -> impl IntoView {
    let state = connection_state();
    let progress = ProgressHandle::expect();
    let toast = ToastHandle::expect();
    let id = conn.id.clone();
    let name = connection_name(&conn);
    let type_label = connection_type_label(conn.kind);
    let subtitle = connection_subtitle(&conn);
    let has_subtitle = !subtitle.is_empty();
    let is_local = conn.kind == ConnectionKind::Local;
    let icon_name = if is_local {
        IconName::Monitor
    } else {
        IconName::Server
    };
    let busy = RwSignal::new(false);
    let edit_open = RwSignal::new(false);
    let delete_confirm_open = RwSignal::new(false);

    let is_connected = {
        let id = id.clone();
        Signal::derive(move || state.is_connected(&id))
    };

    let toggle = {
        let id = id.clone();
        let name = name.clone();
        move |_| {
            if busy.get_untracked() {
                return;
            }
            if state.is_connected(&id) {
                busy.set(true);
                let sim = progress.begin_simulated();
                let id_for_state = id.clone();
                let name = name.clone();
                spawn_local(async move {
                    match api::set_active_connection("").await {
                        Ok(()) => {
                            state.disconnect(&id_for_state);
                            toast.success(t("connect.disconnect_success").replace("{}", &name));
                        }
                        Err(e) => toast.error(e),
                    }
                    busy.set(false);
                    progress.end_simulated(&sim);
                });
            } else {
                busy.set(true);
                let sim = progress.begin_simulated();
                let id_for_state = id.clone();
                let name = name.clone();
                spawn_local(async move {
                    match api::set_active_connection(&id_for_state).await {
                        Ok(()) => {
                            state.connect(&id_for_state);
                            toast.success(t("connect.connect_success").replace("{}", &name));
                        }
                        Err(e) => toast.error(e),
                    }
                    busy.set(false);
                    progress.end_simulated(&sim);
                });
            }
        }
    };
    view! {
        <tr class="group border-b border-border hover:bg-bg align-middle">
            <td class="min-w-[12rem] px-3 py-2 align-middle">
                <div class="flex items-center gap-2.5 min-w-0">
                    <Icon name=icon_name class="size-4 shrink-0 text-muted" />
                    <div class="min-w-0">
                        <span class="block max-w-full truncate whitespace-nowrap font-medium text-content">
                            {name.clone()}
                        </span>
                        <Show when=move || has_subtitle>
                            <span class="block max-w-full truncate whitespace-nowrap text-xs text-muted">
                                {subtitle.clone()}
                            </span>
                        </Show>
                    </div>
                </div>
            </td>
            <td class="w-[5rem] min-w-[5rem] px-3 py-2 whitespace-nowrap align-middle">
                <span class="inline-flex items-center text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">
                    {type_label}
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
                        prop:disabled=move || busy.get()
                    >
                        <Icon name=IconName::Power class="size-4" />
                    </button>
                    <button
                        type="button"
                        title=move || if is_local { t("connect.local_locked") } else { t("connect.edit") }
                        aria-label=move || t("connect.edit")
                        class="inline-flex items-center justify-center size-8 rounded-md text-content hover:bg-primary-soft dark:hover:bg-primary-soft/60 focus:outline-none disabled:opacity-50 disabled:pointer-events-none"
                        prop:disabled=move || is_local || busy.get()
                        on:click=move |_| edit_open.set(true)
                    >
                        <Icon name=IconName::Edit class="size-4" />
                    </button>
                    <button
                        type="button"
                        title=move || if is_local { t("connect.local_locked") } else { t("connect.delete") }
                        aria-label=move || t("connect.delete")
                        class="inline-flex items-center justify-center size-8 rounded-md text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950 focus:outline-none disabled:opacity-50 disabled:pointer-events-none"
                        prop:disabled=move || is_local || busy.get() || is_connected.get()
                        on:click=move |_| delete_confirm_open.set(true)
                    >
                        <Icon name=IconName::Delete class="size-4" />
                    </button>
                </div>
            </td>
        </tr>

        <Show when=move || delete_confirm_open.get()>
            <div class="fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm flex items-center justify-center px-4" on:click=move |_| delete_confirm_open.set(false)>
                <div class="w-full max-w-md bg-surface border border-border rounded-xl shadow-2xl overflow-hidden" on:click=|ev| ev.stop_propagation()>
                    <div class="px-6 py-5">
                        <h2 class="text-base font-semibold text-content">{move || t("connect.delete_confirm_title")}</h2>
                        <p class="mt-2 text-sm text-muted">{move || t("connect.delete_confirm_message")}</p>
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
                            on:click={
                                let id = id.clone();
                                move |_| {
                                    delete_confirm_open.set(false);
                                    if busy.get_untracked() {
                                        return;
                                    }
                                    busy.set(true);
                                    let sim = progress.begin_simulated();
                                    let id = id.clone();
                                    spawn_local(async move {
                                        match api::delete_connection(&id).await {
                                            Ok(()) => {
                                                toast.success(t("connect.delete_success"));
                                                on_changed.call(());
                                            }
                                            Err(e) => toast.error(e),
                                        }
                                        busy.set(false);
                                        progress.end_simulated(&sim);
                                    });
                                }
                            }
                        >
                            {move || t("common.confirm")}
                        </button>
                    </div>
                </div>
            </div>
        </Show>

        <EditConnectionDialog
            open=edit_open
            conn=conn
            on_updated=on_changed
        />
    }
}

fn connection_name(conn: &Connection) -> String {
    if conn.kind == ConnectionKind::Local {
        t("connect.local_name").to_string()
    } else {
        conn.alias.clone()
    }
}

fn connection_type_label(kind: ConnectionKind) -> &'static str {
    match kind {
        ConnectionKind::Local => t("connect.type_local"),
        ConnectionKind::Remote => t("connect.type_remote"),
    }
}

fn connection_subtitle(conn: &Connection) -> String {
    if conn.kind == ConnectionKind::Local {
        return String::new();
    }
    match (&conn.username, &conn.host, conn.port) {
        (Some(user), Some(host), Some(port)) => format!("{user}@{host}:{port}"),
        (Some(user), Some(host), None) => format!("{user}@{host}"),
        (_, Some(host), Some(port)) => format!("{host}:{port}"),
        (_, Some(host), None) => host.clone(),
        _ => String::new(),
    }
}

#[derive(Clone)]
struct ConnectionTestResult {
    success: bool,
    message: String,
}

fn test_bubble_class(success: bool) -> &'static str {
    if success {
        "pointer-events-auto absolute bottom-full left-0 z-[120] mb-2 inline-block max-h-40 w-max max-w-[min(22rem,calc(100vw-3rem))] overflow-y-auto whitespace-normal break-words [overflow-wrap:anywhere] rounded-lg border border-border bg-surface py-2 pl-3 pr-8 text-left text-xs leading-5 text-green-700 shadow-xl dark:text-green-300"
    } else {
        "pointer-events-auto absolute bottom-full left-0 z-[120] mb-2 inline-block max-h-40 w-max max-w-[min(22rem,calc(100vw-3rem))] overflow-y-auto whitespace-normal break-words [overflow-wrap:anywhere] rounded-lg border border-border bg-surface py-2 pl-3 pr-8 text-left text-xs leading-5 text-red-700 shadow-xl dark:text-red-300"
    }
}

#[component]
fn ConnectionTestBubble(result: RwSignal<Option<ConnectionTestResult>>) -> impl IntoView {
    view! {
        <Show when=move || result.get().is_some()>
            <span class=move || test_bubble_class(result.get().map(|result| result.success).unwrap_or(false))>
                <span class="absolute -bottom-1 left-5 size-2 rotate-45 border-b border-r border-border bg-surface"></span>
                <span>{move || result.get().map(|result| result.message).unwrap_or_default()}</span>
                <button
                    type="button"
                    class="absolute right-1.5 top-1.5 inline-flex size-5 items-center justify-center rounded text-muted hover:bg-bg hover:text-content focus:outline-none focus:ring-1 focus:ring-primary"
                    aria-label=move || t("palette.close")
                    on:click=move |_| result.set(None)
                >
                    <Icon name=IconName::Close class="size-3.5" />
                </button>
            </span>
        </Show>
    }
}

#[component]
fn AddConnectionDialog(open: RwSignal<bool>, on_created: Callback<()>) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let toast = ToastHandle::expect();
    let alias = RwSignal::new(String::new());
    let host = RwSignal::new(String::new());
    let port = RwSignal::new("22".to_string());
    let username = RwSignal::new(String::new());
    let auth_method = RwSignal::new("password".to_string());
    let auth_secret = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);
    let testing = RwSignal::new(false);
    let test_result = RwSignal::new(None::<ConnectionTestResult>);
    let auth_options = Signal::derive(move || {
        vec![
            (
                "password".to_string(),
                t("connect.auth_password").to_string(),
            ),
            ("ssh_key".to_string(), t("connect.auth_ssh_key").to_string()),
        ]
    });

    create_effect(move |_| {
        if open.get() {
            alias.set(String::new());
            host.set(String::new());
            port.set("22".to_string());
            username.set(String::new());
            auth_method.set("password".to_string());
            auth_secret.set(String::new());
            test_result.set(None);
        }
    });

    let test_connection = move |_| {
        if submitting.get_untracked() || testing.get_untracked() {
            return;
        }

        let host_val = host.get_untracked().trim().to_string();
        let username_val = username.get_untracked().trim().to_string();
        let auth_method_val = auth_method.get_untracked();
        let auth_secret_val = auth_secret.get_untracked().trim().to_string();
        let port_val = match port.get_untracked().trim().parse::<u16>() {
            Ok(value) if value > 0 => value,
            _ => {
                test_result.set(Some(ConnectionTestResult {
                    success: false,
                    message: t("connect.port_invalid").to_string(),
                }));
                return;
            }
        };

        if host_val.is_empty() || username_val.is_empty() || auth_secret_val.is_empty() {
            test_result.set(Some(ConnectionTestResult {
                success: false,
                message: t("connect.test_required").to_string(),
            }));
            return;
        }

        testing.set(true);
        test_result.set(None);
        spawn_local(async move {
            match api::test_remote_connection_config(
                None,
                host_val,
                port_val,
                username_val,
                auth_method_val,
                auth_secret_val,
            )
            .await
            {
                Ok(_) => test_result.set(Some(ConnectionTestResult {
                    success: true,
                    message: t("connect.test_success").to_string(),
                })),
                Err(e) => test_result.set(Some(ConnectionTestResult {
                    success: false,
                    message: e,
                })),
            }
            testing.set(false);
        });
    };

    let submit = move |_| {
        let alias_val = alias.get_untracked().trim().to_string();
        let host_val = host.get_untracked().trim().to_string();
        let username_val = username.get_untracked().trim().to_string();
        let auth_method_val = auth_method.get_untracked();
        let auth_secret_val = auth_secret.get_untracked().trim().to_string();
        let port_val = match port.get_untracked().trim().parse::<u16>() {
            Ok(value) if value > 0 => value,
            _ => {
                toast.error(t("connect.port_invalid"));
                return;
            }
        };

        if alias_val.is_empty()
            || host_val.is_empty()
            || username_val.is_empty()
            || auth_secret_val.is_empty()
        {
            toast.error(t("connect.remote_required"));
            return;
        }

        submitting.set(true);
        let sim = progress.begin_simulated();
        spawn_local(async move {
            match api::create_remote_connection(
                alias_val,
                host_val,
                port_val,
                username_val,
                auth_method_val,
                auth_secret_val,
            )
            .await
            {
                Ok(_) => {
                    open.set(false);
                    on_created.call(());
                    toast.success(t("connect.create_success"));
                }
                Err(e) => toast.error(e),
            }
            submitting.set(false);
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
                class="w-full max-w-xl bg-surface border border-border rounded-xl shadow-2xl overflow-visible"
                on:click=|ev| ev.stop_propagation()
            >
                <div class="px-6 py-5 border-b border-border">
                    <h2 class="text-base font-semibold text-content">{move || t("connect.dialog_title")}</h2>
                </div>
                <div class="px-6 py-5 space-y-4">
                    <TextInput label_key="connect.alias" value=alias placeholder_key="connect.alias_placeholder" disabled=submitting />
                    <div class="grid grid-cols-1 sm:grid-cols-5 gap-4">
                        <div class="sm:col-span-4">
                            <TextInput label_key="connect.host" value=host placeholder_key="connect.host_placeholder" disabled=submitting />
                        </div>
                        <div class="sm:col-span-1">
                            <TextInput label_key="connect.port" value=port placeholder_key="connect.port_placeholder" disabled=submitting />
                        </div>
                    </div>
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                        <TextInput label_key="connect.username" value=username placeholder_key="connect.username_placeholder" disabled=submitting />
                        <div class="min-w-0">
                            <label class="block text-sm font-medium text-content mb-1.5">{move || t("connect.auth_method")}</label>
                            <FormSelectDropdown
                                options=auth_options
                                selected=Signal::derive(move || auth_method.get())
                                on_select=Callback::new(move |value| {
                                    if auth_method.get_untracked() != value {
                                        auth_method.set(value);
                                        auth_secret.set(String::new());
                                    }
                                })
                                disabled=Signal::derive(move || submitting.get())
                            />
                        </div>
                    </div>
                    <Show
                        when=move || auth_method.get() == "ssh_key"
                        fallback=move || view! {
                            <TextInput
                                label_key="connect.password"
                                value=auth_secret
                                placeholder_key="connect.password_placeholder"
                                input_type="password"
                                disabled=submitting
                            />
                        }
                    >
                        <SshKeyFileInput value=auth_secret disabled=submitting />
                    </Show>
                </div>
                <div class="flex flex-col gap-3 px-6 py-4 border-t border-border sm:flex-row sm:items-center sm:justify-between">
                    <div class="flex min-w-0 items-center">
                        <span class="relative inline-flex shrink-0">
                            <button
                                type="button"
                                class="px-3 py-2 text-sm font-medium rounded-lg border border-border bg-bg text-content hover:text-primary focus:outline-none disabled:opacity-50"
                                on:click=test_connection
                                prop:disabled=move || submitting.get() || testing.get()
                            >
                                {move || if testing.get() { t("connect.testing") } else { t("connect.test") }}
                            </button>
                            <ConnectionTestBubble result=test_result />
                        </span>
                    </div>
                    <div class="flex justify-end gap-2">
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-bg text-content hover:text-primary focus:outline-none disabled:opacity-50"
                            on:click=move |_| open.set(false)
                            prop:disabled=move || submitting.get()
                        >
                            {move || t("common.cancel")}
                        </button>
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none disabled:opacity-50"
                            on:click=submit
                            prop:disabled=move || submitting.get()
                        >
                            {move || if submitting.get() { t("connect.creating") } else { t("connect.create") }}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn EditConnectionDialog(
    open: RwSignal<bool>,
    conn: Connection,
    on_updated: Callback<()>,
) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let toast = ToastHandle::expect();
    let conn_id = conn.id.clone();
    let initial_auth_method = conn
        .auth_method
        .clone()
        .unwrap_or_else(|| "password".to_string());
    let alias = RwSignal::new(String::new());
    let host = RwSignal::new(String::new());
    let port = RwSignal::new("22".to_string());
    let username = RwSignal::new(String::new());
    let auth_method = RwSignal::new(initial_auth_method.clone());
    let auth_secret = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);
    let testing = RwSignal::new(false);
    let test_result = RwSignal::new(None::<ConnectionTestResult>);
    let auth_options = Signal::derive(move || {
        vec![
            (
                "password".to_string(),
                t("connect.auth_password").to_string(),
            ),
            ("ssh_key".to_string(), t("connect.auth_ssh_key").to_string()),
        ]
    });

    create_effect({
        let conn = conn.clone();
        let initial_auth_method = initial_auth_method.clone();
        move |_| {
            if open.get() {
                alias.set(conn.alias.clone());
                host.set(conn.host.clone().unwrap_or_default());
                port.set(conn.port.unwrap_or(22).to_string());
                username.set(conn.username.clone().unwrap_or_default());
                auth_method.set(initial_auth_method.clone());
                auth_secret.set(String::new());
                test_result.set(None);
            }
        }
    });

    let test_connection = {
        let initial_auth_method = initial_auth_method.clone();
        let conn_id = conn_id.clone();
        move |_| {
            if submitting.get_untracked() || testing.get_untracked() {
                return;
            }

            let host_val = host.get_untracked().trim().to_string();
            let username_val = username.get_untracked().trim().to_string();
            let auth_method_val = auth_method.get_untracked();
            let auth_secret_val = auth_secret.get_untracked().trim().to_string();
            let port_val = match port.get_untracked().trim().parse::<u16>() {
                Ok(value) if value > 0 => value,
                _ => {
                    test_result.set(Some(ConnectionTestResult {
                        success: false,
                        message: t("connect.port_invalid").to_string(),
                    }));
                    return;
                }
            };

            if host_val.is_empty() || username_val.is_empty() {
                test_result.set(Some(ConnectionTestResult {
                    success: false,
                    message: t("connect.test_required_edit").to_string(),
                }));
                return;
            }
            if auth_method_val != initial_auth_method && auth_secret_val.is_empty() {
                test_result.set(Some(ConnectionTestResult {
                    success: false,
                    message: t("connect.auth_secret_required").to_string(),
                }));
                return;
            }

            testing.set(true);
            test_result.set(None);
            let conn_id = conn_id.clone();
            spawn_local(async move {
                match api::test_remote_connection_config(
                    Some(conn_id),
                    host_val,
                    port_val,
                    username_val,
                    auth_method_val,
                    auth_secret_val,
                )
                .await
                {
                    Ok(_) => test_result.set(Some(ConnectionTestResult {
                        success: true,
                        message: t("connect.test_success").to_string(),
                    })),
                    Err(e) => test_result.set(Some(ConnectionTestResult {
                        success: false,
                        message: e,
                    })),
                }
                testing.set(false);
            });
        }
    };

    let submit = {
        let initial_auth_method = initial_auth_method.clone();
        move |_| {
            let alias_val = alias.get_untracked().trim().to_string();
            let host_val = host.get_untracked().trim().to_string();
            let username_val = username.get_untracked().trim().to_string();
            let auth_method_val = auth_method.get_untracked();
            let auth_secret_val = auth_secret.get_untracked().trim().to_string();
            let port_val = match port.get_untracked().trim().parse::<u16>() {
                Ok(value) if value > 0 => value,
                _ => {
                    toast.error(t("connect.port_invalid"));
                    return;
                }
            };

            if alias_val.is_empty() || host_val.is_empty() || username_val.is_empty() {
                toast.error(t("connect.edit_required"));
                return;
            }
            if auth_method_val != initial_auth_method && auth_secret_val.is_empty() {
                toast.error(t("connect.auth_secret_required"));
                return;
            }

            submitting.set(true);
            let sim = progress.begin_simulated();
            let conn_id = conn_id.clone();
            spawn_local(async move {
                match api::update_remote_connection(
                    conn_id,
                    alias_val,
                    host_val,
                    port_val,
                    username_val,
                    auth_method_val,
                    auth_secret_val,
                )
                .await
                {
                    Ok(_) => {
                        open.set(false);
                        on_updated.call(());
                        toast.success(t("connect.update_success"));
                    }
                    Err(e) => toast.error(e),
                }
                submitting.set(false);
                progress.end_simulated(&sim);
            });
        }
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
                class="w-full max-w-xl bg-surface border border-border rounded-xl shadow-2xl overflow-visible"
                on:click=|ev| ev.stop_propagation()
            >
                <div class="px-6 py-5 border-b border-border">
                    <h2 class="text-base font-semibold text-content">{move || t("connect.dialog_title_edit")}</h2>
                </div>
                <div class="px-6 py-5 space-y-4">
                    <TextInput label_key="connect.alias" value=alias placeholder_key="connect.alias_placeholder" disabled=submitting />
                    <div class="grid grid-cols-1 sm:grid-cols-5 gap-4">
                        <div class="sm:col-span-4">
                            <TextInput label_key="connect.host" value=host placeholder_key="connect.host_placeholder" disabled=submitting />
                        </div>
                        <div class="sm:col-span-1">
                            <TextInput label_key="connect.port" value=port placeholder_key="connect.port_placeholder" disabled=submitting />
                        </div>
                    </div>
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                        <TextInput label_key="connect.username" value=username placeholder_key="connect.username_placeholder" disabled=submitting />
                        <div class="min-w-0">
                            <label class="block text-sm font-medium text-content mb-1.5">{move || t("connect.auth_method")}</label>
                            <FormSelectDropdown
                                options=auth_options
                                selected=Signal::derive(move || auth_method.get())
                                on_select=Callback::new(move |value| {
                                    if auth_method.get_untracked() != value {
                                        auth_method.set(value);
                                        auth_secret.set(String::new());
                                    }
                                })
                                disabled=Signal::derive(move || submitting.get())
                            />
                        </div>
                    </div>
                    <Show
                        when=move || auth_method.get() == "ssh_key"
                        fallback=move || view! {
                            <TextInput
                                label_key="connect.password"
                                value=auth_secret
                                placeholder_key="connect.auth_secret_keep_placeholder"
                                input_type="password"
                                disabled=submitting
                            />
                        }
                    >
                        <SshKeyFileInput
                            value=auth_secret
                            disabled=submitting
                            placeholder_key="connect.auth_secret_keep_placeholder"
                        />
                    </Show>
                </div>
                <div class="flex flex-col gap-3 px-6 py-4 border-t border-border sm:flex-row sm:items-center sm:justify-between">
                    <div class="flex min-w-0 items-center">
                        <span class="relative inline-flex shrink-0">
                            <button
                                type="button"
                                class="px-3 py-2 text-sm font-medium rounded-lg border border-border bg-bg text-content hover:text-primary focus:outline-none disabled:opacity-50"
                                on:click=test_connection
                                prop:disabled=move || submitting.get() || testing.get()
                            >
                                {move || if testing.get() { t("connect.testing") } else { t("connect.test") }}
                            </button>
                            <ConnectionTestBubble result=test_result />
                        </span>
                    </div>
                    <div class="flex justify-end gap-2">
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-bg text-content hover:text-primary focus:outline-none disabled:opacity-50"
                            on:click=move |_| open.set(false)
                            prop:disabled=move || submitting.get()
                        >
                            {move || t("common.cancel")}
                        </button>
                        <button
                            type="button"
                            class="px-4 py-2 text-sm font-medium rounded-lg bg-primary text-on-primary hover:bg-primary-hover focus:outline-none disabled:opacity-50"
                            on:click=submit
                            prop:disabled=move || submitting.get()
                        >
                            {move || if submitting.get() { t("connect.saving") } else { t("connect.save") }}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn SshKeyFileInput(
    value: RwSignal<String>,
    disabled: RwSignal<bool>,
    #[prop(into, default = MaybeSignal::Static("connect.private_key_path_placeholder"))]
    placeholder_key: MaybeSignal<&'static str>,
) -> impl IntoView {
    let toast = ToastHandle::expect();
    let choose_file = move |_| {
        if disabled.get_untracked() {
            return;
        }
        spawn_local(async move {
            match api::pick_ssh_private_key().await {
                Ok(Some(path)) => value.set(path),
                Ok(None) => {}
                Err(e) => toast.error(e),
            }
        });
    };

    view! {
        <div class="min-w-0">
            <label class="block text-sm font-medium text-content mb-1.5">{move || t("connect.private_key_path")}</label>
            <div class="flex gap-2 min-w-0">
                <input
                    type="text"
                    class="min-w-0 flex-1 py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none font-mono"
                    placeholder=move || t(placeholder_key.get())
                    prop:value=move || value.get()
                    prop:readonly=true
                    prop:disabled=move || disabled.get()
                />
                <button
                    type="button"
                    class="shrink-0 px-3 py-2 text-sm font-medium rounded-lg border border-border bg-bg text-content hover:text-primary focus:outline-none disabled:opacity-50"
                    prop:disabled=move || disabled.get()
                    on:click=choose_file
                >
                    {move || t("connect.choose_private_key")}
                </button>
            </div>
        </div>
    }
}

#[component]
fn TextInput(
    #[prop(into)] label_key: MaybeSignal<&'static str>,
    value: RwSignal<String>,
    #[prop(into)] placeholder_key: MaybeSignal<&'static str>,
    #[prop(into, default = MaybeSignal::Static("text"))] input_type: MaybeSignal<&'static str>,
    disabled: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="min-w-0">
            <label class="block text-sm font-medium text-content mb-1.5">{move || t(label_key.get())}</label>
            <input
                type=move || input_type.get()
                class="w-full py-2 px-3 text-sm rounded-lg border border-border bg-bg text-content placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-primary"
                placeholder=move || t(placeholder_key.get())
                prop:value=move || value.get()
                prop:disabled=move || disabled.get()
                on:input=move |ev| value.set(event_target_value(&ev))
            />
        </div>
    }
}
