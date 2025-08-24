use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedGitHubUrl {
    pub owner: String,
    pub repo: String,
    pub branch: Option<String>,
    pub path: Option<String>,
    pub url_type: GitHubUrlType,
    pub canonical_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GitHubUrlType {
    Repository,
    Tree,
    Blob,
    Raw,
    Commit,
    Gist,
    GistRaw,
    Compare,
}

pub fn parse_github_url(url: &str) -> Option<ParsedGitHubUrl> {
    let url = url.trim().trim_end_matches('/');

    if url.contains("gist.github.com") {
        return parse_gist_url(url);
    }

    if url.contains("raw.githubusercontent.com") {
        return parse_raw_url(url);
    }

    if let Some(path) = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("github.com/"))
    {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 2 {
            let owner = parts[0].to_string();
            let repo = parts[1].to_string();

            if parts.len() == 2 {
                return Some(ParsedGitHubUrl {
                    owner: owner.clone(),
                    repo: repo.clone(),
                    branch: None,
                    path: None,
                    url_type: GitHubUrlType::Repository,
                    canonical_url: format!("https://github.com/{}/{}", owner, repo),
                });
            }

            if parts.len() >= 4 {
                match parts[2] {
                    "tree" | "blob" => {
                        let all_parts = &parts[3..];
                        if all_parts.is_empty() {
                            return None;
                        }
                        
                        let mut branch_end_idx = all_parts.len();
                        for (i, part) in all_parts.iter().enumerate() {
                            if part.contains('.') && !part.ends_with(".git") {
                                branch_end_idx = i;
                                break;
                            }
                            if matches!(*part, "src" | "lib" | "test" | "tests" | "docs" | 
                                               "bin" | "pkg" | "cmd" | "internal" | "api" | 
                                               "web" | "client" | "server" | "assets" | "public") {
                                branch_end_idx = i;
                                break;
                            }
                        }
                        
                        let branch = all_parts[..branch_end_idx].join("/");
                        let path = if branch_end_idx < all_parts.len() {
                            Some(all_parts[branch_end_idx..].join("/"))
                        } else {
                            None
                        };

                        return Some(ParsedGitHubUrl {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            branch: Some(branch),
                            path,
                            url_type: if parts[2] == "tree" { GitHubUrlType::Tree } else { GitHubUrlType::Blob },
                            canonical_url: format!("https://github.com/{}/{}", owner, repo),
                        });
                    }
                    "commit" => {
                        return Some(ParsedGitHubUrl {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            branch: Some(parts[3].to_string()),
                            path: None,
                            url_type: GitHubUrlType::Commit,
                            canonical_url: format!("https://github.com/{}/{}", owner, repo),
                        });
                    }
                    "compare" => {
                        let compare_spec = parts[3..].join("/");
                        return Some(ParsedGitHubUrl {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            branch: Some(compare_spec),
                            path: None,
                            url_type: GitHubUrlType::Compare,
                            canonical_url: format!("https://github.com/{}/{}", owner, repo),
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

fn parse_gist_url(url: &str) -> Option<ParsedGitHubUrl> {
    if let Some(path) = url
        .strip_prefix("https://gist.github.com/")
        .or_else(|| url.strip_prefix("http://gist.github.com/"))
    {
        let parts: Vec<&str> = path.split('/').collect();
        
        if parts.len() == 1 {
            return Some(ParsedGitHubUrl {
                owner: "anonymous".to_string(),
                repo: parts[0].to_string(),
                branch: None,
                path: None,
                url_type: GitHubUrlType::Gist,
                canonical_url: format!("https://gist.github.com/{}", parts[0]),
            });
        }

        if parts.len() >= 2 {
            return Some(ParsedGitHubUrl {
                owner: parts[0].to_string(),
                repo: parts[1].to_string(),
                branch: None,
                path: None,
                url_type: GitHubUrlType::Gist,
                canonical_url: format!("https://gist.github.com/{}/{}", parts[0], parts[1]),
            });
        }
    }

    None
}

fn parse_raw_url(url: &str) -> Option<ParsedGitHubUrl> {
    let path = url
        .strip_prefix("https://raw.githubusercontent.com/")
        .or_else(|| url.strip_prefix("http://raw.githubusercontent.com/"))?;

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 3 {
        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        let branch = parts[2].to_string();
        let path = if parts.len() > 3 {
            Some(parts[3..].join("/"))
        } else {
            None
        };

        return Some(ParsedGitHubUrl {
            owner: owner.clone(),
            repo: repo.clone(),
            branch: Some(branch),
            path,
            url_type: GitHubUrlType::Raw,
            canonical_url: format!("https://github.com/{}/{}", owner, repo),
        });
    }

    None
}

pub fn normalize_source_url(
    source: &str,
    branch: Option<String>,
    path_prefix: Option<String>,
) -> Result<(String, Option<String>, Option<String>), String> {
    if let Some(parsed) = parse_github_url(source) {
        let final_branch = branch.or(parsed.branch);
        let final_path = path_prefix.or(parsed.path);
        return Ok((parsed.canonical_url, final_branch, final_path));
    }
    
    if !source.contains("://") && source.matches('/').count() == 1 {
        let parts: Vec<&str> = source.split('/').collect();
        if parts.len() == 2 && validate_github_name(parts[0]) && validate_github_name(parts[1]) {
            let url = format!("https://github.com/{}/{}", parts[0], parts[1]);
            return Ok((url, branch, path_prefix));
        }
    }
    
    Ok((source.to_string(), branch, path_prefix))
}

pub fn validate_github_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 39
        && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !name.starts_with(['-', '.'])
        && !name.ends_with(['-', '.'])
}
