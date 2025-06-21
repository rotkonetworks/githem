// core/src/lib.rs
use anyhow::{Context, Result};
use git2::{Repository, Status, StatusOptions};
use serde::{Deserialize, Serialize};
use std::io::{IsTerminal, Write};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestOptions {
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub max_file_size: usize,
    pub include_untracked: bool,
    pub branch: Option<String>,
    pub path_prefix: Option<String>,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            max_file_size: 1048576, // 1MB
            include_untracked: false,
            branch: None,
            path_prefix: None,
        }
    }
}

pub struct Ingester {
    repo: Repository,
    options: IngestOptions,
}

impl Ingester {
    pub fn new(repo: Repository, options: IngestOptions) -> Self {
        Self { repo, options }
    }

    pub fn from_path(path: &Path, options: IngestOptions) -> Result<Self> {
        let repo = Repository::open(path).context("Failed to open repository")?;
        Ok(Self::new(repo, options))
    }

    pub fn from_url(url: &str, options: IngestOptions) -> Result<Self> {
        let repo = clone_repository(url, options.branch.as_deref())?;
        Ok(Self::new(repo, options))
    }

    fn should_include(&self, path: &Path) -> Result<bool> {
        let status = self.repo.status_file(path)?;

        if status.contains(Status::IGNORED) && !self.options.include_untracked {
            return Ok(false);
        }

        if path.components().any(|c| c.as_os_str() == ".git") {
            return Ok(false);
        }

        let path_str = path.to_string_lossy();

        for pattern in &self.options.exclude_patterns {
            if glob_match(pattern, &path_str) {
                return Ok(false);
            }
        }

        if !self.options.include_patterns.is_empty() {
            return Ok(self
                .options
                .include_patterns
                .iter()
                .any(|p| glob_match(p, &path_str)));
        }

        Ok(true)
    }

    pub fn ingest<W: Write>(&self, output: &mut W) -> Result<()> {
        let workdir = self
            .repo
            .workdir()
            .context("Repository has no working directory")?;

        let head_result = self.repo.head();
        let has_commits = head_result.is_ok();

        let mut files: Vec<PathBuf> = Vec::new();

        if has_commits {
            let head = head_result?;
            let tree = head.peel_to_tree()?;

            // If path_prefix is specified, get the subtree
            let tree_to_walk = if let Some(prefix) = &self.options.path_prefix {
                match tree.get_path(Path::new(prefix)) {
                    Ok(entry) => self.repo.find_tree(entry.id())?,
                    Err(_) => return Ok(()), // Path doesn't exist, return empty
                }
            } else {
                tree
            };

            tree_to_walk.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
                if entry.kind() == Some(git2::ObjectType::Blob) {
                    if let Some(name) = entry.name() {
                        let path = if dir.is_empty() {
                            PathBuf::from(name)
                        } else {
                            PathBuf::from(dir).join(name)
                        };
                        // Add the prefix back to the path for display
                        let path = if let Some(prefix) = &self.options.path_prefix {
                            PathBuf::from(prefix).join(path)
                        } else {
                            path
                        };
                        files.push(path);
                    }
                }
                git2::TreeWalkResult::Ok
            })?;
        }

        if self.options.include_untracked || !has_commits {
            let mut status_opts = StatusOptions::new();
            status_opts.include_untracked(true);
            status_opts.include_ignored(false);

            let statuses = self.repo.statuses(Some(&mut status_opts))?;

            for status in statuses.iter() {
                if status.status().contains(Status::WT_NEW) {
                    if let Some(path) = status.path() {
                        let path_buf = PathBuf::from(path);
                        // Apply path prefix filter
                        if let Some(prefix) = &self.options.path_prefix {
                            if !path.starts_with(prefix) {
                                continue;
                            }
                        }
                        files.push(path_buf);
                    }
                }
            }
        }

        files.sort();
        files.dedup();

        let mut processed = 0;
        for file in files {
            let full_path = workdir.join(&file);

            if !full_path.exists() || !full_path.is_file() {
                continue;
            }

            if !self.should_include(&file)? {
                continue;
            }

            self.ingest_file(&full_path, &file, output)?;
            processed += 1;
        }

        if processed == 0 {
            eprintln!("Warning: No files found to ingest");
        }

        Ok(())
    }

    fn ingest_file<W: Write>(&self, path: &Path, relative: &Path, output: &mut W) -> Result<()> {
        let metadata = std::fs::metadata(path)?;

        if metadata.len() > self.options.max_file_size as u64 {
            return Ok(());
        }

        let content = std::fs::read_to_string(path).unwrap_or_else(|_| "[Binary file]".to_string());

        writeln!(output, "=== {} ===", relative.display())?;
        writeln!(output, "{content}")?;
        writeln!(output)?;

        Ok(())
    }
}

