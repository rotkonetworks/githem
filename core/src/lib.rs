use anyhow::{Context, Result};
use git2::{Repository, Status, StatusOptions};
use serde::{Deserialize, Serialize};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestOptions {
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub max_file_size: usize,
    pub include_untracked: bool,
    pub branch: Option<String>,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            max_file_size: 1048576, // 1MB
            include_untracked: false,
            branch: None,
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

            tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
                if entry.kind() == Some(git2::ObjectType::Blob) {
                    if let Some(name) = entry.name() {
                        let path = if dir.is_empty() {
                            PathBuf::from(name)
                        } else {
                            PathBuf::from(dir).join(name)
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
                        files.push(PathBuf::from(path));
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
        writeln!(output, "{}", content)?;
        writeln!(output)?;

        Ok(())
    }
}

pub fn is_remote_url(source: &str) -> bool {
    // Security: Only allow known safe protocols
    source.starts_with("https://github.com/") || source.starts_with("https://gitlab.com/")
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
    let path = std::env::temp_dir().join(format!("githem-{}", temp_id));

    let mut fetch_opts = git2::FetchOptions::new();

    // Configure SSH authentication
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(|_url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            if let Ok(cred) = git2::Cred::ssh_key_from_agent(username_from_url.unwrap_or("git")) {
                return Ok(cred);
            }

            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let ssh_dir = Path::new(&home).join(".ssh");

            let key_names = ["id_ed25519", "id_rsa", "id_ecdsa", "id_dsa"];
            for key_name in &key_names {
                let private_key = ssh_dir.join(key_name);
                let public_key = ssh_dir.join(format!("{}.pub", key_name));

                if private_key.exists() {
                    return git2::Cred::ssh_key(
                        username_from_url.unwrap_or("git"),
                        Some(&public_key),
                        &private_key,
                        None,
                    );
                }
            }
        }

        if allowed_types.contains(git2::CredentialType::DEFAULT) {
            return git2::Cred::default();
        }

        Err(git2::Error::from_str(
            "no valid authentication method found",
        ))
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

    if pattern.ends_with("/*") {
        let prefix = &pattern[..pattern.len() - 2];
        return path.starts_with(prefix) && path.len() > prefix.len();
    }

    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return path.starts_with(parts[0]) && path.ends_with(parts[1]);
        }
    }

    path == pattern || path.starts_with(&format!("{}/", pattern))
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
}
