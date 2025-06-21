//! Common error types for Bitquery Solana operations

use thiserror::Error;

/// Result type alias for Bitquery Solana operations
pub type Result<T> = std::result::Result<T, Error>;

/// Common error types for Bitquery Solana operations
#[derive(Error, Debug)]
pub enum Error {
    /// Errors during data compression or decompression operations
    #[error("Compression error: {0}")]
    Compression(String),

    /// Errors during Base58 encoding or decoding operations
    #[error("Base58 encoding/decoding error: {0}")]
    Base58(#[from] bs58::decode::Error),

    /// General I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Protocol buffer errors
    #[error("Protobuf error: {0}")]
    Protobuf(#[from] prost::DecodeError),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Network-related errors
    #[error("Network error: {0}")]
    Network(String),

    /// Circuit breaker errors
    #[error("Circuit breaker open: {0}")]
    CircuitBreakerOpen(String),

    /// Retry exhausted errors
    #[error("Retry attempts exhausted: {0}")]
    RetryExhausted(String),

    /// Generic errors
    #[error("Error: {0}")]
    Generic(String),
}
