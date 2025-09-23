use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub total_requests: u64,
    pub total_ingestions: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub total_bytes_processed: u64,
    pub total_files_processed: u64,
    pub average_response_time_ms: u64,
    pub errors: u64,
    pub repositories: HashMap<String, RepoMetrics>,
    pub hourly_stats: Vec<HourlyStats>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepoMetrics {
    pub url: String,
    pub request_count: u64,
    pub last_accessed: u64,
    pub size_bytes: u64,
    pub file_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HourlyStats {
    pub hour: u64,
    pub requests: u64,
    pub cache_hits: u64,
    pub bytes: u64,
}

pub struct MetricsCollector {
    metrics: Arc<RwLock<Metrics>>,
    response_times: Arc<RwLock<Vec<Duration>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Metrics::default())),
            response_times: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn record_request(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;
    }

    pub async fn record_ingestion(&self, repo_url: &str, files: usize, bytes: u64) {
        let mut metrics = self.metrics.write().await;
        metrics.total_ingestions += 1;
        metrics.total_files_processed += files as u64;
        metrics.total_bytes_processed += bytes;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // get existing request count before updating
        let existing_count = metrics
            .repositories
            .get(repo_url)
            .map(|r| r.request_count)
            .unwrap_or(0);

        metrics.repositories.insert(
            repo_url.to_string(),
            RepoMetrics {
                url: repo_url.to_string(),
                request_count: existing_count + 1,
                last_accessed: now,
                size_bytes: bytes,
                file_count: files,
            },
        );

        // update hourly stats
        let hour = now / 3600;
        if let Some(stat) = metrics.hourly_stats.iter_mut().find(|s| s.hour == hour) {
            stat.requests += 1;
            stat.bytes += bytes;
        } else {
            metrics.hourly_stats.push(HourlyStats {
                hour,
                requests: 1,
                cache_hits: 0,
                bytes,
            });
        }

        // keep only last 24 hours
        let cutoff = hour.saturating_sub(24);
        metrics.hourly_stats.retain(|s| s.hour > cutoff);
    }

    pub async fn record_cache_hit(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.cache_hits += 1;

        let hour = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 3600;

        if let Some(stat) = metrics.hourly_stats.iter_mut().find(|s| s.hour == hour) {
            stat.cache_hits += 1;
        }
    }

    pub async fn record_cache_miss(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.cache_misses += 1;
    }

    pub async fn record_error(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.errors += 1;
    }

    pub async fn record_response_time(&self, duration: Duration) {
        let mut times = self.response_times.write().await;
        times.push(duration);

        // keep only last 1000 response times
        if times.len() > 1000 {
            let excess = times.len() - 1000;
            times.drain(0..excess);
        }

        // update average
        if !times.is_empty() {
            let avg_ms =
                times.iter().map(|d| d.as_millis() as u64).sum::<u64>() / times.len() as u64;

            let mut metrics = self.metrics.write().await;
            metrics.average_response_time_ms = avg_ms;
        }
    }

    pub async fn get_metrics(&self) -> Metrics {
        self.metrics.read().await.clone()
    }

    pub async fn get_top_repositories(&self, limit: usize) -> Vec<RepoMetrics> {
        let metrics = self.metrics.read().await;
        let mut repos: Vec<_> = metrics.repositories.values().cloned().collect();
        repos.sort_by(|a, b| b.request_count.cmp(&a.request_count));
        repos.truncate(limit);
        repos
    }
}