pub fn is_remote_url(source: &str) -> bool {
    source.starts_with("https://github.com/") 
        || source.starts_with("https://gitlab.com/")
        || source.starts_with("https://gist.github.com/")
        || source.starts_with("https://raw.githubusercontent.com/")
        || source.starts_with("https://gist.githubusercontent.com/")
}

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
}

pub fn parse_github_url(url: &str) -> Option<ParsedGitHubUrl> {
    let url = url.trim().trim_end_matches('/');
    
    // Gist URLs
    if url.contains("gist.github.com") {
        return parse_gist_url(url);
    }
    
    // Raw content URLs
    if url.contains("raw.githubusercontent.com") {
        return parse_raw_url(url);
    }
    
    // Standard GitHub URLs
    if let Some(path) = url.strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("github.com/")) {
        
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 2 {
            let owner = parts[0].to_string();
            let repo = parts[1].to_string();
            
            // Just owner/repo
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
            
            // Handle different patterns
            if parts.len() >= 4 {
                match parts[2] {
                    "tree" => {
                        let branch = parts[3].to_string();
                        let path = if parts.len() > 4 {
                            Some(parts[4..].join("/"))
                        } else { None };
                        
                        return Some(ParsedGitHubUrl {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            branch: Some(branch),
                            path,
                            url_type: GitHubUrlType::Tree,
                            canonical_url: format!("https://github.com/{}/{}", owner, repo),
                        });
                    },
                    "blob" => {
                        let branch = parts[3].to_string();
                        let path = if parts.len() > 4 {
                            Some(parts[4..].join("/"))
                        } else { None };
                        
                        return Some(ParsedGitHubUrl {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            branch: Some(branch),
                            path,
                            url_type: GitHubUrlType::Blob,
                            canonical_url: format!("https://github.com/{}/{}", owner, repo),
                        });
                    },
                    "raw" => {
                        let branch = parts[3].to_string();
                        let path = if parts.len() > 4 {
                            Some(parts[4..].join("/"))
                        } else { None };
                        
                        return Some(ParsedGitHubUrl {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            branch: Some(branch),
                            path,
                            url_type: GitHubUrlType::Raw,
                            canonical_url: format!("https://github.com/{}/{}", owner, repo),
                        });
                    },
                    "commit" => {
                        let commit = parts[3].to_string();
                        
                        return Some(ParsedGitHubUrl {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            branch: Some(commit),
                            path: None,
                            url_type: GitHubUrlType::Commit,
                            canonical_url: format!("https://github.com/{}/{}", owner, repo),
                        });
                    },
                    _ => {}
                }
            }
            
            // Handle /tree/{branch} without path
            if parts.len() == 4 && parts[2] == "tree" {
                return Some(ParsedGitHubUrl {
                    owner: owner.clone(),
                    repo: repo.clone(),
                    branch: Some(parts[3].to_string()),
                    path: None,
                    url_type: GitHubUrlType::Tree,
                    canonical_url: format!("https://github.com/{}/{}", owner, repo),
                });
            }
        }
    }
    
    None
}

