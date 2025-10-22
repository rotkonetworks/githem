use crate::{cache::*, clone_repository, glob_match, RepositoryMetadata};
use anyhow::{Context, Result};
use git2::{Repository, Status, StatusOptions};
use serde::{Deserialize, Serialize};
use std::io::Write;
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
    pub filter_preset: Option<crate::FilterPreset>,
    pub apply_default_filters: bool,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            max_file_size: 1048576,
            include_untracked: false,
            branch: None,
            path_prefix: None,
            filter_preset: None,
            apply_default_filters: true,
        }
    }
}

impl IngestOptions {
    pub fn with_preset(preset: crate::FilterPreset) -> Self {
        Self {
            filter_preset: Some(preset),
            apply_default_filters: false,
            ..Default::default()
        }
    }

    pub fn get_effective_excludes(&self) -> Vec<String> {
        let mut excludes = self.exclude_patterns.clone();

        if let Some(preset) = self.filter_preset {
            excludes.extend(crate::get_excludes_for_preset(preset));
        } else if self.apply_default_filters {
            excludes.extend(crate::get_default_excludes());
        }

        excludes.sort();
        excludes.dedup();
        excludes
    }
}

pub struct Ingester {
    repo: Repository,
    pub options: IngestOptions,
    effective_excludes: Vec<String>,
    pub cache: Option<RepositoryCache>,
    pub cache_key: Option<String>,
}

impl Ingester {
    pub fn new(repo: Repository, options: IngestOptions) -> Self {
        let effective_excludes = options.get_effective_excludes();
        Self {
            repo,
            options,
            effective_excludes,
            cache: None,
            cache_key: None,
        }
    }

    pub fn from_path(path: &Path, options: IngestOptions) -> Result<Self> {
        let repo = Repository::open(path).context("Failed to open repository")?;
        Ok(Self::new(repo, options))
    }

    pub fn from_url(url: &str, options: IngestOptions) -> Result<Self> {
        let repo = clone_repository(url, options.branch.as_deref())?;
        Ok(Self::new(repo, options))
    }

    pub fn from_url_cached(url: &str, options: IngestOptions) -> Result<Self> {
        let repo = clone_repository(url, options.branch.as_deref())?;
        let mut ingester = Self::new(repo, options.clone());

        ingester.cache = RepositoryCache::new().ok();
        ingester.cache_key = Some(RepositoryCache::generate_cache_key(
            url,
            options.branch.as_deref(),
        ));

        Ok(ingester)
    }

    pub fn get_filter_preset(&self) -> Option<crate::FilterPreset> {
        self.options.filter_preset
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

        for pattern in &self.effective_excludes {
            if glob_match(pattern, &path_str) {
                return Ok(false);
            }
        }

        if !self.options.include_patterns.is_empty() {
            return Ok(self.options.include_patterns.iter().any(|p| {
                // Handle directory patterns (ending with /)
                if p.ends_with("/") {
                    let dir_prefix = &p[..p.len() - 1];
                    path_str.starts_with(dir_prefix) && path_str.len() > dir_prefix.len()
                } else if !p.contains('/') {
                    // Pattern without path separator - match filename only
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|filename| glob_match(p, filename))
                        .unwrap_or(false)
                } else {
                    // Pattern with path separator - match full path
                    glob_match(p, &path_str)
                }
            }));
        }

