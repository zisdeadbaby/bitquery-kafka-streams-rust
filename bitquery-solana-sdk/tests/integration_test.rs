#[cfg(test)]
mod tests {
    use bitquery_solana_sdk::{
        Config as SdkConfig, KafkaConfig, SslConfig, ProcessingConfig, ResourceLimits, RetryConfig,
        Error as SdkError,
    };
    // use secrecy::Secret; // Not used in new Config for password
    use std::time::Duration;
    use tempfile::tempdir; // For creating temporary files for SSL cert tests
    use std::fs::{self, File}; // For writing dummy cert files & creating directory
    use std::path::Path;

    // Helper to create dummy certificate files for SslConfig tests
    fn create_dummy_certs(base_path: &Path, cert_dir_name: &str) -> std::io::Result<(String, String, String)> {
        let cert_path = base_path.join(cert_dir_name);
        fs::create_dir_all(&cert_path)?;

        let ca_file = cert_path.join("server.cer.pem");
        let key_file = cert_path.join("client.key.pem");
        let cert_file = cert_path.join("client.cer.pem");

        File::create(&ca_file)?;
        File::create(&key_file)?;
        File::create(&cert_file)?;

        Ok((
            ca_file.to_str().unwrap().to_string(),
            key_file.to_str().unwrap().to_string(),
            cert_file.to_str().unwrap().to_string(),
        ))
    }


    #[test]
    fn test_default_config_creation() {
        let config = SdkConfig::default();
        // Check a few default values to ensure they match the prompt's defaults
        assert_eq!(config.kafka.username, "solana_113");
        assert_eq!(config.kafka.password, "cuDDLAUguo75blkNdbCBSHvNCK1udw");
        assert_eq!(config.kafka.ssl.ca_cert, "certs/server.cer.pem");
        assert_eq!(config.resources.max_memory_bytes, 4 * 1024 * 1024 * 1024);
        assert_eq!(config.processing.parallel_workers, num_cpus::get().max(1));
    }

    #[test]
    fn test_kafka_config_validation() {
        let mut kafka_config = KafkaConfig::default();

        // Test valid default
        // Note: SslConfig validation will fail if dummy "certs/" dir doesn't exist.
        // For this specific unit test of KafkaConfig, we might provide a valid SslConfig manually.
        let temp_dir = tempdir().unwrap();
        let (ca, key, cert) = create_dummy_certs(temp_dir.path(), "valid_certs").unwrap();
        kafka_config.ssl = SslConfig { ca_cert: ca, client_key: key, client_cert: cert };
        assert!(kafka_config.validate().is_ok(), "Default KafkaConfig with valid SSL should be OK");

        // Test empty brokers
        let mut cfg_no_brokers = kafka_config.clone();
        cfg_no_brokers.brokers = vec![];
        assert!(matches!(cfg_no_brokers.validate(), Err(SdkError::Config(msg)) if msg.contains("brokers list cannot be empty")));

        // Test empty username
        let mut cfg_no_user = kafka_config.clone();
        cfg_no_user.username = "".to_string();
        assert!(matches!(cfg_no_user.validate(), Err(SdkError::Config(msg)) if msg.contains("username cannot be empty")));

        // Test empty password
        let mut cfg_no_pass = kafka_config.clone();
        cfg_no_pass.password = "".to_string();
        assert!(matches!(cfg_no_pass.validate(), Err(SdkError::Config(msg)) if msg.contains("password cannot be empty")));

        // Test empty group_id
        let mut cfg_no_group = kafka_config.clone();
        cfg_no_group.group_id = "".to_string();
        assert!(matches!(cfg_no_group.validate(), Err(SdkError::Config(msg)) if msg.contains("group_id cannot be empty")));

        // Test empty topics
        let mut cfg_no_topics = kafka_config;
        cfg_no_topics.topics = vec![];
        assert!(matches!(cfg_no_topics.validate(), Err(SdkError::Config(msg)) if msg.contains("topics list cannot be empty")));
    }

    #[test]
    fn test_ssl_config_validation() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create dummy files for a valid scenario
        let (ca_path_str, key_path_str, cert_path_str) = create_dummy_certs(base_path, "dummy_certs_dir").unwrap();

        let valid_ssl_config = SslConfig {
            ca_cert: ca_path_str.clone(),
            client_key: key_path_str.clone(),
            client_cert: cert_path_str.clone(),
        };
        assert!(valid_ssl_config.validate().is_ok());

        // Test with a non-existent CA file path
        let mut invalid_ssl_config = valid_ssl_config.clone();
        invalid_ssl_config.ca_cert = base_path.join("non_existent_ca.pem").to_str().unwrap().to_string();
        assert!(matches!(invalid_ssl_config.validate(), Err(SdkError::Config(msg)) if msg.contains("file does not exist")));

