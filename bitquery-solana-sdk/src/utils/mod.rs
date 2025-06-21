//! Utility modules for the SDK.
//!
//! This module re-exports utilities from the shared core library
//! and provides SDK-specific utilities.

// Re-export utilities from shared core
pub use bitquery_solana_core::utils::*;

// SDK-specific utilities (if any)
pub mod metrics;
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
