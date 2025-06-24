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

// Strict linting configuration
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![warn(unused_imports)]
#![warn(unused_variables)]
#![warn(dead_code)]

// Note: high-performance feature with jemalloc is now handled in main lib.rs

pub mod error;
pub mod utils;
pub mod schemas;

// Re-export commonly used types
pub use error::{Error, Result};

/// Core library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
