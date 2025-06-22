use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid; // For generating unique group_id for Kafka if not specified by user
use crate::error::Result;

// Note: `InitConfig` is now defined in `lib.rs` as it's specific to SDK initialization.

/// `Config` is the main configuration structure for the Bitquery Solana SDK.
/// It aggregates configurations for various components like Kafka, event processing,
/// resource management, and retry mechanisms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Configuration for connecting to Kafka and subscribing to topics.
    pub kafka: KafkaConfig,
    /// Settings related to event processing, such as parallelism, batching, and deduplication.
    pub processing: ProcessingConfig,
    /// Defines limits for resource consumption (e.g., memory, in-flight messages)
    /// to enable backpressure and prevent system overload.
    pub resources: ResourceLimits,
    /// Specifies parameters for retrying failed operations (e.g., Kafka reconnections).
    pub retry: RetryConfig,
}

impl Config {
    /// Creates a new `Config` instance with specified Kafka credentials, allowing
    /// other configurations to use their default values. An optional `group_id_suffix`
    /// can be provided to customize the Kafka consumer group ID.
    pub fn new(username: String, password: String, group_id_suffix: Option<String>) -> Self {
        Self {
            kafka: KafkaConfig::new(username, password, group_id_suffix),
            processing: ProcessingConfig::default(),
            resources: ResourceLimits::default(),
            retry: RetryConfig::default(),
        }
    }

    /// Validates the entire SDK configuration.
    /// This method checks the validity of each component's configuration.
    ///
    /// # Returns
    /// `Ok(())` if the configuration is valid.
    /// `Err(crate::Error::Config)` if any part of the configuration is invalid.
    pub fn validate(&self) -> Result<()> {
        self.kafka.validate().map_err(|e| crate::Error::Config(format!("Kafka config validation failed: {}", e)))?;
        self.processing.validate().map_err(|e| crate::Error::Config(format!("Processing config validation failed: {}", e)))?;
        self.resources.validate().map_err(|e| crate::Error::Config(format!("Resource limits validation failed: {}", e)))?;
        self.retry.validate().map_err(|e| crate::Error::Config(format!("Retry config validation failed: {}", e)))?;
        Ok(())
    }
}

impl Default for Config {
    /// Provides a default `Config` instance.
    /// Note: Default `KafkaConfig` includes hardcoded credentials and SSL cert paths
    /// as per the prompt. These should be reviewed and likely overridden for production.
    fn default() -> Self {
        Self {
            kafka: KafkaConfig::default(),
            processing: ProcessingConfig::default(),
            resources: ResourceLimits::default(),
            retry: RetryConfig::default(),
        }
    }
}

/// Configuration specific to Kafka client behavior and connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaConfig {
    /// A list of Kafka broker addresses (e.g., `"host1:9093"`).
    pub brokers: Vec<String>,
    /// The Kafka consumer group ID.
    pub group_id: String,
    /// SASL username for Kafka authentication.
    pub username: String,
    /// SASL password for Kafka authentication.
    /// WARNING: Stored as a plain `String`. For enhanced security, consider
    /// using `secrecy::Secret` or managing secrets externally.
    pub password: String,
    /// A list of Kafka topics to subscribe to.
    pub topics: Vec<String>,
    /// SSL configuration for secure connection to Kafka.
    pub ssl: SslConfig,
    /// Session timeout for the Kafka consumer.
    pub session_timeout: Duration,
    /// Policy for resetting Kafka offsets if no initial offset is present or if the
    /// current offset is out of range (e.g., "earliest", "latest").
    pub auto_offset_reset: String,
    /// Maximum number of records to fetch in a single Kafka poll request.
    pub max_poll_records: usize, // Note: rdkafka might not use this directly; it's more for higher-level clients.
    /// Partition assignment strategy for the consumer group (e.g., "roundrobin", "range").
    pub partition_assignment_strategy: String,
}

impl KafkaConfig {
    /// Creates a new `KafkaConfig` with specified credentials and an optional suffix for the group ID.
    /// Other fields (brokers, topics, SSL paths, etc.) are taken from `KafkaConfig::default()`.
    pub fn new(username: String, password: String, group_id_suffix: Option<String>) -> Self {
        let base_group_id = username.clone(); // Use username as base for group ID
        let final_group_id = group_id_suffix
            .filter(|s| !s.is_empty()) // Ensure suffix is not empty if provided
            .map_or_else(
                || format!("{}-rust-sdk-{}", base_group_id, Uuid::new_v4()), // Default unique ID
                |suffix| format!("{}-{}", base_group_id, suffix) // User-provided suffix
            );

        Self {
            username,
            password,
            group_id: final_group_id,
            ..Default::default() // Populate other fields from default
        }
    }

