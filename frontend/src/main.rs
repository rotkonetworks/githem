use dioxus::prelude::*;

mod components;
mod views;
mod api;
mod types;

use views::{Repository, RepositoryBranch, RepositoryPath, Home, Layout};

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Layout)]
        #[route("/")]
        Home {},
        
        // GitHub-like routes
        #[route("/:owner/:repo")]
        Repository { owner: String, repo: String },
        
        #[route("/:owner/:repo/tree/:branch")]
        RepositoryBranch { owner: String, repo: String, branch: String },
        
        #[route("/:owner/:repo/tree/:branch/*path")]
        RepositoryPath { owner: String, repo: String, branch: String, path: String },
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // Global app state
    use_context_provider(|| Signal::new(types::AppState::default()));
    
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        
        Router::<Route> {}
    }
}