        Ok(true)
    }

    pub fn ingest<W: Write>(&self, output: &mut W) -> Result<()> {
        let files = self.collect_filtered_files()?;
        let workdir = self
            .repo
            .workdir()
            .context("Repository has no working directory")?;

        // write file tree structure at the start
        let tree_structure = crate::generate_tree_from_paths(&files);
        write!(output, "{}", tree_structure)?;

        let mut processed = 0;
        for file in files {
            let full_path = workdir.join(&file);
            if full_path.exists() && full_path.is_file() {
                self.ingest_file(&full_path, &file, output)?;
                processed += 1;
            }
        }

        if processed == 0 {
            eprintln!("Warning: No files found to ingest");
        }

        Ok(())
    }

    pub fn ingest_cached<W: Write>(&mut self, output: &mut W) -> Result<()> {
        let commit_hash = self.get_current_commit()?;

        if let Some(ref mut cache) = self.cache {
            if let Some(ref cache_key) = self.cache_key {
                match cache.check_commit(cache_key, &commit_hash) {
                    CacheCommitStatus::Match => {
                        if let Ok(Some(cache_entry)) = cache.get(cache_key) {
                            eprintln!("✓ Using cache (commit: {})", &commit_hash[..8]);
                            return self.filter_cached_files(cache_entry, output);
                        }
                    }
                    CacheCommitStatus::Outdated => {
                        eprintln!("↻ Cache outdated, fetching new data...");
                        let _ = cache.remove(cache_key);
                    }
                    CacheCommitStatus::NotCached => {
                        eprintln!("→ No cache found, fetching repository...");
                    }
                }
            }
        }

        let cache_entry = self.fetch_and_cache()?;
        self.filter_cached_files(cache_entry, output)
    }

    fn ingest_file<W: Write>(&self, path: &Path, relative: &Path, output: &mut W) -> Result<()> {
        let metadata = std::fs::metadata(path)?;

        if metadata.len() > self.options.max_file_size as u64 {
            return Ok(());
        }

        let mut content = std::fs::read_to_string(path).unwrap_or_else(|_| "[binary file]".to_string());

        // compress license files to save tokens
        let path_str = relative.to_string_lossy();
        if let Some(compressed) = crate::compress_license(&path_str, &content) {
            content = compressed;
        }

        writeln!(output, "=== {} ===", relative.display())?;
        writeln!(output, "{content}")?;
        writeln!(output)?;

        Ok(())
    }

    fn collect_filtered_files(&self) -> Result<Vec<PathBuf>> {
        let head_result = self.repo.head();
        let has_commits = head_result.is_ok();

        let mut files: Vec<PathBuf> = Vec::new();

        if has_commits {
            let head = head_result?;
            let tree = head.peel_to_tree()?;

            // when path_prefix is set, walk from that subtree
            // otherwise walk from root
            let (tree_to_walk, is_subtree) = if let Some(prefix) = &self.options.path_prefix {
                match tree.get_path(Path::new(prefix)) {
                    Ok(entry) => (self.repo.find_tree(entry.id())?, true),
                    Err(_) => return Ok(Vec::new()),
                }
            } else {
                (tree, false)
            };

            tree_to_walk.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
                if entry.kind() == Some(git2::ObjectType::Blob) {
                    if let Some(name) = entry.name() {
                        let path = if dir.is_empty() {
                            PathBuf::from(name)
                        } else {
                            PathBuf::from(dir).join(name)
                        };

                        // when walking a subtree, paths are relative to that subtree
                        // prepend the prefix to get the full repository path
                        let full_path = if is_subtree {
                            if let Some(prefix) = &self.options.path_prefix {
                                PathBuf::from(prefix).join(path)
                            } else {
                                path
                            }
                        } else {
                            path
                        };

                        if self.should_include(&full_path).unwrap_or(false) {
                            files.push(full_path);
                        }
                    }
                }
                git2::TreeWalkResult::Ok
            })?;
        }

        // handle untracked files
        if self.options.include_untracked || !has_commits {
            let mut status_opts = StatusOptions::new();
            status_opts.include_untracked(true);
            status_opts.include_ignored(false);

            let statuses = self.repo.statuses(Some(&mut status_opts))?;

            for status in statuses.iter() {
                if status.status().contains(Status::WT_NEW) {
                    if let Some(path) = status.path() {
                        let path_buf = PathBuf::from(path);
                        if let Some(prefix) = &self.options.path_prefix {
                            if !path.starts_with(prefix) {
                                continue;
                            }
                        }
                        if self.should_include(&path_buf).unwrap_or(false) {
                            files.push(path_buf);
                        }
                    }
                }
            }
        }

        files.sort();
        files.dedup();
        Ok(files)
    }

    fn get_current_commit(&self) -> Result<String> {
        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;
        Ok(commit.id().to_string())
    }

    fn fetch_and_cache(&mut self) -> Result<CacheEntry> {
        let workdir = self
            .repo
            .workdir()
            .context("Repository has no working directory")?
            .to_path_buf();
        let commit_hash = self.get_current_commit()?;
        let mut files = Vec::new();
        let mut total_size = 0u64;

        let all_files = self.collect_all_repository_files()?;

        eprintln!("→ Indexing {} files...", all_files.len());

        // Only store METADATA, never file contents
        for file_path in all_files {
            let full_path = workdir.join(&file_path);

            if !full_path.exists() || !full_path.is_file() {
                continue;
            }

            let metadata = std::fs::metadata(&full_path)?;
            total_size += metadata.len();

            // Quick check for binary files without loading entire file
            let is_binary = {
                use std::io::Read;
                let mut file = std::fs::File::open(&full_path)?;
                let mut buf = vec![0u8; 8192.min(metadata.len() as usize)];
                let n = file.read(&mut buf)?;
                buf[..n].contains(&0)
            };

            // Store only metadata - file content stays on disk
            files.push(CachedFile {
                path: file_path,
                size: metadata.len(),
                is_binary,
            });
        }

        let total_files = files.len();

        let cache_entry = CacheEntry {
            repo_url: self.repo.path().to_string_lossy().to_string(),
            branch: self
                .options
                .branch
                .clone()
                .unwrap_or_else(|| "HEAD".to_string()),
            commit_hash: commit_hash.clone(),
            files,
            metadata: CacheMetadata {
                total_files,
                total_size,
                tree_hash: commit_hash.clone(),
                cache_version: "2.0.0".to_string(), // Bumped version for streaming cache
            },
            created_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            last_accessed: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            repo_path: workdir,
        };

        if let Some(ref mut cache) = self.cache {
            if let Some(ref cache_key) = self.cache_key {
                cache.put(cache_key.clone(), cache_entry.clone())?;
                eprintln!(
                    "✓ Indexed {} files ({:.2} MB) - contents remain on disk",
                    cache_entry.files.len(),
                    total_size as f64 / 1_048_576.0
                );
            }
        }

        Ok(cache_entry)
    }

    fn collect_all_repository_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        let head = self.repo.head()?;
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

        Ok(files)
    }

    fn filter_cached_files<W: Write>(&self, cache_entry: CacheEntry, output: &mut W) -> Result<()> {
        let mut processed = 0;
        let mut filtered_size = 0u64;

        // first pass: collect files that pass filtering for tree structure
        let mut filtered_files = Vec::new();

        for cached_file in &cache_entry.files {
            // Apply path_prefix filter first if set
            if let Some(ref prefix) = self.options.path_prefix {
                let path_str = cached_file.path.to_string_lossy();
                // Ensure we are checking directory boundaries properly
                let prefix_with_slash = if prefix.ends_with("/") {
                    prefix.to_string()
                } else {
                    format!("{}/", prefix)
                };
                if !path_str.starts_with(&prefix_with_slash) {
                    continue;
                }
            }

            if !self.should_include(&cached_file.path)? {
                continue;
            }

            if cached_file.size > self.options.max_file_size as u64 {
                continue;
            }

            filtered_files.push(cached_file);
        }

        // write file tree structure at the start
        let paths: Vec<_> = filtered_files.iter().map(|f| &f.path).collect();
        let tree_structure = crate::generate_tree_from_paths(&paths);
        write!(output, "{}", tree_structure)?;

        // second pass: write file contents
        for cached_file in filtered_files {
            // Stream file content from disk - NEVER load into RAM
            let full_path = cache_entry.repo_path.join(&cached_file.path);
            let mut content = if cached_file.is_binary {
                "[binary file]".to_string()
            } else {
                std::fs::read_to_string(&full_path)
                    .unwrap_or_else(|_| "[error reading file]".to_string())
            };

            // compress license files to save tokens
            let path_str = cached_file.path.to_string_lossy();
            if let Some(compressed) = crate::compress_license(&path_str, &content) {
                content = compressed;
            }

            writeln!(output, "=== {} ===", cached_file.path.display())?;
            writeln!(output, "{}", content)?;
            writeln!(output)?;

            processed += 1;
            filtered_size += cached_file.size;
        }

        eprintln!(
            "→ Filtered: {} files ({:.2} MB) from {} total",
            processed,
            filtered_size as f64 / 1_048_576.0,
            cache_entry.metadata.total_files
        );

        Ok(())
    }

    pub fn get_filter_stats(&self) -> Result<FilterStats> {
        let workdir = self
            .repo
            .workdir()
            .context("Repository has no working directory")?;
        let all_files = self.collect_all_repository_files()?;

        let mut stats = FilterStats {
            total_files: all_files.len(),
            ..Default::default()
        };
        stats.total_files = all_files.len();

        for file in all_files {
            let full_path = workdir.join(&file);

            if let Ok(metadata) = std::fs::metadata(&full_path) {
                stats.total_size += metadata.len();

                if self.should_include(&file)? {
                    stats.included_files += 1;
                    stats.included_size += metadata.len();
                } else {
                    stats.excluded_files += 1;
                    stats.excluded_size += metadata.len();
                }
            }
        }

        Ok(stats)
    }

    pub fn generate_diff(&self, base: &str, head: &str) -> Result<String> {
        let repo = &self.repo;

        // Try to resolve references (branches, tags, or commit hashes)
        // refs should already be fetched by clone_for_compare
        let resolve_ref = |ref_name: &str| -> Result<git2::Object> {
            repo.revparse_ext(ref_name)
                .or_else(|_| repo.revparse_ext(&format!("origin/{}", ref_name)))
                .or_else(|_| repo.revparse_ext(&format!("refs/tags/{}", ref_name)))
                .map(|(obj, _)| obj)
                .with_context(|| format!("Failed to resolve reference: {}", ref_name))
        };

        let base_object = resolve_ref(base)?;
        let head_object = resolve_ref(head)?;

        let base_commit = base_object.peel_to_commit()?;
        let head_commit = head_object.peel_to_commit()?;

        let base_tree = base_commit.tree()?;
        let head_tree = head_commit.tree()?;

        let mut diff_opts = git2::DiffOptions::new();
        let diff =
            repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut diff_opts))?;

        let mut output = String::new();
        output.push_str(&format!("# Comparing {} to {}\n\n", base, head));

        let stats = diff.stats()?;
        output.push_str(&format!("Files changed: {}\n", stats.files_changed()));
        output.push_str(&format!("Insertions: {}\n", stats.insertions()));
        output.push_str(&format!("Deletions: {}\n\n", stats.deletions()));

        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let content = std::str::from_utf8(line.content()).unwrap_or("[binary]");
            output.push_str(content);
            true
        })?;

        Ok(output)
    }

    pub fn generate_pr_diff(&self, pr_number: u32) -> Result<String> {
        let repo = &self.repo;

        // Fetch the PR ref and common base branches from GitHub
        let mut remote = repo.find_remote("origin")
            .context("Failed to find origin remote")?;

        let pr_ref = format!("refs/pull/{}/head", pr_number);

        eprintln!("→ Fetching PR #{} and base branches from GitHub...", pr_number);

        // Fetch PR ref
        let pr_refspec = format!("+{}:{}", pr_ref, pr_ref);
        remote.fetch(&[&pr_refspec], None, None)
            .context("Failed to fetch PR ref from GitHub")?;

        // Fetch common base branches (ignore errors if they don't exist)
        for branch in &["main", "master", "develop"] {
            let branch_refspec = format!("+refs/heads/{}:refs/remotes/origin/{}", branch, branch);
            let _ = remote.fetch(&[&branch_refspec], None, None);
        }

        // Get the PR head commit
        let pr_ref_obj = repo.find_reference(&pr_ref)
            .context("Failed to find PR ref after fetch")?;
        let pr_commit = pr_ref_obj.peel_to_commit()
            .context("Failed to peel PR ref to commit")?;

        // Find a base branch and use merge base if available, otherwise use branch HEAD
        let base_branches = ["main", "master", "develop"];
        let mut base_info: Option<(String, git2::Commit)> = None;

        for base_name in &base_branches {
            let origin_ref = format!("origin/{}", base_name);

            if let Ok((obj, _)) = repo.revparse_ext(&origin_ref) {
                if let Ok(branch_commit) = obj.peel_to_commit() {
                    eprintln!("→ Found base branch {} at {}", base_name, branch_commit.id());

                    // Try to find merge base, fall back to branch HEAD
                    let base_commit = if let Ok(merge_base_oid) = repo.merge_base(branch_commit.id(), pr_commit.id()) {
                        if let Ok(merge_base_commit) = repo.find_commit(merge_base_oid) {
                            eprintln!("→ Using merge base {}", merge_base_oid);
                            merge_base_commit
                        } else {
                            eprintln!("→ Using {} HEAD (no merge base)", base_name);
                            branch_commit
                        }
                    } else {
                        eprintln!("→ Using {} HEAD (no common history)", base_name);
                        branch_commit
                    };

                    base_info = Some((base_name.to_string(), base_commit));
                    break;
                }
            }
        }

        let (base_name, base_commit) = base_info
            .context("Could not find any base branch (main/master/develop)")?;

        let base_tree = base_commit.tree()?;
        let pr_tree = pr_commit.tree()?;

        let mut diff_opts = git2::DiffOptions::new();
        let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&pr_tree), Some(&mut diff_opts))?;

        let mut output = String::new();
        output.push_str(&format!("# Pull Request #{}\n\n", pr_number));
        output.push_str(&format!("Base: {} ({})\n", base_name, base_commit.id()));
        output.push_str(&format!("Head: PR #{} ({})\n\n", pr_number, pr_commit.id()));

        let stats = diff.stats()?;
        output.push_str(&format!("Files changed: {}\n", stats.files_changed()));
        output.push_str(&format!("Insertions: {}\n", stats.insertions()));
        output.push_str(&format!("Deletions: {}\n\n", stats.deletions()));

        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let content = std::str::from_utf8(line.content()).unwrap_or("[binary]");
            output.push_str(content);
            true
        })?;

        Ok(output)
    }

    pub fn get_metadata(&self) -> Result<RepositoryMetadata> {
        let repo = &self.repo;

        let default_branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
            .unwrap_or_else(|| "main".to_string());

        let mut branches = Vec::new();
        for (branch, _) in (repo.branches(Some(git2::BranchType::Local))?).flatten() {
            if let Ok(Some(name)) = branch.name() {
                branches.push(name.to_string());
            }
        }

        let remote_url = repo
            .find_remote("origin")
            .ok()
            .and_then(|r| r.url().map(String::from));

        let last_commit = repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .map(|c| {
                format!(
                    "{} - {}",
                    c.id().to_string().chars().take(8).collect::<String>(),
                    c.summary().unwrap_or("No message")
                )
            });

        let size = repo.workdir().and_then(|w| {
            walkdir::WalkDir::new(w)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .reduce(|a, b| a + b)
        });

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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FilterStats {
    pub total_files: usize,
    pub included_files: usize,
    pub excluded_files: usize,
    pub total_size: u64,
    pub included_size: u64,
    pub excluded_size: u64,
    pub excluded_by_filter: usize,
}

impl FilterStats {
    pub fn inclusion_rate(&self) -> f64 {
        if self.total_files == 0 {
            0.0
        } else {
            self.included_files as f64 / self.total_files as f64
        }
    }

    pub fn size_reduction(&self) -> f64 {
        if self.total_size == 0 {
            0.0
        } else {
            self.excluded_size as f64 / self.total_size as f64
        }
    }
}

pub trait IngestionCallback: Send + Sync {
    fn on_progress(&mut self, _stage: &str, _message: &str) {}
    fn on_file(&mut self, _path: &Path, _content: &str) {}
    fn on_complete(&mut self, _files: usize, _bytes: usize) {}
    fn on_error(&mut self, _error: &str) {}
}
