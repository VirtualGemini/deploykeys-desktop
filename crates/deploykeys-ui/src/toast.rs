//! Global toast notifications, stacked in the bottom-right corner.
//!
//! Every backend-backed operation reports its outcome here: a green toast on
//! success, a red one on failure. The handle is provided once at the app root
//! (like [`crate::progress::ProgressHandle`]) so any screen can push a message
//! without prop-drilling.
//!
//! Visibility lives in the shared `toasts` vec (a single root-owned signal), not
//! in per-toast signals created at push time — a toast pushed from a screen that
//! later unmounts would otherwise reference a disposed signal. Each item reads
//! its own `visible` flag back out of that vec by id.

use crate::i18n::t;
use crate::icons::{Icon, IconName};
use leptos::*;
use std::time::Duration;

/// How long a toast stays fully shown before it begins fading out.
const SUCCESS_TTL: Duration = Duration::from_secs(4);
const ERROR_TTL: Duration = Duration::from_secs(6);
/// One frame after insertion we flip `visible` on, so the enter transition runs
/// from the off-screen state rendered first.
const ENTER_DELAY: Duration = Duration::from_millis(16);
/// Must outlast the opacity/translate transition in `container_class` so the
/// toast finishes fading before it is dropped from the DOM.
const EXIT_FADE: Duration = Duration::from_millis(220);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastTone {
    Success,
    Error,
}

#[derive(Clone)]
struct Toast {
    id: u64,
    message: String,
    tone: ToastTone,
    visible: bool,
}

/// Shared handle to the global toast stack. Cheap to copy (just signals).
#[derive(Clone, Copy)]
pub struct ToastHandle {
    toasts: RwSignal<Vec<Toast>>,
    next_id: RwSignal<u64>,
}

impl ToastHandle {
    pub fn new() -> Self {
        Self {
            toasts: RwSignal::new(Vec::new()),
            next_id: RwSignal::new(0),
        }
    }

    pub fn provide(self) {
        provide_context(self);
    }

    pub fn expect() -> Self {
        use_context::<Self>().expect("ToastHandle provided at app root")
    }

    /// Show a success (green) toast.
    pub fn success(self, message: impl Into<String>) {
        self.push(ToastTone::Success, message.into(), SUCCESS_TTL);
    }

    /// Show an error (red) toast.
    pub fn error(self, message: impl Into<String>) {
        self.push(ToastTone::Error, message.into(), ERROR_TTL);
    }

    fn push(self, tone: ToastTone, message: String, ttl: Duration) {
        let id = self.next_id.get_untracked();
        self.next_id.update(|n| *n += 1);
        self.toasts.update(|list| {
            list.push(Toast {
                id,
                message,
                tone,
                visible: false,
            })
        });

        // Enter on the next frame so the transition animates from the hidden
        // state the row first rendered with.
        set_timeout(move || self.set_visible(id, true), ENTER_DELAY);
        // Auto-dismiss once the tone's lifetime elapses.
        set_timeout(move || self.dismiss(id), ttl);
    }

    /// Begin dismissing a toast: fade it out, then drop it from the stack.
    /// Idempotent — a manual close and the auto-dismiss timer may both call it.
    pub fn dismiss(self, id: u64) {
        self.set_visible(id, false);
        let toasts = self.toasts;
        set_timeout(
            move || toasts.update(|list| list.retain(|toast| toast.id != id)),
            EXIT_FADE,
        );
    }

    fn set_visible(self, id: u64, visible: bool) {
        self.toasts.update(|list| {
            if let Some(toast) = list.iter_mut().find(|toast| toast.id == id) {
                toast.visible = visible;
            }
        });
    }
}

/// Mount once at the app root: a fixed, click-through column in the bottom-right
/// corner that stacks every active toast above all dialogs.
#[component]
pub fn ToastViewport() -> impl IntoView {
    let toasts = ToastHandle::expect().toasts;
    view! {
        <div class="pointer-events-none fixed bottom-4 right-4 z-[200] flex flex-col items-end gap-2">
            <For
                each=move || toasts.get()
                key=|toast| toast.id
                children=move |toast| {
                    view! { <ToastItem id=toast.id message=toast.message tone=toast.tone /> }
                }
            />
        </div>
    }
}

#[component]
fn ToastItem(id: u64, message: String, tone: ToastTone) -> impl IntoView {
    let handle = ToastHandle::expect();
    let toasts = handle.toasts;
    // Read this row's live `visible` flag back out of the shared vec; it flips
    // shortly after mount (enter) and again on dismissal (exit).
    let visible = Signal::derive(move || {
        toasts
            .get()
            .iter()
            .find(|toast| toast.id == id)
            .is_some_and(|toast| toast.visible)
    });
    view! {
        <div class=move || container_class(tone, visible.get())>
            <span class="min-w-0 break-words">{message}</span>
            <button
                type="button"
                title=move || t("common.cancel")
                aria-label=move || t("common.cancel")
                class=close_class(tone)
                on:click=move |_| handle.dismiss(id)
            >
                <Icon name=IconName::Close class="size-4" />
            </button>
        </div>
    }
}

fn container_class(tone: ToastTone, visible: bool) -> String {
    let base = "pointer-events-auto flex max-w-sm items-start gap-3 rounded-lg border p-3 text-sm shadow-lg transition-all duration-200 ease-out";
    let tone_class = match tone {
        ToastTone::Error => {
            "border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300"
        }
        ToastTone::Success => {
            "border-green-200 bg-green-50 text-green-700 dark:border-green-900 dark:bg-green-950 dark:text-green-300"
        }
    };
    let state = if visible {
        "opacity-100 translate-y-0"
    } else {
        "opacity-0 translate-y-2"
    };
    format!("{base} {tone_class} {state}")
}

fn close_class(tone: ToastTone) -> &'static str {
    match tone {
        ToastTone::Error => {
            "inline-flex shrink-0 items-center justify-center size-5 rounded text-red-700/70 hover:text-red-700 focus:outline-none dark:text-red-300/70 dark:hover:text-red-300"
        }
        ToastTone::Success => {
            "inline-flex shrink-0 items-center justify-center size-5 rounded text-green-700/70 hover:text-green-700 focus:outline-none dark:text-green-300/70 dark:hover:text-green-300"
        }
    }
}
