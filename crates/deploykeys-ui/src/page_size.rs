//! Global page-size preference signal.
//!
//! Mirrors the `i18n::provide_locale` / `i18n::locale()` pattern used for the
//! persisted language. The value is provided at the app root and restored from
//! `app_settings` on startup in `app.rs`.

use leptos::*;

pub const DEFAULT_PAGE_SIZE: usize = 5;
const MAX_PAGE_SIZE: usize = 500;

pub fn provide_page_size(initial: usize) {
    provide_context(RwSignal::new(validate_page_size(initial).unwrap_or(DEFAULT_PAGE_SIZE)));
}

pub fn page_size() -> RwSignal<usize> {
    use_context::<RwSignal<usize>>().expect("page_size signal should be provided at root")
}

pub fn validate_page_size(size: usize) -> Option<usize> {
    if (1..=MAX_PAGE_SIZE).contains(&size) {
        Some(size)
    } else {
        None
    }
}
