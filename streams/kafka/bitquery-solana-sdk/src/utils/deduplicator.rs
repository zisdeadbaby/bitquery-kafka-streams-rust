use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex; // Using std::sync::Mutex for LruCache
use std::time::{Duration, Instant};
use tracing::trace;

/// A time-based message deduplicator using an LRU cache.
///
/// This utility helps in filtering out duplicate messages (identified by a string key,
/// typically a message signature or unique ID) that are received within a specified
/// time window. It uses an LRU cache to store timestamps of recently seen messages.
pub struct MessageDeduplicator {
    // `Mutex` is suitable here as `LruCache` itself is not async.
    // Operations are expected to be quick.
    cache: Mutex<LruCache<String, Instant>>,
    /// The time window within which a message is considered a duplicate.
    time_window: Duration,
}

impl MessageDeduplicator {
    /// Creates a new `MessageDeduplicator`.
    ///
    /// # Arguments
    /// * `max_size`: The maximum number of unique message signatures to track.
    ///               Older entries are evicted based on LRU policy.
    /// * `time_window`: The `Duration` for which a message signature, once seen,
    ///                  will cause subsequent identical signatures to be marked as duplicates.
    pub fn new(max_size: usize, time_window: Duration) -> Self {
        let nz_max_size = NonZeroUsize::new(max_size)
            .unwrap_or_else(|| NonZeroUsize::new(10000).expect("Default cache size is non-zero"));

        Self {
            cache: Mutex::new(LruCache::new(nz_max_size)),
            time_window,
        }
    }

    /// Checks if a message with the given signature should be processed.
    ///
    /// A message should be processed if its signature has not been seen within
    /// the `time_window`. If it's a new signature or an old one outside the window,
    /// this method records the current time for the signature and returns `true`.
    /// Otherwise, it returns `false`.
    ///
    /// # Arguments
    /// * `signature`: A unique string identifier for the message.
    ///
    /// # Returns
    /// `true` if the message is not a duplicate and should be processed, `false` otherwise.
    pub async fn should_process(&self, signature: &str) -> bool {
        // The `async` keyword is kept for API consistency if internal logic might become async,
        // but current LruCache + Mutex usage is synchronous.
        let mut cache_guard = match self.cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // Handle Mutex poisoning: log and potentially recover or panic.
                // For simplicity, we'll assume recovery by using the poisoned guard.
                tracing::error!("Deduplicator cache mutex poisoned. Attempting to continue.");
                poisoned.into_inner()
            }
        };

        let now = Instant::now();

        // Check if signature exists and is within the time window
        if let Some(seen_at) = cache_guard.get(signature) {
            if now.duration_since(*seen_at) < self.time_window {
                trace!("Duplicate signature '{}' detected within time window.", signature);
                return false; // Duplicate within window
            }
        }

        // Not a duplicate (either new or outside window), so add/update it in cache
        cache_guard.put(signature.to_string(), now);
        trace!("New signature '{}' added to deduplicator cache.", signature);

        // Optional: Periodic cleaning of very old entries if LRU isn't sufficient
        // (e.g., if many unique, old items fill cache before recent duplicates are re-checked)
        // However, LRU itself should handle this by evicting the least recently *accessed*.
        // If an item is accessed (even if old), it becomes MRU.
        // The original `clean_old_entries` logic was a bit redundant with LRU's purpose
        // unless the goal was to strictly enforce time window for *all* entries, not just LRU.
        // For now, relying on LRU's eviction is standard.

        true
    }

    // The `clean_old_entries` method from the original prompt might be useful if
    // the cache can fill up with items older than `time_window` that are never
    // re-accessed, preventing newer items from being cached. However, LRU
    // evicts the least recently *used*. If an old item is used (a `get` call),
    // it becomes the most recently used.
    // A manual periodic sweep is usually for time-based eviction *regardless of access*,
    // which `LruCache` doesn't do directly. `moka::cache::Cache` has time-to-live.
    // For simplicity with `lru` and `Mutex`, we'll omit explicit periodic cleaning
    // unless it proves necessary.
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_deduplicator_new_message() {
        let dedup = MessageDeduplicator::new(10, Duration::from_secs(1));
        assert!(dedup.should_process("sig1").await, "First time should process");
    }

    #[tokio::test]
    async fn test_deduplicator_duplicate_within_window() {
        let dedup = MessageDeduplicator::new(10, Duration::from_secs(1));
        assert!(dedup.should_process("sig1").await, "First time");
        assert!(!dedup.should_process("sig1").await, "Duplicate within window should not process");
    }

    #[tokio::test]
    async fn test_deduplicator_duplicate_outside_window() {
        let window = Duration::from_millis(50);
        let dedup = MessageDeduplicator::new(10, window);

        assert!(dedup.should_process("sig1").await, "First time");
        assert!(!dedup.should_process("sig1").await, "Duplicate immediately after");

        sleep(window + Duration::from_millis(10)).await; // Wait for window to pass

        assert!(dedup.should_process("sig1").await, "Same signature after window should process again");
        assert!(!dedup.should_process("sig1").await, "Duplicate immediately after the second processing");
    }

    #[tokio::test]
    async fn test_deduplicator_lru_eviction() {
        let cache_size = 2;
        let window = Duration::from_secs(10); // Long window, eviction is by size
        let dedup = MessageDeduplicator::new(cache_size, window);

        assert!(dedup.should_process("sig1").await); // sig1 added
        assert!(dedup.should_process("sig2").await); // sig2 added, cache: {sig1, sig2} (order may vary)

        // Access sig1 to make it most recently used (assuming internal get updates LRU)
        // With current implementation, should_process on existing key (if it's a duplicate) returns false early.
        // If it was outside window, it would re-put.
        // Let's test by adding a third distinct signature to force eviction.

        assert!(dedup.should_process("sig3").await); // sig3 added. Either sig1 or sig2 evicted.

        // To check which was evicted, we need to know LRU behavior.
        // LruCache evicts the *least recently used*.
        // Order of operations:
        // 1. put("sig1", t1) -> cache: sig1 (MRU)
        // 2. put("sig2", t2) -> cache: sig1, sig2 (MRU)
        // 3. put("sig3", t3) -> sig1 evicted. cache: sig2, sig3 (MRU)

        // So, "sig1" should have been evicted. If we process "sig1" again, it should be treated as new.
        assert!(dedup.should_process("sig1").await, "sig1 should be processed again after eviction");

        // "sig2" should still be in cache and considered duplicate if accessed now.
        assert!(!dedup.should_process("sig2").await, "sig2 should be a duplicate");
        // "sig3" should also be a duplicate.
        assert!(!dedup.should_process("sig3").await, "sig3 should be a duplicate");
    }

    #[tokio::test]
    async fn test_deduplicator_multiple_signatures() {
        let dedup = MessageDeduplicator::new(10, Duration::from_secs(1));
        assert!(dedup.should_process("sigA").await);
        assert!(dedup.should_process("sigB").await);
        assert!(!dedup.should_process("sigA").await, "sigA is duplicate");
        assert!(!dedup.should_process("sigB").await, "sigB is duplicate");
        assert!(dedup.should_process("sigC").await);
    }
}