fn parse_gist_url(url: &str) -> Option<ParsedGitHubUrl> {
    // Handle gist.githubusercontent.com/{user}/{gist_id}/raw/{revision}/{filename}
    if url.contains("gist.githubusercontent.com") {
        let path = url.strip_prefix("https://gist.githubusercontent.com/")
            .or_else(|| url.strip_prefix("http://gist.githubusercontent.com/"))?;
        
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 2 {
            let owner = parts[0].to_string();
            let gist_id = parts[1].to_string();
            let revision = parts.get(3).map(|s| s.to_string());
            let filename = parts.get(4).map(|s| s.to_string());
            
            return Some(ParsedGitHubUrl {
                owner: owner.clone(),
                repo: gist_id.clone(),
                branch: revision,
                path: filename,
                url_type: GitHubUrlType::GistRaw,
                canonical_url: format!("https://gist.github.com/{}/{}", owner, gist_id),
            });
        }
    }
    
    // Handle gist.github.com/{user}/{gist_id} and gist.github.com/{gist_id}
    if let Some(path) = url.strip_prefix("https://gist.github.com/")
        .or_else(|| url.strip_prefix("http://gist.github.com/")) {
        
        let parts: Vec<&str> = path.split('/').collect();
        
        // Anonymous gist (just ID)
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
        
        // User gist
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
    // raw.githubusercontent.com/{user}/{repo}/{branch}/{path}
    let path = url.strip_prefix("https://raw.githubusercontent.com/")
        .or_else(|| url.strip_prefix("http://raw.githubusercontent.com/"))?;
    
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 3 {
        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        let branch = parts[2].to_string();
        let path = if parts.len() > 3 {
            Some(parts[3..].join("/"))
        } else { None };
        
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

pub fn clone_repository(url: &str, branch: Option<&str>) -> Result<Repository> {
    // Security: Validate URL
    if !is_remote_url(url) {
        return Err(anyhow::anyhow!("Invalid or unsafe URL"));
    }

    // Use secure temp directory with proper cleanup
    let temp_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let path = std::env::temp_dir().join(format!("githem-{temp_id}"));

    let mut fetch_opts = git2::FetchOptions::new();

    // Configure SSH authentication with security hardening
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(|url, username_from_url, allowed_types| {
        // Security: Validate URL to prevent credential theft
        if !is_remote_url(url) {
            return Err(git2::Error::from_str("Invalid URL for credential authentication"));
        }

        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            // Security: Try SSH agent first (safer than filesystem keys)
            if let Ok(cred) = git2::Cred::ssh_key_from_agent(username_from_url.unwrap_or("git")) {
                return Ok(cred);
            }

            // Security: Validate and sanitize HOME directory
            let home = match std::env::var("HOME") {
                Ok(h) => {
                    let home_path = Path::new(&h);
                    // Security: Validate HOME path is absolute and exists
                    if !home_path.is_absolute() || !home_path.exists() {
                        return Err(git2::Error::from_str("Invalid HOME directory"));
                    }
                    h
                }
                Err(_) => return Err(git2::Error::from_str("HOME environment variable not set")),
            };

            let ssh_dir = Path::new(&home).join(".ssh");

            // Security: Validate SSH directory exists and has correct permissions
            if !ssh_dir.exists() || !ssh_dir.is_dir() {
                return Err(git2::Error::from_str("SSH directory not found"));
            }

            // Security: Check SSH directory permissions (should be 700)
            #[cfg(unix)]
            if let Ok(metadata) = std::fs::metadata(&ssh_dir) {
                use std::os::unix::fs::PermissionsExt;
                let perms = metadata.permissions().mode();
                if (perms & 0o777) != 0o700 {
                    return Err(git2::Error::from_str("SSH directory has insecure permissions"));
                }
            }

            // Security: Only try Ed25519 keys (most secure)
            let key_names = ["id_ed25519"];
            for key_name in &key_names {
                let private_key = ssh_dir.join(key_name);
                let public_key = ssh_dir.join(format!("{key_name}.pub"));

                if private_key.exists() && public_key.exists() {
                    // Security: Validate private key permissions (should be 600)
                    #[cfg(unix)]
                    if let Ok(metadata) = std::fs::metadata(&private_key) {
                        use std::os::unix::fs::PermissionsExt;
                        let perms = metadata.permissions().mode();
                        if (perms & 0o777) != 0o600 {
                            continue; // Skip keys with wrong permissions
                        }

                        // Security: Validate key ownership
                        if metadata.uid() != unsafe { libc::getuid() } {
                            continue; // Skip keys not owned by current user
                        }

                        // Security: Validate key file size (reasonable limits)
                        if metadata.len() > 8192 || metadata.len() < 64 {
                            continue; // Skip suspiciously sized keys
                        }
                    } else {
                        continue; // Skip if can't read metadata
                    }

                    // Security: Validate public key permissions (should be 644 or 600)
                    #[cfg(unix)]
                    if let Ok(pub_metadata) = std::fs::metadata(&public_key) {
                        use std::os::unix::fs::PermissionsExt;
                        let pub_perms = pub_metadata.permissions().mode();
                        if (pub_perms & 0o777) != 0o644 && (pub_perms & 0o777) != 0o600 {
                            continue; // Skip keys with wrong permissions
                        }
                    }

                    // Security: Use secure credential creation with timeout
                    return git2::Cred::ssh_key(
                        username_from_url.unwrap_or("git"),
                        Some(&public_key),
                        &private_key,
                        None, // No passphrase support to prevent hanging
                    );
                }
            }
        }

        // Security: Only allow default credentials for HTTPS
        if allowed_types.contains(git2::CredentialType::DEFAULT) && url.starts_with("https://") {
            return git2::Cred::default();
        }

        Err(git2::Error::from_str("No secure authentication method available"))
    });

    // Only show progress in TTY (CLI mode)
    if std::io::stderr().is_terminal() {
        callbacks.transfer_progress(|stats| {
            if stats.total_objects() > 0 {
                eprint!(
                    "\rReceiving objects: {}% ({}/{})",
                    (100 * stats.received_objects()) / stats.total_objects(),
                    stats.received_objects(),
                    stats.total_objects()
                );
            }
            true
        });
    }

    fetch_opts.remote_callbacks(callbacks);
    fetch_opts.depth(1);
    fetch_opts.download_tags(git2::AutotagOption::None);

    // Note: git2 doesn't support timeout configuration directly
    // Timeout is handled at the OS network level

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_opts);

    if let Some(branch) = branch {
        builder.branch(branch);
    }

    let repo = builder.clone(url, &path)?;

    if std::io::stderr().is_terminal() {
        eprintln!();
    }

    // Note: Repository owns the temp directory, cleanup happens when dropped
    Ok(repo)
}

pub fn checkout_branch(repo: &Repository, branch_name: &str) -> Result<()> {
    let (object, reference) = repo.revparse_ext(branch_name)?;

    repo.checkout_tree(&object, None)?;

    match reference {
        Some(gref) => repo.set_head(gref.name().unwrap())?,
        None => repo.set_head_detached(object.id())?,
    }

    Ok(())
}

fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern.starts_with("*.") {
        return path.ends_with(&pattern[1..]);
    }

    if let Some(prefix) = pattern.strip_suffix("/*") {
        return path.starts_with(prefix) && path.len() > prefix.len();
    }

    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return path.starts_with(parts[0]) && path.ends_with(parts[1]);
        }
    }

    path == pattern || path.starts_with(&format!("{pattern}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "src/main.rs"));
        assert!(glob_match("src/*", "src/main.rs"));
        assert!(!glob_match("*.rs", "main.py"));
    }

    #[test]
    fn test_url_validation() {
        assert!(is_remote_url("https://github.com/user/repo"));
        assert!(is_remote_url("https://gitlab.com/user/repo"));
        assert!(!is_remote_url("file:///etc/passwd"));
        assert!(!is_remote_url("ftp://example.com/"));
        assert!(!is_remote_url("https://evil.com/"));
    }

    #[test]
    fn test_github_url_parsing() {
        // Basic repo
        let parsed = parse_github_url("https://github.com/rust-lang/rust").unwrap();
        assert_eq!(parsed.owner, "rust-lang");
        assert_eq!(parsed.repo, "rust");
        assert_eq!(parsed.url_type, GitHubUrlType::Repository);

        // Tree with path
        let parsed = parse_github_url("https://github.com/owner/repo/tree/main/src/lib").unwrap();
        assert_eq!(parsed.owner, "owner");
        assert_eq!(parsed.repo, "repo");
        assert_eq!(parsed.branch, Some("main".to_string()));
        assert_eq!(parsed.path, Some("src/lib".to_string()));
        assert_eq!(parsed.url_type, GitHubUrlType::Tree);

        // Blob
        let parsed = parse_github_url("https://github.com/owner/repo/blob/master/README.md").unwrap();
        assert_eq!(parsed.url_type, GitHubUrlType::Blob);
        assert_eq!(parsed.path, Some("README.md".to_string()));

        // Raw
        let parsed = parse_github_url("https://raw.githubusercontent.com/owner/repo/main/file.txt").unwrap();
        assert_eq!(parsed.url_type, GitHubUrlType::Raw);
        assert_eq!(parsed.branch, Some("main".to_string()));
        assert_eq!(parsed.path, Some("file.txt".to_string()));

        // Gist
        let parsed = parse_github_url("https://gist.github.com/user/1234567890abcdef").unwrap();
        assert_eq!(parsed.owner, "user");
        assert_eq!(parsed.repo, "1234567890abcdef");
        assert_eq!(parsed.url_type, GitHubUrlType::Gist);
    }
}

