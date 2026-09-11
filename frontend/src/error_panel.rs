use dioxus::prelude::*;

use crate::models::FailureDetail;

/// Renders the request-failure alert: message, code, status, extra
/// details, and the attempted/failed diagnostics trace when present.
#[component]
pub(crate) fn RequestErrorPanel(failure: FailureDetail) -> Element {
    rsx! {
        div { class: "request-error", role: "alert",
            p { class: "request-error-message", "{failure.message}" }
            ul { class: "request-error-details",
                li { "Code: {failure.code}" }
                if let Some(status) = failure.status {
                    li { "HTTP status: {status}" }
                }
                for detail in &failure.details {
                    li { "{detail}" }
                }
            }
            if !failure.trace.is_empty() {
                ol { class: "request-error-trace",
                    for line in &failure.trace {
                        li { "{line}" }
                    }
                }
            }
        }
    }
}
