//! Shared utility modules
//!
//! This module provides common utilities used across all Bitquery Solana SDKs:
//! - Data compression (LZ4)
//! - Base58 encoding/decoding with LRU caching
//! - Message deduplication with time windows
//! - Circuit breaker pattern
//! - Retry mechanisms with backoff
//! - Metrics recording

/// Data compression utilities using LZ4 algorithm
pub mod compression;
/// Base58 encoding/decoding with LRU caching for performance
pub mod base58_cache;
/// Message deduplication with configurable time windows
pub mod deduplicator;
/// Circuit breaker pattern for fault tolerance
pub mod circuit_breaker;
/// Retry mechanisms with exponential backoff
pub mod retry;
/// Metrics collection and timing utilities
pub mod metrics;

// Re-export commonly used utilities
pub use compression::decompress_lz4;
pub use base58_cache::Base58Cache;
pub use deduplicator::MessageDeduplicator;
pub use circuit_breaker::CircuitBreaker;
pub use retry::RetryStrategy;
pub use metrics::{Timer, record_event_processed, record_batch_processed};
