//! LRU cache for Linux AT-SPI accessibility element trees.
//!
//! Avoids re-querying the full accessibility tree on every frame capture
//! by caching element snapshots keyed on (PID, title_hash) with a
//! configurable TTL.

use lru::LruCache;
use oneshim_core::models::focused_element::AccessibilityElement;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

/// A cached snapshot of accessibility elements for a window.
#[derive(Debug, Clone)]
pub struct CachedElements {
    /// The accessibility elements extracted from the window tree.
    pub elements: Vec<AccessibilityElement>,
    /// When this snapshot was cached.
    pub cached_at: Instant,
}

/// LRU cache for AT-SPI accessibility element trees.
///
/// Keyed on `(pid, title_hash)` so that each unique window gets its own
/// cached element tree. Entries expire after the configured TTL.
pub struct AtSpiCache {
    cache: LruCache<(u32, u64), CachedElements>,
    ttl: Duration,
}

impl AtSpiCache {
    /// Create a new cache with the given maximum number of entries and TTL.
    ///
    /// If `max_entries` is 0 it falls back to 200.
    pub fn new(max_entries: usize, ttl_secs: u8) -> Self {
        Self {
            cache: LruCache::new(
                NonZeroUsize::new(max_entries).unwrap_or(NonZeroUsize::new(200).unwrap()),
            ),
            ttl: Duration::from_secs(ttl_secs as u64),
        }
    }

    /// Look up a cached element tree.
    ///
    /// Returns `None` if the entry does not exist or has expired.
    /// Expired entries are evicted on access.
    pub fn get(&mut self, pid: u32, title_hash: u64) -> Option<&CachedElements> {
        // Check expiry before promoting in the LRU list
        if self
            .cache
            .peek(&(pid, title_hash))
            .map_or(false, |e| e.cached_at.elapsed() > self.ttl)
        {
            self.cache.pop(&(pid, title_hash));
            return None;
        }
        self.cache.get(&(pid, title_hash))
    }

    /// Insert or replace a cached element tree.
    pub fn insert(&mut self, pid: u32, title_hash: u64, elements: Vec<AccessibilityElement>) {
        self.cache.put(
            (pid, title_hash),
            CachedElements {
                elements,
                cached_at: Instant::now(),
            },
        );
    }

    /// Remove a specific entry from the cache.
    pub fn invalidate(&mut self, pid: u32, title_hash: u64) {
        self.cache.pop(&(pid, title_hash));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_basic_lifecycle() {
        let mut cache = AtSpiCache::new(10, 60);
        cache.insert(1234, 5678, vec![]);
        assert!(cache.get(1234, 5678).is_some());
        cache.invalidate(1234, 5678);
        assert!(cache.get(1234, 5678).is_none());
    }

    #[test]
    fn cache_ttl_expiry() {
        let mut cache = AtSpiCache::new(10, 0); // 0s TTL — expires immediately
        cache.insert(1, 1, vec![]);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(cache.get(1, 1).is_none());
    }

    #[test]
    fn cache_eviction_at_capacity() {
        let mut cache = AtSpiCache::new(2, 60);
        cache.insert(1, 1, vec![]);
        cache.insert(2, 2, vec![]);
        cache.insert(3, 3, vec![]);
        assert!(cache.get(1, 1).is_none()); // evicted (LRU)
        assert!(cache.get(3, 3).is_some());
    }

    #[test]
    fn cache_stores_elements() {
        let mut cache = AtSpiCache::new(10, 60);
        let elements = vec![AccessibilityElement {
            role: "AXButton".to_string(),
            label: "OK".to_string(),
            bounds: None,
        }];
        cache.insert(42, 99, elements);

        let cached = cache.get(42, 99).expect("should be cached");
        assert_eq!(cached.elements.len(), 1);
        assert_eq!(cached.elements[0].role, "AXButton");
        assert_eq!(cached.elements[0].label, "OK");
    }

    #[test]
    fn cache_zero_max_entries_uses_fallback() {
        // max_entries=0 should not panic; falls back to 200
        let mut cache = AtSpiCache::new(0, 60);
        cache.insert(1, 1, vec![]);
        assert!(cache.get(1, 1).is_some());
    }
}
