use dioxus::prelude::*;
use crate::types::*;

#[component]
pub fn ContentView(state: Signal<RepositoryState>) -> Element {
    let content = if let Some(_selected) = &state().selected_file {
        // Get content for selected file
        state().ingestion.as_ref().map(|i| i.content.clone())
    } else if let Some(ingestion) = &state().ingestion {
        Some(ingestion.content.clone())
    } else {
        None
    };
    
    rsx! {
        div {
            class: "h-full overflow-auto bg-white dark:bg-gray-900",
            
            if let Some(content) = content {
                pre {
                    class: "p-4 text-sm font-mono text-gray-800 dark:text-gray-200",
                    code {
                        "{content}"
                    }
                }
            } else {
                div {
                    class: "flex items-center justify-center h-full text-gray-500 dark:text-gray-400",
                    "Select a file to view its content"
                }
            }
        }
    }
}
