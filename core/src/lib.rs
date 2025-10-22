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

use anyhow::Result;
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
        || source.starts_with("http://github.com/")
        || source.starts_with("https://gitlab.com/")
        || source.starts_with("http://gitlab.com/")
        || source.starts_with("https://gist.github.com/")
        || source.starts_with("https://raw.githubusercontent.com/")
        || source.starts_with("https://gist.githubusercontent.com/")
}

/// clone a bare repository and fetch only specific refs for comparison
pub fn clone_for_compare(url: &str, base_ref: &str, head_ref: &str) -> Result<Repository> {
    if !is_remote_url(url) {
        return Err(anyhow::anyhow!("Invalid or unsafe URL"));
    }

    let temp_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let path = std::env::temp_dir().join(format!("githem-compare-{temp_id}"));

    // create bare repository (no working tree, minimal disk usage)
    let repo = Repository::init_bare(&path)?;

    let mut remote = repo.remote("origin", url)?;

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

    fetch_opts.remote_callbacks(callbacks);
    fetch_opts.depth(1);
    fetch_opts.download_tags(git2::AutotagOption::None);

    // fetch only the two refs we need for comparison
    let refspecs = vec![
        format!("+refs/heads/{}:refs/remotes/origin/{}", base_ref, base_ref),
        format!("+refs/heads/{}:refs/remotes/origin/{}", head_ref, head_ref),
        format!("+refs/tags/{}:refs/tags/{}", base_ref, base_ref),
        format!("+refs/tags/{}:refs/tags/{}", head_ref, head_ref),
    ];

    // try to fetch, ignoring errors for refs that don't exist
    for refspec in &refspecs {
        let _ = remote.fetch(&[refspec.as_str()], Some(&mut fetch_opts), None);
    }

    drop(remote); // drop remote to release borrow on repo

    Ok(repo)
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

/// detect and compress common license files and headers into a single line
pub fn compress_license(path: &str, content: &str) -> Option<String> {
    let path_lower = path.to_lowercase();
    let content_lower = content.to_lowercase();

    // for dedicated license files
    if path_lower.contains("license") || path_lower.contains("licence")
        || path_lower.contains("copying") {

        // mit license
        if (content_lower.contains("permission is hereby granted, free of charge")
            && content_lower.contains("mit license")) || (content_lower.contains("without restriction")
            && content_lower.contains("above copyright notice")) {
            return Some("[mit license - https://opensource.org/licenses/MIT]".to_string());
        }

        // apache 2.0
        if content_lower.contains("apache license") && content_lower.contains("version 2.0") {
            return Some("[apache license 2.0 - https://www.apache.org/licenses/LICENSE-2.0]".to_string());
        }

        // gpl v3
        if content_lower.contains("gnu general public license") && content_lower.contains("version 3") {
            return Some("[gnu gpl v3 - https://www.gnu.org/licenses/gpl-3.0.html]".to_string());
        }

        // gpl v2
        if content_lower.contains("gnu general public license") && content_lower.contains("version 2") {
            return Some("[gnu gpl v2 - https://www.gnu.org/licenses/gpl-2.0.html]".to_string());
        }

        // bsd 3-clause
        if content_lower.contains("redistribution and use in source and binary forms")
            && content_lower.contains("neither the name of") {
            return Some("[bsd 3-clause license - https://opensource.org/licenses/BSD-3-Clause]".to_string());
        }

        // bsd 2-clause
        if content_lower.contains("redistribution and use in source and binary forms")
            && !content_lower.contains("neither the name of") {
            return Some("[bsd 2-clause license - https://opensource.org/licenses/BSD-2-Clause]".to_string());
        }

        // isc license
        if content_lower.contains("isc license") || (content_lower.contains("permission to use, copy, modify")
            && content_lower.contains("and/or sell copies")) {
            return Some("[isc license - https://opensource.org/licenses/ISC]".to_string());
        }

        // mozilla public license
        if content_lower.contains("mozilla public license") && content_lower.contains("version 2.0") {
            return Some("[mozilla public license 2.0 - https://www.mozilla.org/MPL/2.0/]".to_string());
        }

        // lgpl
        if content_lower.contains("gnu lesser general public license") {
            return Some("[gnu lgpl - https://www.gnu.org/licenses/lgpl.html]".to_string());
        }

        // agpl
        if content_lower.contains("gnu affero general public license") {
            return Some("[gnu agpl - https://www.gnu.org/licenses/agpl.html]".to_string());
        }

        // unlicense
        if content_lower.contains("this is free and unencumbered software released into the public domain") {
            return Some("[unlicense - public domain - https://unlicense.org/]".to_string());
        }

        // creative commons
        if content_lower.contains("creative commons") {
            return Some("[creative commons license - see repository for details]".to_string());
        }
    }

    None
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

/// generate a tree structure from a list of file paths
pub fn generate_tree_from_paths<P: AsRef<Path>>(paths: &[P]) -> String {
    use std::collections::BTreeMap;

    // build directory tree structure
    let mut tree: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for path in paths {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let parts: Vec<&str> = path_str.split('/').collect();

        if parts.len() == 1 {
            // root level file
            tree.entry(".".to_string())
                .or_insert_with(Vec::new)
                .push(path_str.clone());
        } else {
            // file in subdirectory
            let dir = parts[..parts.len() - 1].join("/");
            tree.entry(dir)
                .or_insert_with(Vec::new)
                .push(path_str.clone());
        }
    }

    let mut output = String::new();
    output.push_str("# File Structure\n\n");
    output.push_str(&format!("Total files: {}\n\n", paths.len()));

    // output directories and their files
    for (dir, files) in tree {
        if dir == "." {
            for file in files {
                output.push_str(&format!("  {}\n", file));
            }
        } else {
            output.push_str(&format!("  {}/\n", dir));
            for file in files {
                let filename = file.split('/').last().unwrap_or(&file);
                output.push_str(&format!("    {}\n", filename));
            }
        }
    }

    output.push_str("\n");
    output
}
