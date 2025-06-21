use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid; // For generating unique group_id if needed, though not in default

/// Main SDK configuration, aggregating configurations for different components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Kafka connection and subscription configuration.
    pub kafka: KafkaConfig,
    /// Configuration for event processing, batching, and parallelism.
    pub processing: ProcessingConfig,
    /// Limits for resource consumption (memory, in-flight messages).
    pub resources: ResourceLimits,
    /// Configuration for retry strategies on recoverable errors.
    pub retry: RetryConfig,
    // Note: InitConfig is now in lib.rs as per prompt's lib.rs structure for initialization params.
}

impl Config {
    /// Creates a new `Config` instance with specified Kafka credentials.
    /// Other configurations will use their default values.
    pub fn new(username: String, password: String, group_id_suffix: Option<String>) -> Self {
        Self {
            kafka: KafkaConfig::new(username, password, group_id_suffix),
            processing: ProcessingConfig::default(),
            resources: ResourceLimits::default(),
            retry: RetryConfig::default(),
        }
    }

    /// Validates the entire configuration.
    /// Placeholder for more comprehensive validation logic if needed.
    pub fn validate(&self) -> crate::Result<()> {
        self.kafka.validate()?;
        self.processing.validate()?;
        self.resources.validate()?;
        self.retry.validate()?;
        Ok(())
    }
}

// Default implementation for Config will use defaults of its members.
impl Default for Config {
    fn default() -> Self {
        Self {
            kafka: KafkaConfig::default(),
            processing: ProcessingConfig::default(),
            resources: ResourceLimits::default(),
            retry: RetryConfig::default(),
        }
    }
}

/// Kafka-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaConfig {
    /// List of Kafka broker addresses (e.g., "host1:port1,host2:port2").
    pub brokers: Vec<String>,
    /// Consumer group ID.
    pub group_id: String,
    /// SASL username for Kafka authentication.
    pub username: String,
    /// SASL password for Kafka authentication.
    /// Note: Stored as a plain String. Consider using `secrecy::Secret` for better protection in memory.
    pub password: String,
    /// List of Kafka topics to subscribe to.
    pub topics: Vec<String>,
    /// SSL configuration for Kafka connection.
    pub ssl: SslConfig,
    /// Kafka session timeout.
    pub session_timeout: Duration,
    /// Kafka auto offset reset policy (e.g., "earliest", "latest").
    pub auto_offset_reset: String,
    /// Maximum number of records to fetch in a single poll.
    pub max_poll_records: usize,
    /// Kafka partition assignment strategy (e.g., "roundrobin", "range").
    pub partition_assignment_strategy: String,
}

impl KafkaConfig {
    /// Creates a new KafkaConfig with specific credentials.
    pub fn new(username: String, password: String, group_id_suffix: Option<String>) -> Self {
        let base_group_id = username.clone();
        let final_group_id = match group_id_suffix {
            Some(suffix) if !suffix.is_empty() => format!("{}-{}", base_group_id, suffix),
            _ => format!("{}-{}", base_group_id, Uuid::new_v4().to_string()),
        };
        Self {
            username,
            password,
            group_id: final_group_id,
            ..Default::default() // Use default for other fields like brokers, topics etc.
        }
    }

    /// Validates Kafka configuration.
    pub fn validate(&self) -> crate::Result<()> {
        if self.brokers.is_empty() {
            return Err(crate::Error::Config("Kafka brokers list cannot be empty.".into()));
        }
        if self.username.is_empty() {
            return Err(crate::Error::Config("Kafka username cannot be empty.".into()));
        }
        if self.password.is_empty() {
            return Err(crate::Error::Config("Kafka password cannot be empty.".into()));
        }
        if self.group_id.is_empty() {
            return Err(crate::Error::Config("Kafka group_id cannot be empty.".into()));
        }
        // As per original SDK, group_id should start with username.
        // The prompt's default doesn't enforce this, but it's a common Bitquery requirement.
        // Let's assume for now the user is responsible or the default is blessed.
        // If strict validation is needed:
        // if !self.group_id.starts_with(&self.username) {
        //     return Err(crate::Error::Config(format!(
        //         "Kafka group_id must start with the username prefix ('{}'). Current: '{}'",
        //         self.username, self.group_id
        //     )));
        // }
        if self.topics.is_empty() {
             return Err(crate::Error::Config("Kafka topics list cannot be empty.".into()));
        }
        self.ssl.validate()?;
        Ok(())
    }
}


impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            brokers: vec![
                "rpk0.bitquery.io:9093".to_string(),
                "rpk1.bitquery.io:9093".to_string(),
                "rpk2.bitquery.io:9093".to_string(),
            ],
            // Default group_id from prompt. Consider making it unique per instance.
            group_id: "solana_113-rust-sdk-default".to_string(),
            username: "solana_113".to_string(), // Hardcoded default username from prompt
            password: "cuDDLAUguo75blkNdbCBSHvNCK1udw".to_string(), // Hardcoded default password from prompt
            topics: vec![
                "solana.transactions.proto".to_string(),
                "solana.dextrades.proto".to_string(),
            ],
            ssl: SslConfig::default(),
            session_timeout: Duration::from_secs(30),
            auto_offset_reset: "latest".to_string(),
            max_poll_records: 500,
            partition_assignment_strategy: "roundrobin".to_string(),
        }
    }
}

/// SSL configuration for secure Kafka connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslConfig {
    /// Path to the CA certificate file.
    pub ca_cert: String,
    /// Path to the client's private key file.
    pub client_key: String,
    /// Path to the client's public certificate file.
    pub client_cert: String,
}

