//! Bitquery Solana Kafka Streaming SDK
//!
//! High-performance, production-ready Rust SDK for consuming real-time Solana blockchain data
//! from Bitquery's Kafka streams, featuring advanced resource management, batch processing,
//! and event filtering capabilities.

// Strict linting configuration
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![warn(unused_imports)]
#![warn(unused_variables)]
#![warn(dead_code)]

// Conditional compilation for Jemalloc global allocator via "high-performance" feature
#[cfg(feature = "high-performance")]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

// Declare all public and internal modules of the SDK
/// Batch processing functionality for efficient Kafka message handling
pub mod batch_processor;
/// Main client interface for interacting with Bitquery Solana services
pub mod client;
/// Configuration structures for SDK, Kafka, and processing settings
pub mod config; // Contains SdkConfig, KafkaConfig, ProcessingConfig, etc.
/// Kafka consumer implementation with Solana-specific optimizations
pub mod consumer;
/// Error types and error handling utilities
pub mod error;
/// Event definitions and serialization for Solana blockchain data
pub mod events;
/// Message filtering and routing functionality
pub mod filters;
/// Message processors for different Solana data types
pub mod processors;
/// Resource management and connection pooling
pub mod resource_manager;
/// Utility functions and helper modules
pub mod utils;

// Re-export key types and structs for convenient access by SDK users
pub use batch_processor::BatchProcessor;
pub use client::BitqueryClient;
// Re-export all config structs that users might need to interact with.
// InitConfig is defined in this file (lib.rs) as per prompt.
pub use config::{Config, KafkaConfig, ProcessingConfig, ResourceLimits, RetryConfig, SslConfig};
pub use consumer::StreamConsumer;
pub use error::{Error, Result as SdkResult}; // SdkResult alias for crate::error::Result
pub use events::{SolanaEvent, EventType};
pub use filters::{EventFilter, FilterBuilder}; // For creating and applying filters
pub use processors::{DexProcessor, EventProcessor, TransactionProcessor}; // Core processor trait and examples
pub use resource_manager::ResourceManager;
pub use bitquery_solana_core::schemas;

use tracing::info; // Used for logging within initialization functions

/// The current version of the SDK, sourced from the `Cargo.toml` file at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Configuration specific to the SDK's initialization process.
/// This includes settings for logging and metrics exporter setup.
#[derive(Debug, Clone)]
pub struct InitConfig {
    /// Logging filter string, compatible with `tracing_subscriber::EnvFilter`.
    /// Example: "info,my_crate=debug,bitquery_solana_sdk=trace"
    pub log_filter: String,
    /// If `true` and the "metrics" feature is enabled, the Prometheus metrics exporter
    /// will be initialized.
    pub enable_metrics: bool,
    /// The port on which the Prometheus HTTP metrics exporter will listen.
    /// Only used if `enable_metrics` is `true` and "metrics" feature is active.
    pub metrics_port: u16,
}

impl Default for InitConfig {
    /// Provides default settings for SDK initialization.
    /// - `log_filter`: "bitquery_solana_sdk=info"
    /// - `enable_metrics`: True if the "metrics" cargo feature is enabled, false otherwise.
    /// - `metrics_port`: 9090
    fn default() -> Self {
        Self {
            log_filter: "bitquery_solana_sdk=info".to_string(),
            enable_metrics: cfg!(feature = "metrics"), // Default based on compiled features
            metrics_port: 9090, // Common default port for Prometheus exporters
        }
    }
}

/// Initializes the SDK with default `InitConfig` settings.
/// This typically sets up logging and, if the "metrics" feature is enabled,
/// the Prometheus metrics exporter.
///
/// This is a convenience function equivalent to `init_with_config(InitConfig::default())`.
pub async fn init() -> SdkResult<()> {
    init_with_config(InitConfig::default()).await
}

/// Initializes the SDK with custom settings provided via `InitConfig`.
///
/// This function should be called once at the beginning of an application
/// that uses the SDK. It sets up global logging and metrics infrastructure.
///
/// # Arguments
/// * `config`: An `InitConfig` struct specifying how logging and metrics should be set up.
///
/// # Returns
/// An `SdkResult<()>` which is `Ok(())` on successful initialization, or an `Error`
/// if initialization fails (e.g., logger already set, metrics port unavailable).
pub async fn init_with_config(config: InitConfig) -> SdkResult<()> {
    // Initialize logging using `tracing_subscriber`.
    use tracing_subscriber::{fmt, EnvFilter};
    
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .json()
        .try_init() // Use `try_init` to avoid panic if a logger is already globally set (e.g., in tests)
        .map_err(|e| Error::Config(format!("Failed to initialize global tracing subscriber: {}",e)))?;

    // Initialize the Prometheus metrics exporter if the "metrics" feature is enabled
    // AND the `InitConfig` explicitly requests it.
    #[cfg(feature = "metrics")]
    if config.enable_metrics {
        use metrics_exporter_prometheus::PrometheusBuilder;
        use std::net::SocketAddr;
        use std::str::FromStr; // For parsing IP address string

        // Construct the socket address for the metrics HTTP listener.
        let metrics_socket_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", config.metrics_port))
            .map_err(|e| Error::Config(format!("Invalid IP address or port for metrics exporter: {}", e)))?;

        PrometheusBuilder::new()
            .with_http_listener(metrics_socket_addr) // Listen on all network interfaces
            .install()
            .map_err(|e| Error::Config(format!("Failed to install Prometheus metrics exporter: {}", e)))?;
        info!("Prometheus metrics exporter initialized and listening on {}.", metrics_socket_addr);
    } else {
        info!("Metrics collection/exporter is disabled (either by compile-time 'metrics' feature not being active or by InitConfig setting).");
    }

    info!(version = VERSION, log_filter = %config.log_filter, "Bitquery Solana SDK initialized successfully.");
    Ok(())
}
