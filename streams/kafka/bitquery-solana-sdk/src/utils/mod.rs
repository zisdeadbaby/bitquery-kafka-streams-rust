//! Utility modules providing common helper functionalities for the SDK.
//!
//! This module currently includes:
//! - `compression`: For handling data decompression (e.g., LZ4 from Kafka messages).
//! - `metrics`: For recording and potentially exporting operational metrics,
//!              leveraging the `metrics` facade crate.
//!
//! Other utilities from previous iterations (like dedicated Base58 caching,
//! circuit breakers, retry strategies, or complex deduplicators) have either been
//! integrated more directly into relevant core components (e.g., simple deduplication
//! in `consumer.rs`, backpressure in `resource_manager.rs`) or are not explicitly
//! part of the current "Enhanced SDK" specification provided. They can be
//! re-introduced if specific needs arise.

pub mod compression;
pub mod metrics;

// Re-exports for convenience if any utilities were meant for broader SDK internal use.
// For example, if Timer was broadly used:
// pub use metrics::Timer;
// However, current usage seems to be `crate::utils::metrics::Timer` or `sdk_metrics::Timer`.

// Note: The `Config` struct now contains a `RetryConfig`. If a generic retry utility
// (`RetryStrategy` like in previous versions) is intended to use this, `retry.rs`
// would need to be added here. For now, retries might be specific to rdkafka's
// internal mechanisms or handled by simple loops where needed.