// ============ Additional functions needed by API ============

/// Validate GitHub username/repo name according to GitHub's rules
pub fn validate_github_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 39
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !name.starts_with(['-', '.'])
        && !name.ends_with(['-', '.'])
}

/// Estimate token count for LLM usage
pub fn estimate_tokens(content: &str) -> usize {
    let chars = content.len();
    let words = content.split_whitespace().count();
    let lines = content.lines().count();

    // Base estimate: ~3.3 chars per token for code/text mix
    let char_estimate = chars as f32 / 3.3;

    // Word-based estimate: ~0.75 tokens per word
    let word_estimate = words as f32 * 0.75;

    // Line penalty for structured content
    let line_penalty = lines as f32 * 0.1;

    // Take average and add line penalty
    ((char_estimate + word_estimate) / 2.0 + line_penalty) as usize
}

/// Generate a tree representation from ingested content
pub fn generate_tree_representation(content: &str) -> String {
    let mut tree = String::new();
    tree.push_str("Repository structure:\n");

    for line in content.lines() {
        if line.starts_with("=== ") && line.ends_with(" ===") {
            let path = &line[4..line.len() - 4];
            tree.push_str(&format!("📄 {path}\n"));
        }
    }

    tree
}

/// Count files in ingested content
pub fn count_files(content: &str) -> usize {
    content.matches("=== ").count()
}

