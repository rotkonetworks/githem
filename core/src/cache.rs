use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub repo_url: String,
    pub branch: String,
    pub commit_hash: String,
    pub files: Vec<CachedFile>,
    pub metadata: CacheMetadata,
    pub created_at: u64,
    pub last_accessed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFile {
    pub path: PathBuf,
    pub content: Vec<u8>,
    pub size: u64,
    pub is_binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub total_files: usize,
    pub total_size: u64,
    pub tree_hash: String,
    pub cache_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub total_size: u64,
    pub max_size: u64,
    pub expired_entries: usize,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheIndex {
    pub entries: HashMap<String, CacheEntryInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntryInfo {
    pub key: String,
    pub path: PathBuf,
    pub size: u64,
    pub created_at: u64,
    pub last_accessed: u64,
    pub commit_hash: String,
}

pub struct RepositoryCache {
    cache_dir: PathBuf,
    index: HashMap<String, CacheEntryInfo>,
    max_cache_size: u64,
    max_age_seconds: u64,
}

impl RepositoryCache {
    pub fn new() -> Result<Self> {
        Self::with_config(5 * 1024 * 1024 * 1024, 7 * 24 * 3600)
    }

    pub fn with_config(max_size: u64, max_age_seconds: u64) -> Result<Self> {
        let cache_dir = Self::get_cache_dir()?;
        fs::create_dir_all(&cache_dir)?;

        let index = Self::load_index(&cache_dir).unwrap_or_default();

        Ok(Self {
            cache_dir,
            index,
            max_cache_size: max_size,
            max_age_seconds,
        })
    }

    fn get_cache_dir() -> Result<PathBuf> {
        let cache_dir = if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(xdg_cache).join("githem")
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".cache").join("githem")
        } else {
            PathBuf::from("/tmp/githem-cache")
        };
        Ok(cache_dir)
    }

    pub fn generate_cache_key(url: &str, branch: Option<&str>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        if let Some(branch) = branch {
            hasher.update(b":");
            hasher.update(branch.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    pub fn get(&mut self, key: &str) -> Result<Option<CacheEntry>> {
        if let Some(info) = self.index.get_mut(key) {
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

            if now - info.created_at > self.max_age_seconds {
                self.remove(key)?;
                return Ok(None);
            }

            info.last_accessed = now;

            let cache_path = &info.path;
            if cache_path.exists() {
                let data = fs::read(cache_path)?;
                let entry: CacheEntry = bincode::deserialize(&data)?;
                self.save_index()?;
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    pub fn put(&mut self, key: String, entry: CacheEntry) -> Result<()> {
        let serialized = bincode::serialize(&entry)?;
        let entry_size = serialized.len() as u64;

        self.evict_if_needed(entry_size)?;

        let cache_file = self.cache_dir.join(format!("{}.cache", key));
        fs::write(&cache_file, serialized)?;

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        self.index.insert(
            key.clone(),
            CacheEntryInfo {
                key,
                path: cache_file,
                size: entry_size,
                created_at: now,
                last_accessed: now,
                commit_hash: entry.commit_hash.clone(),
            },
        );

        self.save_index()?;
        Ok(())
    }

    pub fn check_commit(&self, key: &str, current_commit: &str) -> CacheCommitStatus {
        if let Some(info) = self.index.get(key) {
            if info.commit_hash == current_commit {
                CacheCommitStatus::Match
            } else {
                CacheCommitStatus::Outdated
            }
        } else {
            CacheCommitStatus::NotCached
        }
    }

    pub fn remove(&mut self, key: &str) -> Result<()> {
        if let Some(info) = self.index.remove(key) {
            if info.path.exists() {
                fs::remove_file(info.path)?;
            }
            self.save_index()?;
        }
        Ok(())
    }

    fn evict_if_needed(&mut self, new_entry_size: u64) -> Result<()> {
        let total_size: u64 = self.index.values().map(|e| e.size).sum();

        if total_size + new_entry_size <= self.max_cache_size {
            return Ok(());
        }

        let mut entries: Vec<_> = self.index.values().cloned().collect();
        entries.sort_by_key(|e| e.last_accessed);

        let mut freed_space = 0u64;
        for entry in entries {
            if total_size - freed_space + new_entry_size <= self.max_cache_size {
                break;
            }
            freed_space += entry.size;
            self.remove(&entry.key)?;
        }

        Ok(())
    }

    fn load_index(cache_dir: &Path) -> Result<HashMap<String, CacheEntryInfo>> {
        let index_path = cache_dir.join("index.json");
        if index_path.exists() {
            let data = fs::read_to_string(index_path)?;
            let index: CacheIndex = serde_json::from_str(&data)?;
            Ok(index.entries)
        } else {
            Ok(HashMap::new())
        }
    }

    fn save_index(&self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");
        let index = CacheIndex {
            entries: self.index.clone(),
        };
        let data = serde_json::to_string_pretty(&index)?;
        fs::write(index_path, data)?;
        Ok(())
    }

    pub fn clear_all(&mut self) -> Result<()> {
        for key in self.index.keys().cloned().collect::<Vec<_>>() {
            self.remove(&key)?;
        }
        Ok(())
    }

    pub fn get_stats(&self) -> CacheStats {
        let total_size: u64 = self.index.values().map(|e| e.size).sum();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let expired_count = self
            .index
            .values()
            .filter(|e| now - e.created_at > self.max_age_seconds)
            .count();

        CacheStats {
            total_entries: self.index.len(),
            total_size,
            max_size: self.max_cache_size,
            expired_entries: expired_count,
            cache_dir: self.cache_dir.clone(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum CacheCommitStatus {
    Match,
    Outdated,
    NotCached,
}

pub struct CacheManager;

impl CacheManager {
    pub fn clear_cache() -> Result<()> {
        let mut cache = RepositoryCache::new()?;
        cache.clear_all()?;
        Ok(())
    }

    pub fn get_stats() -> Result<CacheStats> {
        let cache = RepositoryCache::new()?;
        Ok(cache.get_stats())
    }
}
