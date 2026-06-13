//! Icon definitions. Each icon is a pure data struct holding its SVG path(s).
//! Components render them via the `Icon` component, which wraps the <svg> shell
//! and applies theme-aware styling (currentColor for automatic light/dark).

use leptos::*;

/// Icon identifier. Extend this enum when adding new icons; the `Icon` component
/// resolves the variant to its path data at render time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Folder,
    Server,
    Key,
    Globe,
    Moon,
    Sun,
    Monitor,
    Close,
}

impl IconName {
    /// SVG path data for this icon. Returns a slice of path `d` attributes; most
    /// icons are single-path, but complex ones (e.g., Server with multiple layers)
    /// may return multiple paths rendered in sequence.
    pub fn paths(self) -> &'static [&'static str] {
        match self {
            // Folder (repository icon)
            IconName::Folder => &[
                "M13 7L11.8845 4.76892C11.5634 4.1268 11.4029 3.80573 11.1634 3.57116C10.9516 3.36373 10.6963 3.20597 10.4161 3.10931C10.0992 3 9.74021 3 9.02229 3H5.2C4.0799 3 3.51984 3 3.09202 3.21799C2.71569 3.40973 2.40973 3.71569 2.21799 4.09202C2 4.51984 2 5.0799 2 6.2V7M2 7H17.2C18.8802 7 19.7202 7 20.362 7.32698C20.9265 7.6146 21.3854 8.07354 21.673 8.63803C22 9.27976 22 10.1198 22 11.8V16.2C22 17.8802 22 18.7202 21.673 19.362C21.3854 19.9265 20.9265 20.3854 20.362 20.673C19.7202 21 18.8802 21 17.2 21H6.8C5.11984 21 4.27976 21 3.63803 20.673C3.07354 20.3854 2.6146 19.9265 2.32698 19.362C2 18.7202 2 17.8802 2 16.2V7Z",
            ],
            // Server (target/deployment icon)
            IconName::Server => &[
                "M22 10.5L21.5256 6.70463C21.3395 5.21602 21.2465 4.47169 20.8961 3.9108C20.5875 3.41662 20.1416 3.02301 19.613 2.77804C19.013 2.5 18.2629 2.5 16.7626 2.5H7.23735C5.73714 2.5 4.98704 2.5 4.38702 2.77804C3.85838 3.02301 3.4125 3.41662 3.10386 3.9108C2.75354 4.47169 2.6605 5.21601 2.47442 6.70463L2 10.5M5.5 14.5H18.5M5.5 14.5C3.567 14.5 2 12.933 2 11C2 9.067 3.567 7.5 5.5 7.5H18.5C20.433 7.5 22 9.067 22 11C22 12.933 20.433 14.5 18.5 14.5M5.5 14.5C3.567 14.5 2 16.067 2 18C2 19.933 3.567 21.5 5.5 21.5H18.5C20.433 21.5 22 19.933 22 18C22 16.067 20.433 14.5 18.5 14.5M6 11H6.01M6 18H6.01M12 11H18M12 18H18",
            ],
            // Key (used for both "Keys" nav and "Key Forge")
            IconName::Key => &[
                "M15 9H15.01M15 15C18.3137 15 21 12.3137 21 9C21 5.68629 18.3137 3 15 3C11.6863 3 9 5.68629 9 9C9 9.27368 9.01832 9.54308 9.05381 9.80704C9.11218 10.2412 9.14136 10.4583 9.12172 10.5956C9.10125 10.7387 9.0752 10.8157 9.00469 10.9419C8.937 11.063 8.81771 11.1823 8.57913 11.4209L3.46863 16.5314C3.29568 16.7043 3.2092 16.7908 3.14736 16.8917C3.09253 16.9812 3.05213 17.0787 3.02763 17.1808C3 17.2959 3 17.4182 3 17.6627V19.4C3 19.9601 3 20.2401 3.10899 20.454C3.20487 20.6422 3.35785 20.7951 3.54601 20.891C3.75992 21 4.03995 21 4.6 21H6.33726C6.58185 21 6.70414 21 6.81923 20.9724C6.92127 20.9479 7.01881 20.9075 7.10828 20.8526C7.2092 20.7908 7.29568 20.7043 7.46863 20.5314L12.5791 15.4209C12.8177 15.1823 12.937 15.063 13.0581 14.9953C13.1843 14.9248 13.2613 14.8987 13.4044 14.8783C13.5417 14.8586 13.7588 14.8878 14.193 14.9462C14.4569 14.9817 14.7263 15 15 15Z",
            ],
            // Globe (language selector)
            IconName::Globe => &[
                "M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10z",
                "M2 12h20",
                "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z",
            ],
            // Moon (dark theme)
            IconName::Moon => &["M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"],
            // Sun (light theme)
            IconName::Sun => &[
                "M12 2v2",
                "M12 20v2",
                "m4.93 4.93 1.41 1.41",
                "m17.66 17.66 1.41 1.41",
                "M2 12h2",
                "M20 12h2",
                "m6.34 17.66-1.41 1.41",
                "m19.07 4.93-1.41 1.41",
            ],
            // Monitor (system theme)
            IconName::Monitor => &[
                "M2 3h20v14H2z",
                "M8 21h8",
                "M12 17v4",
            ],
            // Close (×)
            IconName::Close => &[
                "M18 6 6 18",
                "m6 6 12 12",
            ],
        }
    }
}

/// Icon component: renders an SVG with the given name and size. Styled with
/// `currentColor` so it inherits the text color and adapts to light/dark themes
/// automatically. Additional Tailwind classes can be passed via `class`.
#[component]
pub fn Icon(
    /// Which icon to render.
    name: IconName,
    /// Additional Tailwind classes (e.g., "size-5 text-muted").
    #[prop(into, optional)]
    class: String,
) -> impl IntoView {
    let base_class = "shrink-0";
    let combined_class = if class.is_empty() {
        base_class.to_string()
    } else {
        format!("{} {}", base_class, class)
    };

    view! {
        <svg
            class=combined_class
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            xmlns="http://www.w3.org/2000/svg"
        >
            {name.paths().iter().map(|d| {
                view! { <path d=*d></path> }
            }).collect_view()}
        </svg>
    }
}
