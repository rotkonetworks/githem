use crate::types::*;
use gloo_net::http::Request;

const API_BASE: &str = "/api";

pub async fn ingest_repository(request: IngestRequest) -> Result<IngestionResult, String> {
    let response = Request::post(&format!("{}/ingest", API_BASE))
        .json(&request)
        .map_err(|e| format!("Failed to create request: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;
    
    if !response.ok() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("API error: {}", error_text));
    }
    
    let value = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    
    // Get the ID from the response
    let id = value["id"].as_str()
        .ok_or_else(|| "Missing ID in response".to_string())?;
    
    // Fetch the full result
    get_ingestion_result(id).await
}

pub async fn get_ingestion_result(id: &str) -> Result<IngestionResult, String> {
    let response = Request::get(&format!("{}/result/{}", API_BASE, id))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch result: {}", e))?;
    
    if !response.ok() {
        return Err("Failed to get ingestion result".to_string());
    }
    
    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse result: {}", e))
}

pub async fn get_repository_metadata(owner: &str, repo: &str) -> Result<RepositoryMetadata, String> {
    let response = Request::get(&format!("{}/metadata/{}/{}", API_BASE, owner, repo))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch metadata: {}", e))?;
    
    if !response.ok() {
        return Err("Failed to get repository metadata".to_string());
    }
    
    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse metadata: {}", e))
}

pub async fn get_branches(owner: &str, repo: &str) -> Result<Vec<String>, String> {
    let response = Request::get(&format!("{}/branches/{}/{}", API_BASE, owner, repo))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch branches: {}", e))?;
    
    if !response.ok() {
        return Err("Failed to get branches".to_string());
    }
    
    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse branches: {}", e))
}

pub async fn download_content(id: &str) -> Result<String, String> {
    let response = Request::get(&format!("{}/download/{}", API_BASE, id))
        .send()
        .await
        .map_err(|e| format!("Failed to download: {}", e))?;
    
    if !response.ok() {
        return Err("Failed to download content".to_string());
    }
    
    response
        .text()
        .await
        .map_err(|e| format!("Failed to read content: {}", e))
}

// Parse file tree from the ingestion result
pub fn parse_file_tree(tree_text: &str) -> Option<FileNode> {
    // This is a simplified parser - you'd want to make this more robust
    let lines: Vec<&str> = tree_text.lines().collect();
    
    // Find the root directory line
    let root_line = lines.iter()
        .find(|line| line.contains("└── ") && line.ends_with("/"))?;
    
    let root_name = root_line
        .split("└── ")
        .last()?
        .trim_end_matches('/');
    
    Some(FileNode {
        name: root_name.to_string(),
        path: "/".to_string(),
        is_directory: true,
        size: None,
        children: vec![], // Would need to parse children recursively
        content: None,
        is_expanded: true,
        is_included: true,
    })
}
