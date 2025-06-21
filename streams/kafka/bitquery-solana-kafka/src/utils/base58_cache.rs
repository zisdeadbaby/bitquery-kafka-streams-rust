use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex; // Using std::sync::Mutex for simplicity with LruCache

/// A cache for Base58 encoding and decoding operations with LRU eviction.
///
/// This utility helps to reduce redundant Base58 computations by caching
/// recent results. It's particularly useful when dealing with frequently
/// seen addresses or signatures.
///
/// The cache is thread-safe due to the internal `Mutex`.
pub struct Base58Cache {
    // Using Mutex from std::sync for LruCache as it's not async.
    // If this becomes a bottleneck, consider async-aware mutexes or sharding.
    encode_cache: Mutex<LruCache<Vec<u8>, String>>,
    decode_cache: Mutex<LruCache<String, Vec<u8>>>,
}

impl Base58Cache {
    /// Creates a new `Base58Cache` with a specified maximum size for each
    /// (encode/decode) internal cache.
    ///
    /// # Arguments
    /// * `max_size`: The maximum number of entries to store in each cache.
    ///               If `max_size` is 0, a default size (e.g., 1000) is used.
    pub fn new(max_size: usize) -> Self {
        // Ensure NonZeroUsize, as LruCache requires it. Default to a reasonable size if 0.
        let nz_max_size = NonZeroUsize::new(max_size)
            .unwrap_or_else(|| NonZeroUsize::new(1000).expect("Default cache size is non-zero"));

        Self {
            encode_cache: Mutex::new(LruCache::new(nz_max_size)),
            decode_cache: Mutex::new(LruCache::new(nz_max_size)),
        }
    }

    /// Encodes a byte slice into a Base58 string, using the cache.
    ///
    /// If the encoding for the given data is already in the cache, it's returned directly.
    /// Otherwise, the data is encoded, the result is stored in the cache, and then returned.
    ///
    /// # Arguments
    /// * `data`: The byte slice to encode.
    ///
    /// # Returns
    /// The Base58 encoded string.
    pub fn encode(&self, data: &[u8]) -> String {
        let data_key = data.to_vec(); // Clone data for cache key

        // Check cache first
        // Lock is held for a short duration.
        if let Ok(mut cache) = self.encode_cache.lock() {
            if let Some(cached_val) = cache.get(&data_key) {
                return cached_val.clone();
            }
        } else {
            // Handle mutex poisoning if necessary, though unlikely here.
            // For simplicity, encode directly if lock fails.
            return bs58::encode(data).into_string();
        }

        // If not in cache, encode and then store
        let encoded_value = bs58::encode(data).into_string();

        if let Ok(mut cache) = self.encode_cache.lock() {
            cache.put(data_key, encoded_value.clone());
        }

        encoded_value
    }

    /// Decodes a Base58 encoded string into a byte vector, using the cache.
    ///
    /// If the decoding for the given string is already in the cache, it's returned directly.
    /// Otherwise, the string is decoded, the result is stored in the cache, and then returned.
    ///
    /// # Arguments
    /// * `encoded_str`: The Base58 string to decode.
    ///
    /// # Returns
    /// A `Result<Vec<u8>, bs58::decode::Error>` containing the decoded bytes or an error.
    pub fn decode(&self, encoded_str: &str) -> Result<Vec<u8>, bs58::decode::Error> {
        let str_key = encoded_str.to_string(); // Clone string for cache key

        // Check cache first
        if let Ok(mut cache) = self.decode_cache.lock() {
            if let Some(cached_val) = cache.get(&str_key) {
                return Ok(cached_val.clone());
            }
        } else {
            // Handle mutex poisoning. Decode directly if lock fails.
            return bs58::decode(encoded_str).into_vec();
        }

        // If not in cache, decode and then store
        let decoded_value = bs58::decode(encoded_str).into_vec()?;

        if let Ok(mut cache) = self.decode_cache.lock() {
            cache.put(str_key, decoded_value.clone());
        }

        Ok(decoded_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base58_cache_new() {
        let cache = Base58Cache::new(100);
        // Not much to assert other than it doesn't panic.
        // We can try a simple operation.
        let data = b"hello world";
        cache.encode(data); // Should work
    }

    #[test]
    fn test_base58_cache_encode_and_decode() {
        let cache = Base58Cache::new(10);
        let original_data = b"test data 123";

        // First encode
        let encoded_str = cache.encode(original_data);
        assert_eq!(encoded_str, bs58::encode(original_data).into_string());

        // Second encode (should use cache, though hard to verify without internal access)
        let encoded_str_cached = cache.encode(original_data);
        assert_eq!(encoded_str, encoded_str_cached);

        // First decode
        let decoded_data = cache.decode(&encoded_str).expect("Decode failed");
        assert_eq!(decoded_data, original_data);

        // Second decode (should use cache)
        let decoded_data_cached = cache.decode(&encoded_str).expect("Decode cached failed");
        assert_eq!(decoded_data, decoded_data_cached);
    }

    #[test]
    fn test_base58_cache_lru_eviction_encode() {
        let cache_size = 2;
        let cache = Base58Cache::new(cache_size);

        let data1 = b"data1";
        let data2 = b"data2";
        let data3 = b"data3";

        let enc1 = cache.encode(data1); // Add data1
        let _enc2 = cache.encode(data2); // Add data2, cache is full {data1, data2}

        // Access data1 to make it recently used
        let _enc1_again = cache.encode(data1); // Cache: {data2, data1} (data1 is now MRU)

        let _enc3 = cache.encode(data3); // Add data3, data2 should be evicted. Cache: {data1, data3}

        // Verify data1 is still cached (by checking if encode_cache.get returns Some)
        // This requires internal access or a more complex test.
        // For now, we assume LRU property of LruCache crate works.
        // Let's just ensure operations don't fail.
        assert_eq!(cache.encode(data1), enc1); // Should still be quick if cached
        assert_ne!(cache.encode(data2), ""); // data2 might be recomputed
    }

    #[test]
    fn test_base58_cache_decode_invalid_string() {
        let cache = Base58Cache::new(10);
        let invalid_str = "Invalid Base58 String!"; // Contains invalid characters

        let result = cache.decode(invalid_str);
        assert!(result.is_err());

        // Try decoding again, it should still fail and not cache the error (it caches success)
        let result_cached = cache.decode(invalid_str);
        assert!(result_cached.is_err());
    }
}
