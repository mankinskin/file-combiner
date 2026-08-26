use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

/// Placeholder shell. The upload/download UI lands in a later package.
#[component]
fn App() -> Element {
    rsx! {
        main { "file-combiner" }
    }
}
