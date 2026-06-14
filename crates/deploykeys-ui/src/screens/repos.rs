//! Repositories list screen.
//!
//! Renders the locally-synced repositories as a flat, GitHub-like list with a
//! name search box and visibility + language filters. Filtering is entirely
//! client-side over the synced rows; "Refresh" re-syncs from GitHub and reloads.
//! The list is gated on being signed in: signing out clears it, and a
//! signed-out "Refresh" routes to the sign-in screen instead of erroring.

use crate::api::{self, Repo};
use crate::i18n::t;
use crate::icons::{Icon, IconName};
use crate::page_size::page_size;
use crate::progress::ProgressHandle;
use leptos::*;
use wasm_bindgen_futures::spawn_local;
/// Sentinel value for the "no language" bucket in the language filter.
const OTHER: &str = "\u{1}other";

#[component]
pub fn Repos(
    #[allow(unused_variables)] pending_count: RwSignal<usize>,
    account: RwSignal<Option<api::Account>>,
    on_sign_in_hint: Callback<()>,
) -> impl IntoView {
    let progress = ProgressHandle::expect();
    let repos = RwSignal::new(Vec::<Repo>::new());
    let loading = RwSignal::new(false);
    let syncing = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

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
        filtered.get().into_iter().skip(start).take(size).collect::<Vec<_>>()
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

    let open_repo = move |url: String| {
        let sim = progress.begin_simulated();
        spawn_local(async move {
            let _ = api::open_url(&url).await;
            progress.end_simulated(&sim);
        });
    };

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
                            <div class="flex flex-col gap-4 flex-1 min-h-0">
                            <ul class="flex flex-col gap-2 flex-1 overflow-y-auto min-h-0">
                                <For
                                    each=move || paged.get()
                                    key=|r| r.full_name.clone()
                                    children=move |r| {
                                        let Repo { full_name, private, language, archived, html_url, .. } = r;
                                        view! {
                                            <li
                                                class="flex items-center gap-3 py-3 px-4 rounded-lg border border-border bg-surface hover:bg-bg cursor-pointer transition-colors"
                                                on:click={
                                                    let url = html_url.clone();
                                                    move |_| open_repo(url.clone())
                                                }
                                            >
                                                <span class="font-medium text-content truncate">{full_name}</span>
                                                <span class="shrink-0 text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">
                                                    {move || if private { t("repos.private") } else { t("repos.public") }}
                                                </span>
                                                {language.map(|l| {
                                                    let dot = format!("background-color: {}", language_color(&l));
                                                    view! {
                                                        <span class="shrink-0 inline-flex items-center gap-1.5 text-xs text-muted">
                                                            <span class="inline-block size-2.5 rounded-full" style=dot></span>
                                                            {l}
                                                        </span>
                                                    }
                                                })}
                                                {archived.then(|| view! {
                                                    <span class="shrink-0 text-[11px] py-0.5 px-2 rounded-full border border-border text-muted">
                                                        {move || t("repos.archived")}
                                                    </span>
                                                })}
                                            </li>
                                        }
                                    }
                                />
                            </ul>
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

    view! {
        <div class="relative">
            // Trigger aligns in height with the search input (py-2), text not icon.
            <button
                type="button"
                class="inline-flex items-center justify-center w-36 h-10 px-3 text-sm rounded-lg border border-border bg-bg text-content hover:bg-surface focus:outline-none transition-colors whitespace-nowrap"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <span class="min-w-0 truncate">{move || label.get()}</span>
            </button>

            <Show when=move || open.get()>
                <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                // Panel: content-width, rows half the height of the language switcher.
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
                                    class="w-full flex items-center gap-x-2 py-1 px-2.5 rounded-lg text-sm text-content hover:bg-bg focus:outline-none focus:bg-bg transition-colors whitespace-nowrap"
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
const PAGER_CELL: &str = "inline-flex items-center justify-center min-w-[2rem] h-8 px-2 text-sm rounded-md text-muted transition-colors hover:bg-surface hover:text-content disabled:opacity-40 disabled:pointer-events-none";

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
                    class="inline-flex items-center justify-center gap-1 min-w-[3.25rem] h-8 px-2.5 text-sm rounded-md text-content hover:bg-surface focus:outline-none transition-colors"
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
                                                "w-full flex items-center justify-center h-8 px-2.5 rounded-lg text-sm text-content hover:bg-bg transition-colors"
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