impl SslConfig {
    /// Validates SSL file paths.
    /// This version assumes paths are mandatory if SSL is used.
    /// The default impl provides hardcoded paths; ensure these files exist or are configurable.
    pub fn validate(&self) -> crate::Result<()> {
        // In this enhanced version, SSL paths are not Option<>s, so they must be non-empty.
        // The check for file existence is crucial.
        if self.ca_cert.is_empty() || !std::path::Path::new(&self.ca_cert).exists() {
            return Err(crate::Error::Config(format!("SSL CA certificate path is invalid or file does not exist: '{}'", self.ca_cert)));
        }
        if self.client_key.is_empty() || !std::path::Path::new(&self.client_key).exists() {
            return Err(crate::Error::Config(format!("SSL client key path is invalid or file does not exist: '{}'", self.client_key)));
        }
        if self.client_cert.is_empty() || !std::path::Path::new(&self.client_cert).exists() {
            return Err(crate::Error::Config(format!("SSL client certificate path is invalid or file does not exist: '{}'", self.client_cert)));
        }
        Ok(())
    }
}

impl Default for SslConfig {
    fn default() -> Self {
        // Default paths are relative. Application needs to ensure these files are present.
        // This is risky for a library. Better to have Option<String> and require users to set them.
        // However, sticking to the prompt's structure.
        Self {
            ca_cert: "certs/server.cer.pem".to_string(),
            client_key: "certs/client.key.pem".to_string(),
            client_cert: "certs/client.cer.pem".to_string(),
        }
    }
}

/// Configuration for event processing logic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    /// Number of parallel workers for event processing (e.g., for `BatchProcessor`).
    pub parallel_workers: usize,
    /// Size of internal buffers/channels for messages.
    pub buffer_size: usize,
    /// Target batch size for `BatchProcessor`.
    pub batch_size: usize,
    /// Timeout for forming a batch in `BatchProcessor`.
    pub batch_timeout: Duration,
    /// Time window for message deduplication in `StreamConsumer`.
    pub dedup_window: Duration,
    /// Flag to enable pre-filtering of events in `StreamConsumer`.
    pub enable_pre_filtering: bool,
    /// Flag to enable metrics collection (used by `lib.rs`'s `InitConfig`).
    pub enable_metrics: bool,
}

impl ProcessingConfig {
    /// Validates processing configuration.
    pub fn validate(&self) -> crate::Result<()> {
        if self.parallel_workers == 0 {
            return Err(crate::Error::Config("parallel_workers must be at least 1.".into()));
        }
        if self.batch_size == 0 && self.parallel_workers > 0 { // If batch processing is implied by workers
             return Err(crate::Error::Config("batch_size must be positive if using batch processing.".into()));
        }
        // buffer_size, batch_timeout, dedup_window have wider acceptable ranges.
        Ok(())
    }
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            parallel_workers: num_cpus::get().max(1), // Default to num_cpus, min 1
            buffer_size: 10000,
            batch_size: 100,
            batch_timeout: Duration::from_millis(500),
            dedup_window: Duration::from_secs(300), // 5 minutes
            enable_pre_filtering: true,
            enable_metrics: cfg!(feature = "metrics"), // Default based on "metrics" feature
        }
    }
}

/// Configuration for resource limits and backpressure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum estimated memory usage in bytes before applying backpressure.
    pub max_memory_bytes: usize,
    /// Maximum number of messages considered "in-flight" (queued or being processed).
    pub max_messages_in_flight: usize,
    /// Maximum size of internal queues (e.g., for `BatchProcessor`).
    pub max_queue_size: usize,
    /// Interval for checking memory and other resource statuses.
    pub memory_check_interval: Duration,
    /// Memory usage percentage (of `max_memory_bytes`) to trigger backpressure.
    pub backpressure_threshold: f64, // Percentage, e.g., 0.8 for 80%
}

impl ResourceLimits {
    /// Validates resource limits configuration.
    pub fn validate(&self) -> crate::Result<()> {
        if self.max_memory_bytes == 0 {
            return Err(crate::Error::Config("max_memory_bytes must be positive.".into()));
        }
        if self.max_messages_in_flight == 0 {
            return Err(crate::Error::Config("max_messages_in_flight must be positive.".into()));
        }
        if self.max_queue_size == 0 {
            return Err(crate::Error::Config("max_queue_size must be positive.".into()));
        }
        if !(0.1..=1.0).contains(&self.backpressure_threshold) {
            return Err(crate::Error::Config("backpressure_threshold must be between 0.1 and 1.0.".into()));
        }
        Ok(())
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 4 * 1024 * 1024 * 1024, // 4GB
            max_messages_in_flight: 10000,
            max_queue_size: 50000,
            memory_check_interval: Duration::from_secs(1),
            backpressure_threshold: 0.8, // 80%
        }
    }
}

/// Configuration for retry attempts on transient errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts for an operation.
    pub max_retries: usize,
    /// Initial delay before the first retry.
    pub initial_delay: Duration,
    /// Maximum possible delay for a single retry, capping exponential backoff.
    pub max_delay: Duration,
    /// Multiplier for calculating the next delay in exponential backoff.
    pub multiplier: f64,
}

impl RetryConfig {
    /// Validates retry configuration.
    pub fn validate(&self) -> crate::Result<()> {
        if self.multiplier < 1.0 {
            return Err(crate::Error::Config("Retry multiplier must be >= 1.0.".into()));
        }
        if self.initial_delay > self.max_delay {
            return Err(crate::Error::Config("Initial retry delay cannot exceed max retry delay.".into()));
        }
        Ok(())
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}
