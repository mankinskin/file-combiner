use dioxus::prelude::*;

/// Renders a labelled representation field only when the backend provided
/// values for the field.
#[component]
pub(crate) fn RepresentationList(heading: String, items: Vec<String>) -> Element {
    if items.is_empty() {
        return rsx! {};
    }
    rsx! {
        h4 { "{heading}" }
        ul {
            for item in items {
                li { "{item}" }
            }
        }
    }
}
