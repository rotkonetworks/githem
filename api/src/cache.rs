use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

// cache timing constants
const CACHE_FRESH_SECS: u64 = 300;      // 5 min - serve immediately without validation
#[allow(dead_code)]
const CACHE_VALIDATE_SECS: u64 = 86400; // 24h - validate commit hash before serving
const CACHE_EXPIRE_SECS: u64 = 604800;  // 7 days - hard expiry

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedRepository {
    pub key: String,
    pub url: String,
    pub branch: Option<String>,
    pub commit_hash: String,
    pub result: crate::ingestion::IngestionResult,
    pub created_at: u64,       // unix timestamp
    pub last_accessed: u64,    // unix timestamp
    pub last_validated: u64,   // last time we checked commit hash
    pub access_count: u64,
    pub size_bytes: usize,
}

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum CacheStatus {
    Fresh,              // < 5 min old, serve immediately
    Valid,              // validated commit hash matches
    Stale,              // commit hash changed, needs refresh
    Expired,            // > 7 days old
    Miss,               // not in cache
}

pub struct RepositoryCache {
    cache: Arc<RwLock<HashMap<String, CachedRepository>>>,
    max_size: usize,
    metrics: Arc<crate::metrics::MetricsCollector>,
}

impl RepositoryCache {
    pub fn new(
        max_size: usize,
        _ttl: Duration, // kept for API compat but we use constants now
        metrics: Arc<crate::metrics::MetricsCollector>,
    ) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
            metrics,
        }
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    pub fn generate_key(
        url: &str,
        branch: Option<&str>,
        preset: Option<&str>,
        path: Option<&str>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        if let Some(branch) = branch {
            hasher.update(b":");
            hasher.update(branch.as_bytes());
        }
        if let Some(preset) = preset {
            hasher.update(b":");
            hasher.update(preset.as_bytes());
        }
        if let Some(path) = path {
            hasher.update(b":");
            hasher.update(path.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    /// check cache status without returning content
    pub async fn check_status(&self, key: &str) -> (CacheStatus, Option<String>) {
        let cache = self.cache.read().await;
        let now = Self::current_timestamp();

        if let Some(entry) = cache.get(key) {
            let age = now - entry.created_at;
            let since_validation = now - entry.last_validated;

            if age > CACHE_EXPIRE_SECS {
                return (CacheStatus::Expired, None);
            }

            if since_validation < CACHE_FRESH_SECS {
                return (CacheStatus::Fresh, Some(entry.commit_hash.clone()));
            }

            // needs validation - return cached commit hash for comparison
            (CacheStatus::Valid, Some(entry.commit_hash.clone()))
        } else {
            (CacheStatus::Miss, None)
        }
    }

    /// get cached entry if fresh or validated
    pub async fn get(&self, key: &str) -> Option<CachedRepository> {
        let mut cache = self.cache.write().await;
        let now = Self::current_timestamp();

        if let Some(entry) = cache.get_mut(key) {
            let age = now - entry.created_at;

            // hard expiry
            if age > CACHE_EXPIRE_SECS {
                cache.remove(key);
                self.metrics.record_cache_miss().await;
                return None;
            }

            entry.last_accessed = now;
            entry.access_count += 1;
            self.metrics.record_cache_hit().await;

            Some(entry.clone())
        } else {
            self.metrics.record_cache_miss().await;
            None
        }
    }

    /// mark entry as validated (commit hash confirmed current)
    pub async fn mark_validated(&self, key: &str) {
        let mut cache = self.cache.write().await;
        let now = Self::current_timestamp();

        if let Some(entry) = cache.get_mut(key) {
            entry.last_validated = now;
        }
    }

    /// invalidate entry (commit hash changed)
    pub async fn invalidate(&self, key: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(key);
    }

    pub async fn put(
        &self,
        key: String,
        url: String,
        branch: Option<String>,
        commit_hash: String,
        result: crate::ingestion::IngestionResult,
    ) {
        let size_bytes = result.content.len();
        let now = Self::current_timestamp();

        let entry = CachedRepository {
            key: key.clone(),
            url,
            branch,
            commit_hash,
            result,
            created_at: now,
            last_accessed: now,
            last_validated: now,
            access_count: 1,
            size_bytes,
        };

        let mut cache = self.cache.write().await;

        // enforce size limit with lru eviction
        while self.calculate_size(&cache) + size_bytes > self.max_size && !cache.is_empty() {
            // find least recently used
            let lru_key = cache
                .values()
                .min_by_key(|e| e.last_accessed)
                .map(|e| e.key.clone());

            if let Some(key) = lru_key {
                cache.remove(&key);
            }
        }

        cache.insert(key, entry);
    }

    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;

        CacheStats {
            entries: cache.len(),
            total_size: self.calculate_size(&cache),
            max_size: self.max_size,
            hit_rate: self.calculate_hit_rate(&cache),
            top_accessed: self.get_top_accessed(&cache, 10),
        }
    }

    fn calculate_size(&self, cache: &HashMap<String, CachedRepository>) -> usize {
        cache.values().map(|e| e.size_bytes).sum()
    }

    fn calculate_hit_rate(&self, cache: &HashMap<String, CachedRepository>) -> f64 {
        let total_accesses: u64 = cache.values().map(|e| e.access_count).sum();
        let cache_hits: u64 = cache
            .values()
            .map(|e| e.access_count.saturating_sub(1))
            .sum();

        if total_accesses > 0 {
            cache_hits as f64 / total_accesses as f64
        } else {
            0.0
        }
    }

    fn get_top_accessed(
        &self,
        cache: &HashMap<String, CachedRepository>,
        limit: usize,
    ) -> Vec<(String, u64)> {
        let mut entries: Vec<_> = cache
            .values()
            .map(|e| (e.url.clone(), e.access_count))
            .collect();

        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries.truncate(limit);
        entries
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheStats {
    pub entries: usize,
    pub total_size: usize,
    pub max_size: usize,
    pub hit_rate: f64,
    pub top_accessed: Vec<(String, u64)>,
}

/// Simple cache for diff results (commits, PRs, compares)
/// Diffs are immutable so we can cache them longer
pub struct DiffCache {
    cache: Arc<RwLock<HashMap<String, CachedDiff>>>,
    max_entries: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct CachedDiff {
    pub content: String,
    pub created_at: u64,
    pub access_count: u64,
}

impl DiffCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
        }
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    pub fn generate_key(diff_type: &str, owner: &str, repo: &str, identifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(diff_type.as_bytes());
        hasher.update(b":");
        hasher.update(owner.as_bytes());
        hasher.update(b"/");
        hasher.update(repo.as_bytes());
        hasher.update(b":");
        hasher.update(identifier.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get_mut(key) {
            entry.access_count += 1;
            Some(entry.content.clone())
        } else {
            None
        }
    }

    pub async fn put(&self, key: String, content: String) {
        let mut cache = self.cache.write().await;

        // evict least accessed if at capacity
        while cache.len() >= self.max_entries && !cache.is_empty() {
            let lru_key = cache
                .iter()
                .min_by_key(|(_, e)| e.access_count)
                .map(|(k, _)| k.clone());

            if let Some(k) = lru_key {
                cache.remove(&k);
            }
        }

        cache.insert(
            key,
            CachedDiff {
                content,
                created_at: Self::current_timestamp(),
                access_count: 1,
            },
        );
    }

    pub async fn stats(&self) -> DiffCacheStats {
        let cache = self.cache.read().await;
        DiffCacheStats {
            entries: cache.len(),
            max_entries: self.max_entries,
            total_size: cache.values().map(|e| e.content.len()).sum(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiffCacheStats {
    pub entries: usize,
    pub max_entries: usize,
    pub total_size: usize,
}
