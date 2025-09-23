use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedRepository {
    pub key: String,
    pub url: String,
    pub commit_hash: String,
    pub result: crate::ingestion::IngestionResult,
    pub created_at: u64,  // unix timestamp
    pub last_accessed: u64,  // unix timestamp
    pub access_count: u64,
    pub size_bytes: usize,
}

pub struct RepositoryCache {
    cache: Arc<RwLock<HashMap<String, CachedRepository>>>,
    max_size: usize,
    ttl_seconds: u64,
    metrics: Arc<crate::metrics::MetricsCollector>,
}

impl RepositoryCache {
    pub fn new(max_size: usize, ttl: Duration, metrics: Arc<crate::metrics::MetricsCollector>) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
            ttl_seconds: ttl.as_secs(),
            metrics,
        }
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    pub fn generate_key(url: &str, branch: Option<&str>, preset: Option<&str>) -> String {
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
        format!("{:x}", hasher.finalize())
    }

    pub async fn get(&self, key: &str) -> Option<CachedRepository> {
        let mut cache = self.cache.write().await;
        let now = Self::current_timestamp();
        
        if let Some(entry) = cache.get_mut(key) {
            // check ttl
            if now - entry.created_at > self.ttl_seconds {
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

    pub async fn put(&self, key: String, url: String, commit_hash: String, result: crate::ingestion::IngestionResult) {
        let size_bytes = result.content.len();
        let now = Self::current_timestamp();
        
        let entry = CachedRepository {
            key: key.clone(),
            url,
            commit_hash,
            result,
            created_at: now,
            last_accessed: now,
            access_count: 1,
            size_bytes,
        };
        
        let mut cache = self.cache.write().await;
        
        // enforce size limit with lru eviction
        while self.calculate_size(&cache) + size_bytes > self.max_size && !cache.is_empty() {
            // find least recently used
            let lru_key = cache.values()
                .min_by_key(|e| e.last_accessed)
                .map(|e| e.key.clone());
            
            if let Some(key) = lru_key {
                cache.remove(&key);
            }
        }
        
        cache.insert(key, entry);
    }

    pub async fn _invalidate(&self, key: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(key);
    }

    pub async fn _clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
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
        let cache_hits: u64 = cache.values().map(|e| e.access_count.saturating_sub(1)).sum();
        
        if total_accesses > 0 {
            cache_hits as f64 / total_accesses as f64
        } else {
            0.0
        }
    }

    fn get_top_accessed(&self, cache: &HashMap<String, CachedRepository>, limit: usize) -> Vec<(String, u64)> {
        let mut entries: Vec<_> = cache.values()
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
