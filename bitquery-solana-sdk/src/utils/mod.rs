//! Utility modules for the SDK.
//!
//! This module aggregates various helper functionalities used across the SDK,
//! such as data compression and metrics integration.
//! Caching utilities (like Base58Cache) and more complex fault tolerance
//! mechanisms (like dedicated RetryStrategy or CircuitBreaker) from a previous
//! iteration have been simplified or integrated into core components in this version.

// pub mod base58_cache; // Not explicitly used in the new "Enhanced" file listing, TBD if needed
pub mod compression;
pub mod metrics; // This is the new metrics.rs provided by user

// Re-export commonly used utilities for easier access from other SDK modules.
// pub use base58_cache::Base58Cache; // TBD
// Compression is usually called directly: `compression::decompress_lz4()`
// Metrics functions/macros are typically called directly: `metrics::record_event_processed()`, `metrics::Timer`

// Note: The previous version had `base58_cache.rs`, `circuit_breaker.rs`,
// `deduplicator.rs`, `retry.rs`.
// The new "Enhanced" SDK structure provided in the prompt did not list these under `utils/`
// except for `metrics.rs` and `compression.rs`.
// - Deduplication is now a simpler HashSet in `consumer.rs`.
// - Retry logic is simpler (e.g. in Kafka client config or basic loops), or assumed to be handled by rdkafka.
//   The new `Config` has `RetryConfig` but it's not used by a generic `RetryStrategy` util in this version.
// - Circuit Breaker is not present; `ResourceManager` handles backpressure.
// - Base58Cache: If needed, it can be added back. For now, following the new structure.

// If any of the old utilities are found to be implicitly required by the new
// modules (e.g., `config.rs` having a `RetryConfig` might imply a `retry.rs` util),
// they will be added back. For now, sticking to the explicitly listed new structure.