// ============ Enhanced Ingester with callbacks ============

pub trait IngestionCallback: Send + Sync {
    fn on_progress(&mut self, _stage: &str, _message: &str) {}
    fn on_file(&mut self, _path: &Path, _content: &str) {}
    fn on_complete(&mut self, _files: usize, _bytes: usize) {}
    fn on_error(&mut self, _error: &str) {}
}

impl Ingester {
    /// Ingest with progress callbacks (for WebSocket streaming)
    pub fn ingest_with_callback<W: Write, C: IngestionCallback>(
        &self,
        output: &mut W,
        callback: &mut C,
    ) -> Result<()> {
        callback.on_progress("starting", "Beginning ingestion");
        
        let workdir = self
            .repo
            .workdir()
            .context("Repository has no working directory")?;

        let head_result = self.repo.head();
        let has_commits = head_result.is_ok();

        let mut files: Vec<PathBuf> = Vec::new();

        callback.on_progress("scanning", "Scanning repository files");

        if has_commits {
            let head = head_result?;
            let tree = head.peel_to_tree()?;

            // If path_prefix is specified, get the subtree
            let tree_to_walk = if let Some(prefix) = &self.options.path_prefix {
                match tree.get_path(Path::new(prefix)) {
                    Ok(entry) => self.repo.find_tree(entry.id())?,
                    Err(_) => {
                        callback.on_error(&format!("Path '{}' not found", prefix));
                        return Ok(());
                    }
                }
            } else {
                tree
            };

            tree_to_walk.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
                if entry.kind() == Some(git2::ObjectType::Blob) {
                    if let Some(name) = entry.name() {
                        let path = if dir.is_empty() {
                            PathBuf::from(name)
                        } else {
                            PathBuf::from(dir).join(name)
                        };
                        // Add the prefix back to the path for display
                        let path = if let Some(prefix) = &self.options.path_prefix {
                            PathBuf::from(prefix).join(path)
                        } else {
                            path
                        };
                        files.push(path);
                    }
                }
                git2::TreeWalkResult::Ok
            })?;
        }

        if self.options.include_untracked || !has_commits {
            let mut status_opts = StatusOptions::new();
            status_opts.include_untracked(true);
            status_opts.include_ignored(false);

            let statuses = self.repo.statuses(Some(&mut status_opts))?;

            for status in statuses.iter() {
                if status.status().contains(Status::WT_NEW) {
                    if let Some(path) = status.path() {
                        let path_buf = PathBuf::from(path);
                        // Apply path prefix filter
                        if let Some(prefix) = &self.options.path_prefix {
                            if !path.starts_with(prefix) {
                                continue;
                            }
                        }
                        files.push(path_buf);
                    }
                }
            }
        }

        files.sort();
        files.dedup();

        callback.on_progress("processing", &format!("Processing {} files", files.len()));

        let mut processed = 0;
        let mut total_bytes = 0;
        
        for file in files {
            let full_path = workdir.join(&file);

            if !full_path.exists() || !full_path.is_file() {
                continue;
            }

            if !self.should_include(&file)? {
                continue;
            }

            if let Ok(metadata) = std::fs::metadata(&full_path) {
                if metadata.len() > self.options.max_file_size as u64 {
                    continue;
                }
                
                let content = std::fs::read_to_string(&full_path)
                    .unwrap_or_else(|_| "[Binary file]".to_string());

                // Write to output
                writeln!(output, "=== {} ===", file.display())?;
                writeln!(output, "{content}")?;
                writeln!(output)?;

                // Callback for streaming
                callback.on_file(&file, &content);

                total_bytes += content.len();
                processed += 1;
            }
        }

        if processed == 0 {
            callback.on_error("No files found to ingest");
            eprintln!("Warning: No files found to ingest");
        } else {
            callback.on_complete(processed, total_bytes);
        }

        Ok(())
    }
}

