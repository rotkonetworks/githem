use dioxus::prelude::*;
use crate::types::*;
use crate::components::{format_size, format_tokens};

#[component]
pub fn ControlPanel(state: Signal<RepositoryState>) -> Element {
    let mut include_input = use_signal(String::new);
    let mut exclude_input = use_signal(String::new);
    
    rsx! {
        div {
            class: "bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 p-4",
            
            // First row: Branch, view mode, search
            div {
                class: "flex items-center gap-4 mb-4",
                
                // Branch selector
                if let Some(ingestion) = &state().ingestion {
                    select {
                        value: "{state().branch}",
                        onchange: move |evt| {
                            state.write().branch = evt.value();
                            // TODO: Reload with new branch
                        },
                        class: "px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg
                               bg-white dark:bg-gray-700 text-gray-900 dark:text-white",
                        
                        for branch in &ingestion.metadata.branches {
                            option {
                                value: "{branch}",
                                selected: branch == &state().branch,
                                "{branch}"
                            }
                        }
                    }
                }
                
                // View mode selector
                div {
                    class: "flex bg-white dark:bg-gray-700 rounded-lg border border-gray-300 dark:border-gray-600",
                    
                    ViewModeButton { mode: ViewMode::Tree, current: state().view_mode.clone(), state: state.clone() }
                    ViewModeButton { mode: ViewMode::Content, current: state().view_mode.clone(), state: state.clone() }
                    ViewModeButton { mode: ViewMode::Split, current: state().view_mode.clone(), state: state.clone() }
                    ViewModeButton { mode: ViewMode::Raw, current: state().view_mode.clone(), state: state.clone() }
                }
                
                // Search
                input {
                    r#type: "text",
                    placeholder: "Search files...",
                    value: "{state().search_query}",
                    oninput: move |evt| state.write().search_query = evt.value(),
                    class: "flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg
                           bg-white dark:bg-gray-700 text-gray-900 dark:text-white
                           placeholder-gray-500 dark:placeholder-gray-400",
                }
            }
            
            // Second row: Filters
            div {
                class: "flex items-center gap-4 mb-4",
                
                // Include patterns
                div {
                    class: "flex items-center gap-2",
                    
                    label {
                        class: "text-sm text-gray-600 dark:text-gray-400",
                        "Include:"
                    }
                    
                    input {
                        r#type: "text",
                        placeholder: "*.rs, *.toml",
                        value: "{include_input}",
                        oninput: move |evt| include_input.set(evt.value()),
                        class: "px-2 py-1 text-sm border border-gray-300 dark:border-gray-600 rounded
                               bg-white dark:bg-gray-700 text-gray-900 dark:text-white",
                    }
                    
                    button {
                        onclick: move |_| {
                            let patterns = include_input()
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            state.write().include_patterns = patterns;
                            // TODO: Apply filters
                        },
                        class: "px-3 py-1 text-sm bg-blue-600 text-white rounded hover:bg-blue-700",
                        "Apply"
                    }
                }
                
                // Exclude patterns
                div {
                    class: "flex items-center gap-2",
                    
                    label {
                        class: "text-sm text-gray-600 dark:text-gray-400",
                        "Exclude:"
                    }
                    
                    input {
                        r#type: "text",
                        placeholder: "tests/*, *.lock",
                        value: "{exclude_input}",
                        oninput: move |evt| exclude_input.set(evt.value()),
                        class: "px-2 py-1 text-sm border border-gray-300 dark:border-gray-600 rounded
                               bg-white dark:bg-gray-700 text-gray-900 dark:text-white",
                    }
                    
                    button {
                        onclick: move |_| {
                            let patterns = exclude_input()
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            state.write().exclude_patterns = patterns;
                            // TODO: Apply filters
                        },
                        class: "px-3 py-1 text-sm bg-blue-600 text-white rounded hover:bg-blue-700",
                        "Apply"
                    }
                }
            }
            
            // Third row: Stats and actions
            if let Some(ingestion) = &state().ingestion {
                div {
                    class: "flex items-center justify-between",
                    
                    // Stats
                    div {
                        class: "flex items-center gap-6 text-sm text-gray-600 dark:text-gray-400",
                        
                        span {
                            "📁 {ingestion.summary.files_analyzed} files"
                        }
                        
                        span {
                            "💾 {format_size(ingestion.summary.total_size)}"
                        }
                        
                        span {
                            "🔤 ~{format_tokens(ingestion.summary.estimated_tokens)} tokens"
                        }
                    }
                    
                    // Actions
                    div {
                        class: "flex items-center gap-2",
                        
                        button {
                            onclick: move |_| {
                                // TODO: Download content
                            },
                            class: "px-4 py-2 text-sm bg-gray-200 dark:bg-gray-700 rounded-lg
                                   hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors",
                            "📥 Download"
                        }
                        
                        button {
                            onclick: move |_| {
                                // TODO: Copy to clipboard
                            },
                            class: "px-4 py-2 text-sm bg-gray-200 dark:bg-gray-700 rounded-lg
                                   hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors",
                            "📋 Copy"
                        }
                        
                        a {
                            href: "/api/result/{ingestion.id}",
                            target: "_blank",
                            class: "px-4 py-2 text-sm bg-gray-200 dark:bg-gray-700 rounded-lg
                                   hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors",
                            "🔗 API"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ViewModeButton(mode: ViewMode, current: ViewMode, state: Signal<RepositoryState>) -> Element {
    let label = match mode {
        ViewMode::Tree => "Tree",
        ViewMode::Content => "Content",
        ViewMode::Split => "Split",
        ViewMode::Raw => "Raw",
    };

    let is_active = mode == current;

    rsx! {
        button {
            onclick: move |_| state.write().view_mode = mode.clone(),
            class: if is_active {
                "px-3 py-1.5 text-sm font-medium transition-colors bg-blue-600 text-white"
            } else {
                "px-3 py-1.5 text-sm font-medium transition-colors text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-600"
            },
            "{label}"
        }
    }
}
