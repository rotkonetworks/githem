// src/components/raw_view.rs
use dioxus::prelude::*;
use crate::types::*;

#[component]
pub fn RawView(state: Signal<RepositoryState>) -> Element {
    let json_content = state().ingestion.as_ref()
        .and_then(|i| serde_json::to_string_pretty(i).ok())
        .unwrap_or_default();
    
    rsx! {
        div {
            class: "h-full overflow-auto bg-gray-900",
            
            pre {
                class: "p-4 text-sm font-mono text-green-400",
                code {
                    "{json_content}"
                }
            }
        }
    }
}
