//! Minimal reactive theming for the CSR UI.
//!
//! Mirrors the `i18n` module: a global reactive `Theme` signal provided at the
//! app root, read via `theme()`. An effect keeps the `<html>` element's `.dark`
//! class in sync with the signal so the semantic color tokens in
//! `styles/input.css` flip globally. The `System` theme tracks the OS
//! `prefers-color-scheme` live via a media-query listener.

use leptos::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl Theme {
    /// The theme code (`light`/`dark`/`system`). Reserved for a future picker
    /// and persisted preference.
    #[allow(dead_code)]
    pub fn code(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "system",
        }
    }

    #[allow(dead_code)]
    pub fn from_code(code: &str) -> Theme {
        match code {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => Theme::System,
        }
    }
}

/// Provide the theme signal at the app root and wire it to the DOM.
///
/// An effect re-runs whenever the theme changes: it resolves the effective
/// dark/light state and toggles `.dark` on `<html>`. For `System`, it also
/// registers a `prefers-color-scheme` listener so OS changes apply live; the
/// listener is torn down when the theme moves away from `System`.
pub fn provide_theme(initial: Theme) {
    let theme = RwSignal::new(initial);
    provide_context(theme);

    // Holds the active media-query listener (only set while theme == System) so
    // it stays alive and can be detached when the theme changes.
    let listener: StoredValue<Option<MediaListener>> = store_value(None);

    create_effect(move |_| {
        let theme = theme.get();

        // Drop any previous System listener before re-evaluating.
        listener.set_value(None);

        match theme {
            Theme::Light => apply_dark(false),
            Theme::Dark => apply_dark(true),
            Theme::System => {
                if let Some(mql) = prefers_dark_media_query() {
                    apply_dark(mql.matches());
                    listener.set_value(MediaListener::attach(mql));
                } else {
                    apply_dark(false);
                }
            }
        }
    });
}

/// Read the global theme signal. Reserved for a future in-app theme picker;
/// the theme is wired to the DOM by `provide_theme`'s effect regardless.
#[allow(dead_code)]
pub fn theme() -> RwSignal<Theme> {
    use_context::<RwSignal<Theme>>().expect("theme signal provided at root")
}

/// Toggle the `.dark` class on the `<html>` element.
fn apply_dark(dark: bool) {
    if let Some(root) = document_element() {
        let list = root.class_list();
        if dark {
            let _ = list.add_1("dark");
        } else {
            let _ = list.remove_1("dark");
        }
    }
}

fn document_element() -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
}

fn prefers_dark_media_query() -> Option<web_sys::MediaQueryList> {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok())
        .flatten()
}

/// A `prefers-color-scheme` change listener that re-applies `.dark` on OS theme
/// changes. Detaches itself on drop so switching away from `System` cleans up.
struct MediaListener {
    mql: web_sys::MediaQueryList,
    closure: Closure<dyn FnMut(web_sys::MediaQueryListEvent)>,
}

impl MediaListener {
    fn attach(mql: web_sys::MediaQueryList) -> Option<Self> {
        let closure = Closure::wrap(Box::new(move |e: web_sys::MediaQueryListEvent| {
            apply_dark(e.matches());
        }) as Box<dyn FnMut(_)>);
        mql.add_listener_with_opt_callback(Some(closure.as_ref().unchecked_ref()))
            .ok()?;
        Some(MediaListener { mql, closure })
    }
}

impl Drop for MediaListener {
    fn drop(&mut self) {
        let _ = self
            .mql
            .remove_listener_with_opt_callback(Some(self.closure.as_ref().unchecked_ref()));
    }
}
