use dioxus::prelude::*;
use crate::{api, types::*, components::*};

#[component]
pub fn Repository(owner: String, repo: String) -> Element {
    let state = use_signal(|| RepositoryState {
        owner: owner.clone(),
        repo: repo.clone(),
        branch: String::new(),
        subpath: None,
        ingestion: None,
        file_tree: None,
        selected_file: None,
        include_patterns: Default::default(),
        exclude_patterns: Default::default(),
        search_query: String::new(),
        view_mode: ViewMode::Split,
    });
    
    let app_state = use_context::<Signal<AppState>>();
    
    // Load repository on mount
    use_effect(move || {
        to_owned![state, app_state];
        spawn(async move {
            // Set loading
            app_state.write().loading = true;
            
            // Create ingestion request
            let request = IngestRequest {
                url: format!("https://github.com/{}/{}", state().owner, state().repo),
                branch: None,
                subpath: None,
                include_patterns: vec![],
                exclude_patterns: vec![],
                max_file_size: 10 * 1024 * 1024,
            };
            
            match api::ingest_repository(request).await {
                Ok(ingestion) => {
                    let file_tree = api::parse_file_tree(&ingestion.tree);
                    state.write().ingestion = Some(ingestion.clone());
                    state.write().file_tree = file_tree;
                    state.write().branch = ingestion.summary.branch.clone();
                }
                Err(e) => {
                    app_state.write().error = Some(e);
                }
            }
            
            app_state.write().loading = false;
        });
    });
    
    rsx! {
        div {
            class: "h-screen flex flex-col",
            
            ControlPanel { state: state }
            
            div {
                class: "flex-1 overflow-hidden",
                
                match state().view_mode {
                    ViewMode::Tree => rsx! {
                        FileTreeView { state: state }
                    },
                    ViewMode::Content => rsx! {
                        ContentView { state: state }
                    },
                    ViewMode::Split => rsx! {
                        div {
                            class: "grid grid-cols-3 h-full",
                            
                            div {
                                class: "col-span-1 border-r border-gray-200 dark:border-gray-700",
                                FileTreeView { state: state }
                            }
                            
                            div {
                                class: "col-span-2",
                                ContentView { state: state }
                            }
                        }
                    },
                    ViewMode::Raw => rsx! {
                        RawView { state: state }
                    },
                }
            }
        }
    }
}

#[component]
pub fn RepositoryBranch(owner: String, repo: String, branch: String) -> Element {
    // Similar to Repository but with branch pre-selected
    rsx! {
        Repository { owner: owner, repo: repo }
    }
}

#[component]
pub fn RepositoryPath(owner: String, repo: String, branch: String, path: String) -> Element {
    // Similar to Repository but with path pre-selected
    rsx! {
        Repository { owner: owner, repo: repo }
    }
}