    /// Validates the `KafkaConfig` settings.
    pub fn validate(&self) -> Result<()> {
        if self.brokers.is_empty() {
            return Err(crate::Error::Config("Kafka brokers list cannot be empty.".into()));
        }
        if self.username.is_empty() {
            return Err(crate::Error::Config("Kafka username cannot be empty.".into()));
        }
        if self.password.is_empty() { // Now a plain String, still check for empty
            return Err(crate::Error::Config("Kafka password cannot be empty.".into()));
        }
        if self.group_id.is_empty() {
            return Err(crate::Error::Config("Kafka group_id cannot be empty.".into()));
        }
        // It's good practice for group_id to relate to username for Bitquery, but not strictly enforced here
        // if defaults from prompt are used directly. This constructor `new` does prefix it.
        if self.topics.is_empty() {
             return Err(crate::Error::Config("Kafka topics list cannot be empty.".into()));
        }
        self.ssl.validate()?; // Validate SSL sub-configuration
        Ok(())
    }
}

impl Default for KafkaConfig {
    /// Provides default `KafkaConfig`.
    /// WARNING: Includes hardcoded credentials and SSL cert paths from the prompt.
    /// These are placeholders and MUST be configured appropriately for production.
    fn default() -> Self {
        Self {
            brokers: vec![
                "rpk0.bitquery.io:9093".to_string(),
                "rpk1.bitquery.io:9093".to_string(),
                "rpk2.bitquery.io:9093".to_string(),
            ],
            // Prompt default: "solana_113-rust-sdk".
            // Making it slightly more unique by default if using Config::new might be better.
            // Here, sticking to prompt's direct default.
            group_id: format!("solana_113-rust-sdk-{}", Uuid::new_v4().to_string().get(0..8).unwrap_or("def")),
            username: "solana_113".to_string(), // As per prompt
            password: "cuDDLAUguo75blkNdbCBSHvNCK1udw".to_string(), // As per prompt
            topics: vec![
                "solana.transactions.proto".to_string(),
                "solana.dextrades.proto".to_string(),
            ],
            ssl: SslConfig::default(),
            session_timeout: Duration::from_secs(30),
            auto_offset_reset: "latest".to_string(),
            max_poll_records: 500, // This is a high-level concept; rdkafka has different knobs.
            partition_assignment_strategy: "roundrobin".to_string(),
        }
    }
}

/// Configuration for SSL/TLS encryption for Kafka connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslConfig {
    /// Path to the Certificate Authority (CA) certificate file.
    pub ca_cert: String,
    /// Path to the client's private key file (PEM format).
    pub client_key: String,
    /// Path to the client's public certificate file (PEM format), corresponding to the client key.
    pub client_cert: String,
}

impl SslConfig {
    /// Validates the SSL configuration, checking if specified certificate files exist.
    /// Note: This checks for file existence only, not for cryptographic validity.
    pub fn validate(&self) -> Result<()> {
        // Paths are mandatory strings in this version.
        if self.ca_cert.is_empty() || !std::path::Path::new(&self.ca_cert).exists() {
            return Err(crate::Error::Config(format!(
                "SSL CA certificate path ('{}') is empty or file does not exist.", self.ca_cert
            )));
        }
        if self.client_key.is_empty() || !std::path::Path::new(&self.client_key).exists() {
            return Err(crate::Error::Config(format!(
                "SSL client key path ('{}') is empty or file does not exist.", self.client_key
            )));
        }
        if self.client_cert.is_empty() || !std::path::Path::new(&self.client_cert).exists() {
            return Err(crate::Error::Config(format!(
                "SSL client certificate path ('{}') is empty or file does not exist.", self.client_cert
            )));
        }
        Ok(())
    }
}

impl Default for SslConfig {
    /// Provides default paths for SSL certificates.
    /// WARNING: These are relative paths ("certs/..."). The application using the SDK
    /// must ensure these files are present at these locations relative to the execution
    /// directory, or these paths MUST be overridden.
    fn default() -> Self {
        Self {
            ca_cert: "certs/server.cer.pem".to_string(),    // Prompt default
            client_key: "certs/client.key.pem".to_string(), // Prompt default
            client_cert: "certs/client.cer.pem".to_string(),// Prompt default
        }
    }
}

/// Configuration related to event processing behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    /// Number of parallel worker tasks for event processing (e.g., in `BatchProcessor`).
    pub parallel_workers: usize,
    /// General buffer size for internal channels or queues.
    pub buffer_size: usize,
    /// Target number of events per batch for `BatchProcessor`.
    pub batch_size: usize,
    /// Maximum duration to wait before dispatching a partially filled batch.
    pub batch_timeout: Duration,
    /// Time window for message signature deduplication (used in `StreamConsumer`).
    pub dedup_window: Duration, // Note: New consumer uses HashSet, this might be for future LRU again.
                                // For now, it's not directly used by HashSet deduplicator.
    /// Enables or disables client-side pre-filtering of events in `StreamConsumer`.
    pub enable_pre_filtering: bool,
    /// Enables or disables metrics collection (checked by `InitConfig` in `lib.rs`).
    pub enable_metrics: bool,
}

