use dioxus::prelude::*;
use crate::{Route, types::*};

#[component]
pub fn Layout() -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    
    let theme_class = match app_state().theme {
        Theme::Light => "theme-light",
        Theme::Dark => "theme-dark",
        Theme::GitHub => "",
    };
    
    rsx! {
        div {
            class: "min-h-screen bg-white dark:bg-gray-900 {theme_class}",
            
            Header {}
            
            if app_state().loading {
                LoadingOverlay {}
            }
            
            if let Some(error) = &app_state().error {
                ErrorBanner { message: error.clone() }
            }
            
            main {
                class: "flex-1",
                Outlet::<Route> {}
            }
        }
    }
}

#[component]
fn Header() -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let route = use_route::<Route>();
    
    rsx! {
        header {
            class: "bg-gray-900 text-white border-b border-gray-800",
            
            div {
                class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8",
                
                div {
                    class: "flex items-center justify-between h-16",
                    
                    // Logo and breadcrumbs
                    div {
                        class: "flex items-center space-x-4",
                        
                        Link {
                            to: Route::Home {},
                            class: "text-xl font-bold hover:text-gray-300 transition-colors",
                            "Githem"
                        }
                        
                        // Breadcrumbs based on current route
                        match &route {
                            Route::Repository { owner, repo } => rsx! {
                                span { class: "text-gray-400", "/" }
                                Link {
                                    to: Route::Repository { owner: owner.clone(), repo: repo.clone() },
                                    class: "hover:text-gray-300",
                                    "{owner}/{repo}"
                                }
                            },
                            Route::RepositoryBranch { owner, repo, branch } => rsx! {
                                span { class: "text-gray-400", "/" }
                                Link {
                                    to: Route::Repository { owner: owner.clone(), repo: repo.clone() },
                                    class: "hover:text-gray-300",
                                    "{owner}/{repo}"
                                }
                                span { class: "text-gray-400", "/" }
                                span { "{branch}" }
                            },
                            Route::RepositoryPath { owner, repo, branch, path } => rsx! {
                                span { class: "text-gray-400", "/" }
                                Link {
                                    to: Route::Repository { owner: owner.clone(), repo: repo.clone() },
                                    class: "hover:text-gray-300",
                                    "{owner}/{repo}"
                                }
                                span { class: "text-gray-400", "/" }
                                Link {
                                    to: Route::RepositoryBranch { 
                                        owner: owner.clone(), 
                                        repo: repo.clone(), 
                                        branch: branch.clone() 
                                    },
                                    class: "hover:text-gray-300",
                                    "{branch}"
                                }
                                span { class: "text-gray-400", "/" }
                                span { class: "text-sm", "{path}" }
                            },
                            _ => rsx! {}
                        }
                    }
                    
                    // Theme switcher
                    div {
                        class: "flex items-center space-x-4",
                        
                        button {
                            onclick: move |_| {
                                let mut state = app_state();
                                state.theme = match state.theme {
                                    Theme::Light => Theme::Dark,
                                    Theme::Dark => Theme::GitHub,
                                    Theme::GitHub => Theme::Light,
                                };
                                app_state.set(state);
                            },
                            class: "p-2 rounded-lg hover:bg-gray-800 transition-colors",
                            match app_state().theme {
                                Theme::Light => "🌙",
                                Theme::Dark => "☀️",
                                Theme::GitHub => "🎨",
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn LoadingOverlay() -> Element {
    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg p-8 shadow-xl",
                
                div {
                    class: "animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto"
                }
                
                p {
                    class: "mt-4 text-gray-700 dark:text-gray-300",
                    "Loading repository..."
                }
            }
        }
    }
}

#[component]
fn ErrorBanner(message: String) -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    
    rsx! {
        div {
            class: "bg-red-50 border-l-4 border-red-500 p-4",
            
            div {
                class: "flex justify-between items-center",
                
                div {
                    class: "flex items-center",
                    
                    span {
                        class: "text-red-600 mr-2",
                        "❌"
                    }
                    
                    p {
                        class: "text-red-700",
                        "{message}"
                    }
                }
                
                button {
                    onclick: move |_| {
                        let mut state = app_state();
                        state.error = None;
                        app_state.set(state);
                    },
                    class: "text-red-500 hover:text-red-700",
                    "×"
                }
            }
        }
    }
}
