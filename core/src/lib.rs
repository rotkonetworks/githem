pub mod cache;
pub mod filtering;
pub mod ingester;
pub mod parser;

pub use cache::{
    CacheCommitStatus, CacheEntry, CacheManager, CacheStats, CachedFile, RepositoryCache,
};
pub use filtering::{get_default_excludes, get_excludes_for_preset, FilterConfig, FilterPreset};
pub use ingester::{FilterStats, IngestOptions, Ingester, IngestionCallback};
pub use parser::{
    normalize_source_url, parse_github_url, validate_github_name, GitHubUrlType, ParsedGitHubUrl,
};

use anyhow::{Context, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryMetadata {
    pub url: String,
    pub default_branch: String,
    pub branches: Vec<String>,
    pub size: Option<u64>,
    pub last_commit: Option<String>,
    pub remote_url: Option<String>,
}

pub fn is_remote_url(source: &str) -> bool {
    source.starts_with("https://github.com/")
        || source.starts_with("https://gitlab.com/")
        || source.starts_with("https://gist.github.com/")
        || source.starts_with("https://raw.githubusercontent.com/")
        || source.starts_with("https://gist.githubusercontent.com/")
}

pub fn clone_repository(url: &str, branch: Option<&str>) -> Result<Repository> {
    if !is_remote_url(url) {
        return Err(anyhow::anyhow!("Invalid or unsafe URL"));
    }

    let temp_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let path = std::env::temp_dir().join(format!("githem-{temp_id}"));

    let mut fetch_opts = git2::FetchOptions::new();
    let mut callbacks = git2::RemoteCallbacks::new();

    callbacks.credentials(|url, username_from_url, allowed_types| {
        if !is_remote_url(url) {
            return Err(git2::Error::from_str(
                "Invalid URL for credential authentication",
            ));
        }

        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            if let Ok(cred) = git2::Cred::ssh_key_from_agent(username_from_url.unwrap_or("git")) {
                return Ok(cred);
            }

            if let Ok(home) = std::env::var("HOME") {
                let ssh_dir = Path::new(&home).join(".ssh");
                if ssh_dir.exists() {
                    let private_key = ssh_dir.join("id_ed25519");
                    let public_key = ssh_dir.join("id_ed25519.pub");

                    if private_key.exists() && public_key.exists() {
                        return git2::Cred::ssh_key(
                            username_from_url.unwrap_or("git"),
                            Some(&public_key),
                            &private_key,
                            None,
                        );
                    }
                }
            }
        }

        if allowed_types.contains(git2::CredentialType::DEFAULT) && url.starts_with("https://") {
            return git2::Cred::default();
        }

        Err(git2::Error::from_str(
            "No secure authentication method available",
        ))
    });

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

pub fn glob_match(pattern: &str, path: &str) -> bool {
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

pub fn estimate_tokens(content: &str) -> usize {
    let chars = content.len();
    let words = content.split_whitespace().count();
    let lines = content.lines().count();
    ((chars as f32 / 3.3 + words as f32 * 0.75) / 2.0 + lines as f32 * 0.1) as usize
}

pub fn count_files(content: &str) -> usize {
    content.matches("=== ").count()
}

pub fn generate_tree(content: &str) -> String {
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