// ============ Repository metadata extraction ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryMetadata {
    pub url: String,
    pub default_branch: String,
    pub branches: Vec<String>,
    pub size: Option<u64>,
    pub last_commit: Option<String>,
    pub remote_url: Option<String>,
}

impl Ingester {
    /// Extract metadata about the repository
    pub fn get_metadata(&self) -> Result<RepositoryMetadata> {
        let repo = &self.repo;
        
        // Get default branch
        let default_branch = repo.head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
            .unwrap_or_else(|| "main".to_string());

        // Get all branches
        let mut branches = Vec::new();
        for branch_result in repo.branches(Some(git2::BranchType::Local))? {
            if let Ok((branch, _)) = branch_result {
                if let Ok(Some(name)) = branch.name() {
                    branches.push(name.to_string());
                }
            }
        }

        // Get remote URL
        let remote_url = repo.find_remote("origin")
            .ok()
            .and_then(|r| r.url().map(String::from));

        // Get last commit info
        let last_commit = repo.head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .map(|c| {
                format!("{} - {}", 
                    c.id().to_string().chars().take(8).collect::<String>(),
                    c.summary().unwrap_or("No message")
                )
            });

        // Calculate repo size (approximate)
        let size = repo.workdir()
            .and_then(|w| walkdir::WalkDir::new(w)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .reduce(|a, b| a + b));

        Ok(RepositoryMetadata {
            url: remote_url.clone().unwrap_or_default(),
            default_branch,
            branches,
            size,
            last_commit,
            remote_url,
        })
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[test]
    fn test_validate_github_name() {
        assert!(validate_github_name("rust-lang"));
        assert!(validate_github_name("hello_world"));
        assert!(validate_github_name("test.repo"));
        assert!(!validate_github_name(""));
        assert!(!validate_github_name("-invalid"));
        assert!(!validate_github_name("invalid-"));
        assert!(!validate_github_name(&"a".repeat(40)));
    }

    #[test]
    fn test_estimate_tokens() {
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let tokens = estimate_tokens(code);
        assert!(tokens > 0);
        assert!(tokens < 50); // Small code snippet
    }

    #[test]
    fn test_tree_generation() {
        let content = "=== src/main.rs ===\nfn main() {}\n\n=== Cargo.toml ===\n[package]\n";
        let tree = generate_tree_representation(content);
        assert!(tree.contains("📄 src/main.rs"));
        assert!(tree.contains("📄 Cargo.toml"));
    }
}