impl ProcessingConfig {
    /// Validates the processing configuration settings.
    pub fn validate(&self) -> Result<()> {
        if self.parallel_workers == 0 {
            return Err(crate::Error::Config("parallel_workers must be at least 1.".into()));
        }
        if self.batch_size == 0 && self.parallel_workers > 0 {
             return Err(crate::Error::Config("batch_size must be positive if using batch processing features.".into()));
        }
        if self.buffer_size == 0 {
            return Err(crate::Error::Config("buffer_size must be positive.".into()));
        }
        // dedup_window and batch_timeout have wide acceptable ranges.
        Ok(())
    }
}

impl Default for ProcessingConfig {
    /// Provides default `ProcessingConfig`.
    fn default() -> Self {
        Self {
            parallel_workers: num_cpus::get().max(1), // Sensible default, at least 1 worker.
            buffer_size: 10000,
            batch_size: 100,
            batch_timeout: Duration::from_millis(500),
            dedup_window: Duration::from_secs(300), // 5 minutes (for potential future use with time-window dedupe)
            enable_pre_filtering: true,
            enable_metrics: cfg!(feature = "metrics"), // Default based on "metrics" cargo feature
        }
    }
}

/// Configuration for resource limits to manage system load and apply backpressure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum estimated total memory (in bytes) the SDK should aim to use before
    /// `ResourceManager` activates backpressure.
    pub max_memory_bytes: usize,
    /// Maximum number of messages allowed to be "in-flight" (i.e., fetched from Kafka
    /// and either queued or actively being processed).
    pub max_messages_in_flight: usize,
    /// Maximum capacity of internal queues (e.g., for `BatchProcessor`'s input).
    pub max_queue_size: usize,
    /// Interval at which the `ResourceManager` performs its monitoring checks.
    pub memory_check_interval: Duration,
    /// The percentage of `max_memory_bytes` or `max_messages_in_flight` that,
    /// if exceeded, triggers backpressure. Value should be between 0.0 and 1.0.
    pub backpressure_threshold: f64,
}

impl ResourceLimits {
    /// Validates the resource limit settings.
    pub fn validate(&self) -> Result<()> {
        if self.max_memory_bytes == 0 {
            return Err(crate::Error::Config("max_memory_bytes must be positive.".into()));
        }
        if self.max_messages_in_flight == 0 {
            return Err(crate::Error::Config("max_messages_in_flight must be positive.".into()));
        }
        if self.max_queue_size == 0 {
            return Err(crate::Error::Config("max_queue_size must be positive.".into()));
        }
        if !(0.1..=1.0).contains(&self.backpressure_threshold) { // Ensure threshold is meaningful
            return Err(crate::Error::Config("backpressure_threshold must be between 0.1 (exclusive, effectively) and 1.0 (inclusive).".into()));
        }
        if self.memory_check_interval < Duration::from_millis(100) { // Ensure reasonable check interval
            return Err(crate::Error::Config("memory_check_interval is too short, recommend >= 100ms.".into()));
        }
        Ok(())
    }
}

impl Default for ResourceLimits {
    /// Provides default `ResourceLimits`.
    fn default() -> Self {
        Self {
            max_memory_bytes: 4 * 1024 * 1024 * 1024, // 4GB
            max_messages_in_flight: 10000,
            max_queue_size: 50000,
            memory_check_interval: Duration::from_secs(1), // Check every second
            backpressure_threshold: 0.8, // Activate backpressure at 80% utilization
        }
    }
}

/// Configuration for retrying operations that might fail due to transient issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts for a single operation.
    pub max_retries: usize,
    /// Initial delay to wait before the first retry attempt.
    pub initial_delay: Duration,
    /// Maximum delay between retries, capping the exponential backoff.
    pub max_delay: Duration,
    /// The multiplier used for exponential backoff calculation (e.g., 2.0 for doubling).
    pub multiplier: f64,
}

impl RetryConfig {
    /// Validates the retry configuration settings.
    pub fn validate(&self) -> Result<()> {
        if self.multiplier < 1.0 {
            return Err(crate::Error::Config("Retry multiplier must be >= 1.0.".into()));
        }
        if self.initial_delay > self.max_delay && self.max_retries > 0 { // Only matters if retries happen
            return Err(crate::Error::Config("Initial retry delay cannot exceed max retry delay if retries are enabled.".into()));
        }
        Ok(())
    }
}

impl Default for RetryConfig {
    /// Provides default `RetryConfig`.
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0, // Standard exponential backoff
        }
    }
}
