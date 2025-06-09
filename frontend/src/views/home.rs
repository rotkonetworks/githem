use dioxus::prelude::*;
use crate::Route;
use dioxus::events::Key;

#[component]
pub fn Home() -> Element {
    let mut url_input = use_signal(String::new);
    let mut quick_options = use_signal(QuickOptions::default);
    let navigator = use_navigator();
    
    let handle_submit = move |_| {
        let url = url_input();
        if !url.is_empty() {
            if let Some((owner, repo)) = parse_github_url(&url) {
                navigator.push(Route::Repository { owner, repo });
            }
        }
    };
    
    rsx! {
        div {
            class: "min-h-screen bg-gray-50 dark:bg-gray-900",
            
            div {
                class: "max-w-6xl mx-auto px-4 py-8",
                
                // Minimal header
                div {
                    class: "text-center mb-8",
                    h1 {
                        class: "text-3xl font-bold text-gray-900 dark:text-white",
                        "Githem"
                    }
                    p {
                        class: "text-sm text-gray-600 dark:text-gray-400 mt-1",
                        "Fast repository ingestion for LLMs"
                    }
                }
                
                // Main input section
                div {
                    class: "bg-white dark:bg-gray-800 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700 p-6 mb-6",
                    
                    form {
                        onsubmit: handle_submit,
                        class: "space-y-4",
                        
                        // URL input with shortcuts
                        div {
                            class: "relative",
                            input {
                                r#type: "text",
                                placeholder: "github.com/owner/repo or just owner/repo",
                                value: "{url_input}",
                                oninput: move |evt| url_input.set(evt.value()),
                                onkeydown: move |evt| {
                                    // Ctrl+Enter for quick ingest
                                    if evt.key() == Key::Enter && evt.modifiers().ctrl() {
                                        let url = url_input();
                                        if !url.is_empty() {
                                            if let Some((owner, repo)) = parse_github_url(&url) {
                                                navigator.push(Route::Repository { owner, repo });
                                            }
                                        }
                                    }
                                },
                                class: "w-full px-4 py-3 text-lg border border-gray-300 dark:border-gray-600 rounded-lg
                                       bg-white dark:bg-gray-700 text-gray-900 dark:text-white
                                       focus:ring-2 focus:ring-blue-500 focus:border-transparent
                                       font-mono",
                                autofocus: true,
                            }
                            
                            // Keyboard hint
                            span {
                                class: "absolute right-3 top-3.5 text-xs text-gray-400",
                                "Ctrl+Enter"
                            }
                        }
                        
                        // Quick options
                        div {
                            class: "grid grid-cols-2 md:grid-cols-4 gap-3",
                            
                            QuickOption {
                                label: "Exclude tests",
                                checked: quick_options().exclude_tests,
                                onchange: move |_| quick_options.write().exclude_tests = !quick_options().exclude_tests,
                                shortcut: "T"
                            }
                            
                            QuickOption {
                                label: "Source only",
                                checked: quick_options().source_only,
                                onchange: move |_| quick_options.write().source_only = !quick_options().source_only,
                                shortcut: "S"
                            }
                            
                            QuickOption {
                                label: "No vendors",
                                checked: quick_options().no_vendors,
                                onchange: move |_| quick_options.write().no_vendors = !quick_options().no_vendors,
                                shortcut: "V"
                            }
                            
                            QuickOption {
                                label: "Compact view",
                                checked: quick_options().compact,
                                onchange: move |_| quick_options.write().compact = !quick_options().compact,
                                shortcut: "C"
                            }
                        }
                        
                        // Action buttons
                        div {
                            class: "flex gap-3",
                            
                            button {
                                r#type: "submit",
                                class: "flex-1 px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700
                                       transition-colors font-medium text-lg",
                                "Ingest Repository"
                            }
                            
                            button {
                                r#type: "button",
                                onclick: move |_| {
                                    // TODO: Open file picker for local repos
                                },
                                class: "px-6 py-3 border border-gray-300 dark:border-gray-600 rounded-lg
                                       hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors",
                                "Local Folder"
                            }
                        }
                    }
                }
                
                // Recent repositories
                RecentRepos {}
                
                // Quick examples in compact grid
                div {
                    class: "mt-8",
                    
                    h2 {
                        class: "text-sm font-semibold text-gray-600 dark:text-gray-400 mb-3",
                        "QUICK TO ANALYZE"
                    }
                    
                    div {
                        class: "grid grid-cols-2 md:grid-cols-4 gap-2",
                        
                        QuickExample { owner: "vuejs", repo: "vue" }
                        QuickExample { owner: "d3", repo: "d3" }
                        QuickExample { owner: "excalidraw", repo: "excalidraw" }
                        QuickExample { owner: "ollama", repo: "ollama" }
                        QuickExample { owner: "mrdoob", repo: "three.js" }
                        QuickExample { owner: "bitcoin", repo: "bitcoin" }
                        QuickExample { owner: "neovim", repo: "neovim" }
                        QuickExample { owner: "denoland", repo: "deno" }
                    }
                }
                
                // Keyboard shortcuts help
                div {
                    class: "fixed bottom-4 right-4 text-xs text-gray-500 dark:text-gray-500",
                    
                    button {
                        onclick: move |_| {
                            // TODO: Show shortcuts modal
                        },
                        class: "hover:text-gray-700 dark:hover:text-gray-300",
                        "⌘ Shortcuts"
                    }
                }
            }
        }
    }
}

