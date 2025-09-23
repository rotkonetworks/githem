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
                if !p.contains('/') {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|filename| glob_match(p, filename))
                        .unwrap_or(false)
                } else {
                    glob_match(p, &path_str)
                }
            }));
        }

        Ok(true)
    }

    pub fn ingest<W: Write>(&self, output: &mut W) -> Result<()> {
        let files = self.collect_filtered_files()?;
        let workdir = self.repo.workdir().context("Repository has no working directory")?;

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

        let content = std::fs::read_to_string(path).unwrap_or_else(|_| "[Binary file]".to_string());

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
        let workdir = self.repo.workdir().context("Repository has no working directory")?;
        let commit_hash = self.get_current_commit()?;
        let mut files = Vec::new();
        let mut total_size = 0u64;

        let all_files = self.collect_all_repository_files()?;

        for file_path in all_files {
            let full_path = workdir.join(&file_path);

            if !full_path.exists() || !full_path.is_file() {
                continue;
            }

            let metadata = std::fs::metadata(&full_path)?;
            let content = std::fs::read(&full_path)?;
            let is_binary = content.iter().take(8000).any(|&b| b == 0);

            total_size += metadata.len();

            files.push(CachedFile {
                path: file_path,
                content,
                size: metadata.len(),
                is_binary,
            });
        }

        let cache_entry = CacheEntry {
            repo_url: self.repo.path().to_string_lossy().to_string(),
            branch: self.options.branch.clone().unwrap_or_else(|| "HEAD".to_string()),
            commit_hash: commit_hash.clone(),
            files: files.clone(),
            metadata: CacheMetadata {
                total_files: files.len(),
                total_size,
                tree_hash: commit_hash.clone(),
                cache_version: "1.0.0".to_string(),
            },
            created_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            last_accessed: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        };

        if let Some(ref mut cache) = self.cache {
            if let Some(ref cache_key) = self.cache_key {
                cache.put(cache_key.clone(), cache_entry.clone())?;
                eprintln!("✓ Cached {} files ({:.2} MB)",
                files.len(),
                total_size as f64 / 1_048_576.0);
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

        for cached_file in &cache_entry.files {
            if !self.should_include(&cached_file.path)? {
                continue;
            }

            if cached_file.size > self.options.max_file_size as u64 {
                continue;
            }

            let content = if cached_file.is_binary {
                "[Binary file]".to_string()
            } else {
                String::from_utf8_lossy(&cached_file.content).to_string()
            };

            writeln!(output, "=== {} ===", cached_file.path.display())?;
            writeln!(output, "{}", content)?;
            writeln!(output)?;

            processed += 1;
            filtered_size += cached_file.size;
        }

        eprintln!("→ Filtered: {} files ({:.2} MB) from {} total",
        processed,
        filtered_size as f64 / 1_048_576.0,
        cache_entry.metadata.total_files);

        Ok(())
    }

    pub fn get_filter_stats(&self) -> Result<FilterStats> {
        let workdir = self.repo.workdir().context("Repository has no working directory")?;
        let all_files = self.collect_all_repository_files()?;

        let mut stats = FilterStats::default();
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

        let (base_object, _) = repo.revparse_ext(base)?;
        let (head_object, _) = repo.revparse_ext(head)?;

        let base_commit = base_object.peel_to_commit()?;
        let head_commit = head_object.peel_to_commit()?;

        let base_tree = base_commit.tree()?;
        let head_tree = head_commit.tree()?;

        let mut diff_opts = git2::DiffOptions::new();
        let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut diff_opts))?;

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

    pub fn get_metadata(&self) -> Result<RepositoryMetadata> {
        let repo = &self.repo;

        let default_branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
            .unwrap_or_else(|| "main".to_string());

        let mut branches = Vec::new();
        for branch_result in repo.branches(Some(git2::BranchType::Local))? {
            if let Ok((branch, _)) = branch_result {
                if let Ok(Some(name)) = branch.name() {
                    branches.push(name.to_string());
                }
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
