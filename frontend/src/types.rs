use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppState {
    pub theme: Theme,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Theme {
    Light,
    Dark,
    GitHub,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::GitHub
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IngestionResult {
    pub id: String,
    pub summary: IngestionSummary,
    pub tree: String,
    pub content: String,
    pub metadata: RepositoryMetadata,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IngestionSummary {
    pub repository: String,
    pub branch: String,
    pub subpath: Option<String>,
    pub files_analyzed: usize,
    pub total_size: usize,
    pub estimated_tokens: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RepositoryMetadata {
    pub url: String,
    pub default_branch: String,
    pub branches: Vec<String>,
    pub size: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: Option<usize>,
    pub children: Vec<FileNode>,
    pub content: Option<String>,
    pub is_expanded: bool,
    pub is_included: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RepositoryState {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub subpath: Option<String>,
    pub ingestion: Option<IngestionResult>,
    pub file_tree: Option<FileNode>,
    pub selected_file: Option<String>,
    pub include_patterns: HashSet<String>,
    pub exclude_patterns: HashSet<String>,
    pub search_query: String,
    pub view_mode: ViewMode,
}

#[derive(Clone, Debug, PartialEq, Copy)]
pub enum ViewMode {
    Tree,
    Content,
    Split,
    Raw,
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::Split
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IngestRequest {
    pub url: String,
    pub branch: Option<String>,
    pub subpath: Option<String>,
    #[serde(default)]
    pub include_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,
}

fn default_max_file_size() -> usize {
    10 * 1024 * 1024 // 10MB
}