#[derive(Clone, Default)]
struct QuickOptions {
    exclude_tests: bool,
    source_only: bool,
    no_vendors: bool,
    compact: bool,
}

#[component]
fn QuickOption(
    label: &'static str,
    checked: bool,
    onchange: EventHandler<Event<FormData>>,
    shortcut: &'static str,
) -> Element {
    rsx! {
        label {
            class: "flex items-center space-x-2 cursor-pointer p-2 rounded hover:bg-gray-50 dark:hover:bg-gray-700",
            
            input {
                r#type: "checkbox",
                checked: checked,
                onchange: move |evt| onchange.call(evt),
                class: "rounded border-gray-300 dark:border-gray-600 text-blue-600
                       focus:ring-blue-500 dark:bg-gray-700",
            }
            
            span {
                class: "text-sm text-gray-700 dark:text-gray-300 select-none",
                "{label}"
            }
            
            span {
                class: "text-xs text-gray-400 ml-auto",
                "Alt+{shortcut}"
            }
        }
    }
}

#[component]
fn QuickExample(owner: &'static str, repo: &'static str) -> Element {
    let navigator = use_navigator();
    
    rsx! {
        button {
            onclick: move |_| {
                navigator.push(Route::Repository {
                    owner: owner.to_string(),
                    repo: repo.to_string()
                });
            },
            class: "text-left px-3 py-2 rounded border border-gray-200 dark:border-gray-700
                   hover:border-blue-500 dark:hover:border-blue-400 transition-colors
                   hover:shadow-sm group",
            
            div {
                class: "text-xs text-gray-500 dark:text-gray-400 group-hover:text-blue-600 dark:group-hover:text-blue-400",
                "{owner}/"
            }
            div {
                class: "text-sm font-medium text-gray-900 dark:text-white",
                "{repo}"
            }
        }
    }
}

#[component]
fn RecentRepos() -> Element {
    // TODO: Load from localStorage
    let recent = vec![
        ("zed-industries", "zed"),
        ("astral-sh", "ruff"),
        ("biomejs", "biome"),
    ];
    
    if recent.is_empty() {
        return rsx! {};
    }
    
    let navigator = use_navigator();
    
    rsx! {
        div {
            class: "mb-6",
            
            h2 {
                class: "text-sm font-semibold text-gray-600 dark:text-gray-400 mb-3",
                "RECENT"
            }
            
            div {
                class: "flex flex-wrap gap-2",
                
                for (owner, repo) in recent {
                    button {
                        onclick: move |_| {
                            navigator.push(Route::Repository {
                                owner: owner.to_string(),
                                repo: repo.to_string()
                            });
                        },
                        class: "inline-flex items-center px-3 py-1.5 text-sm rounded-full
                               bg-gray-100 dark:bg-gray-800 text-gray-700 dark:text-gray-300
                               hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors",
                        
                        span { class: "font-mono", "{owner}/{repo}" }
                        
                        button {
                            onclick: move |evt| {
                                evt.stop_propagation();
                                // TODO: Remove from recent
                            },
                            class: "ml-2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200",
                            "×"
                        }
                    }
                }
                
                button {
                    onclick: move |_| {
                        // TODO: Clear all recent
                    },
                    class: "text-xs text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 ml-2",
                    "Clear all"
                }
            }
        }
    }
}

fn parse_github_url(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    
    // Security: Validate against path traversal
    if url.contains("..") || url.contains("//") || url.contains('\\') {
        return None;
    }
    
    // Direct owner/repo format
    if !url.contains("://") && url.matches('/').count() == 1 {
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return validate_github_parts(parts[0], parts[1]);
        }
    }
    
    // GitHub URL formats
    if let Some(path) = url.strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("github.com/")) {
        
        let path_parts: Vec<&str> = path.split('/').collect();
        if path_parts.len() >= 2 {
            return validate_github_parts(path_parts[0], path_parts[1]);
        }
    }
    
    None
}

fn validate_github_parts(owner: &str, repo: &str) -> Option<(String, String)> {
    // GitHub username/repo naming rules
    let valid_pattern = |s: &str| {
        !s.is_empty() 
        && s.len() <= 100
        && s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !s.starts_with('.')
        && !s.ends_with('.')
    };
    
    if valid_pattern(owner) && valid_pattern(repo) {
        Some((owner.to_string(), repo.to_string()))
    } else {
        None
    }
}
