//! Bitquery Solana Core Library
//!
//! Shared core functionality for all Bitquery Solana SDKs including:
//! - Common data compression utilities
//! - Base58 encoding/decoding with caching
//! - Message deduplication
//! - Circuit breaker patterns
//! - Retry mechanisms
//! - Shared protobuf schemas
//! - Common error types
//! - Metrics utilities

#![warn(missing_docs)]

// Conditional compilation for high-performance features
#[cfg(feature = "high-performance")]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

pub mod error;
pub mod utils;
pub mod schemas;

// Re-export commonly used types
pub use error::{Error, Result};

/// Core library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
