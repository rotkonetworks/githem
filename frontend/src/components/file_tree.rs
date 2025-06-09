// src/components/file_tree.rs
use dioxus::prelude::*;
use crate::types::*;

#[component]
pub fn FileTreeView(state: Signal<RepositoryState>) -> Element {
    rsx! {
        div {
            class: "h-full overflow-y-auto bg-white dark:bg-gray-900 p-4",
            
            if let Some(tree) = &state().file_tree {
                FileTreeNode { 
                    node: tree.clone(), 
                    state: state,
                    depth: 0 
                }
            } else {
                div {
                    class: "text-gray-500 dark:text-gray-400 text-center py-8",
                    "Loading file tree..."
                }
            }
        }
    }
}

#[component]
fn FileTreeNode(
    node: FileNode,
    state: Signal<RepositoryState>,
    depth: usize,
) -> Element {
    // Implementation similar to the original Dioxus code
    rsx! {
        div {
            class: "select-none",
            style: "padding-left: {depth * 20}px",
            
            div {
                class: "flex items-center py-1 px-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded cursor-pointer",
                onclick: move |_| {
                    if !node.is_directory {
                        state.write().selected_file = Some(node.path.clone());
                    }
                },
                
                if node.is_directory {
                    span { class: "mr-1", "📁" }
                } else {
                    span { class: "mr-1", "📄" }
                }
                
                span {
                    class: "text-sm",
                    "{node.name}"
                }
            }
        }
    }
}