        // Test with an empty path for client key
        let mut empty_path_ssl_config = valid_ssl_config.clone();
        empty_path_ssl_config.client_key = "".to_string();
        assert!(matches!(empty_path_ssl_config.validate(), Err(SdkError::Config(msg)) if msg.contains("path is invalid or file does not exist")));
    }

    #[test]
    fn test_default_ssl_config_validation_failure_if_files_missing() {
        // The default SslConfig points to "certs/server.cer.pem", etc.
        // This test verifies that validation fails if these default paths don't point to existing files.
        let default_ssl = SslConfig::default();
        // This will fail unless a "certs" directory with the files exists relative to test execution.
        // For isolated unit tests, this is often problematic.
        // We expect this to fail in a clean environment.
        let validation_result = default_ssl.validate();
        assert!(validation_result.is_err(), "Default SSL config validation should fail if cert files don't exist at default paths.");
        if let Err(SdkError::Config(msg)) = validation_result {
            assert!(msg.contains("file does not exist"), "Error message should indicate missing file.");
        }
    }


    #[test]
    fn test_processing_config_validation() {
        let mut proc_config = ProcessingConfig::default();
        assert!(proc_config.validate().is_ok());

        proc_config.parallel_workers = 0;
        assert!(matches!(proc_config.validate(), Err(SdkError::Config(msg)) if msg.contains("parallel_workers must be at least 1")));

        proc_config.parallel_workers = 1; // Reset
        proc_config.batch_size = 0;
        // This validation depends on how batch_size=0 is interpreted with parallel_workers > 0
        assert!(matches!(proc_config.validate(), Err(SdkError::Config(msg)) if msg.contains("batch_size must be positive")));
    }

    #[test]
    fn test_resource_limits_validation() {
        let mut limits = ResourceLimits::default();
        assert!(limits.validate().is_ok());

        limits.max_memory_bytes = 0;
        assert!(matches!(limits.validate(), Err(SdkError::Config(msg)) if msg.contains("max_memory_bytes must be positive")) );
        limits.max_memory_bytes = 1024; // Reset

        limits.max_messages_in_flight = 0;
        assert!(matches!(limits.validate(), Err(SdkError::Config(msg)) if msg.contains("max_messages_in_flight must be positive")) );
        limits.max_messages_in_flight = 100; // Reset

        limits.max_queue_size = 0;
        assert!(matches!(limits.validate(), Err(SdkError::Config(msg)) if msg.contains("max_queue_size must be positive")) );
        limits.max_queue_size = 100; // Reset

        limits.backpressure_threshold = 0.0; // Too low
        assert!(matches!(limits.validate(), Err(SdkError::Config(msg)) if msg.contains("backpressure_threshold must be between 0.1 and 1.0")) );

        limits.backpressure_threshold = 1.1; // Too high
        assert!(matches!(limits.validate(), Err(SdkError::Config(msg)) if msg.contains("backpressure_threshold must be between 0.1 and 1.0")) );
    }

    #[test]
    fn test_retry_config_validation() {
        let mut retry = RetryConfig::default();
        assert!(retry.validate().is_ok());

        retry.multiplier = 0.5; // Invalid
        assert!(matches!(retry.validate(), Err(SdkError::Config(msg)) if msg.contains("Retry multiplier must be >= 1.0")) );
        retry.multiplier = 2.0; // Reset

        retry.initial_delay = Duration::from_secs(10);
        retry.max_delay = Duration::from_secs(5); // Invalid
        assert!(matches!(retry.validate(), Err(SdkError::Config(msg)) if msg.contains("Initial retry delay cannot exceed max retry delay")) );
    }

    #[test]
    fn test_full_config_validation() {
        let mut config = SdkConfig::default();
        // To make default config pass validation, especially SslConfig,
        // we need to either ensure dummy certs exist at "certs/..." or mock SslConfig.
        // For this test, let's assume user ensures certs exist if using default SslConfig paths.
        // Or, more robustly, override SslConfig for the test.
        let temp_dir = tempdir().unwrap();
        let (ca, key, cert) = create_dummy_certs(temp_dir.path(), "default_like_certs").unwrap();
        config.kafka.ssl = SslConfig { ca_cert: ca, client_key: key, client_cert: cert };

        assert!(config.validate().is_ok(), "Config validation failed: {:?}", config.validate().err());
    }

    #[test]
    fn test_config_serialization_kafka_password_is_string() {
        let mut config = SdkConfig::default();
        config.kafka.password = "mysecretpassword".to_string(); // Ensure it's set

        let json_output = serde_json::to_string(&config).expect("Failed to serialize SdkConfig");

        // Password should now be present as a plain string in the serialized JSON output
        assert!(json_output.contains("\"password\":\"mysecretpassword\""),
            "Serialized config should contain the plain string password. Actual: {}", json_output);

        // Deserialize and check
        let deserialized_config: SdkConfig = serde_json::from_str(&json_output).expect("Failed to deserialize SdkConfig");
        assert_eq!(deserialized_config.kafka.password, "mysecretpassword");
    }

    // TODO: Add integration tests for client creation, connection (requires Kafka setup or mocks).
    // TODO: Add tests for filter logic, batch processor behavior, resource manager actions.
    // These are more complex and might require specific test environments or mocking strategies.
}
