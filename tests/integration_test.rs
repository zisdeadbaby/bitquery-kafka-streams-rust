#[cfg(test)]
mod tests {
    use zola_streams::{
        // Main SDK Config and its components
        Config as SdkFullConfig, InitConfig, // InitConfig from lib.rs
        KafkaConfig, SslConfig, ProcessingConfig, ResourceLimits, RetryConfig,
        // SDK Error type for asserting validation results
        Error as SdkError,
    };
    use tracing::warn;
    use std::fs::{self, File}; // For filesystem operations (creating dummy cert files/dirs)
    use std::path::Path;
    use std::time::Duration;
    use tempfile::tempdir; // For creating temporary directories for tests

    // Helper function to create dummy certificate files for SslConfig tests.
    // Takes a base_path (e.g., from tempdir) and the directory name for certs (e.g., "certs").
    // Returns paths to the created dummy files.
    fn create_dummy_ssl_files(base_path: &Path, cert_subdir_name: &str) -> std::io::Result<(String, String, String)> {
        let certs_full_path = base_path.join(cert_subdir_name);
        fs::create_dir_all(&certs_full_path)?; // Create "certs" subdirectory within temp_dir

        let ca_file_path = certs_full_path.join("server.cer.pem");
        let key_file_path = certs_full_path.join("client.key.pem");
        let cert_file_path = certs_full_path.join("client.cer.pem");

        // Create empty dummy files
        File::create(&ca_file_path)?;
        File::create(&key_file_path)?;
        File::create(&cert_file_path)?;

        Ok((
            ca_file_path.to_str().unwrap().to_string(),
            key_file_path.to_str().unwrap().to_string(),
            cert_file_path.to_str().unwrap().to_string(),
        ))
    }

    #[test]
    fn test_sdk_full_config_defaults() {
        let config = SdkFullConfig::default();
        // Check some key default values to ensure they match the "Enhanced Version" prompt
        assert_eq!(config.kafka.username, "solana_113", "Default Kafka username mismatch.");
        assert_eq!(config.kafka.password, "cuDDLAUguo75blkNdbCBSHvNCK1udw", "Default Kafka password mismatch.");
        // Default SSL paths are relative, e.g., "certs/server.cer.pem"
        assert_eq!(config.kafka.ssl.ca_cert, "certs/server.cer.pem", "Default SSL CA cert path mismatch.");
        assert_eq!(config.resources.max_memory_bytes, 4 * 1024 * 1024 * 1024, "Default max_memory_bytes mismatch.");
        assert_eq!(config.processing.parallel_workers, num_cpus::get().max(1), "Default parallel_workers mismatch.");
        assert!(config.kafka.group_id.contains("solana_113-rust-sdk-"), "Default Kafka group_id should be unique and contain username prefix.");
    }

    #[test]
    fn test_lib_init_config_defaults() {
        let init_cfg = InitConfig::default(); // InitConfig from lib.rs
        assert_eq!(init_cfg.log_filter, "bitquery_solana_sdk=info");
        assert_eq!(init_cfg.enable_metrics, cfg!(feature = "metrics"));
        assert_eq!(init_cfg.metrics_port, 9090);
    }

    #[test]
    fn test_kafka_config_validation_logic() {
        let temp_ssl_dir = tempdir().expect("Failed to create temp dir for SSL certs");
        // Create dummy certs in a "certs" subdirectory of the temp_dir to match default SslConfig paths if needed
        let (ca_path, key_path, cert_path) = create_dummy_ssl_files(temp_ssl_dir.path(), "arbitrary_certs").unwrap();

        let mut valid_kafka_config = KafkaConfig::default();
        // Override default SSL paths with paths to freshly created dummy files for this test's isolation
        valid_kafka_config.ssl = SslConfig { ca_cert: ca_path, client_key: key_path, client_cert: cert_path };
        assert!(valid_kafka_config.validate().is_ok(), "Valid KafkaConfig (with mocked SSL) failed validation: {:?}", valid_kafka_config.validate().err());

        // Test cases for invalid KafkaConfig fields
        let mut test_cfg = valid_kafka_config.clone();
        test_cfg.brokers = vec![];
        assert!(matches!(test_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("brokers list cannot be empty")));

        test_cfg = valid_kafka_config.clone();
        test_cfg.username = "".to_string();
        assert!(matches!(test_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("username cannot be empty")));

        test_cfg = valid_kafka_config.clone();
        test_cfg.password = "".to_string(); // Password is now String, still check for empty
        assert!(matches!(test_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("password cannot be empty")));

        test_cfg = valid_kafka_config.clone();
        test_cfg.group_id = "".to_string();
        assert!(matches!(test_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("group_id cannot be empty")));

        test_cfg = valid_kafka_config; // Re-clone the valid one
        test_cfg.topics = vec![];
        assert!(matches!(test_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("topics list cannot be empty")));
    }

    #[test]
    fn test_ssl_config_validation_file_existence() {
        let temp_dir = tempdir().expect("Failed to create temp dir for SSL certs");
        let (ca_p, key_p, cert_p) = create_dummy_ssl_files(temp_dir.path(), "ssl_test_certs").unwrap();

        let config_valid = SslConfig { ca_cert: ca_p.clone(), client_key: key_p.clone(), client_cert: cert_p.clone() };
        assert!(config_valid.validate().is_ok(), "SslConfig with existing dummy files should be valid.");

        let config_missing_ca = SslConfig { ca_cert: temp_dir.path().join("missing_ca.pem").to_str().unwrap().to_string(), client_key: key_p.clone(), client_cert: cert_p.clone() };
        assert!(matches!(config_missing_ca.validate(), Err(SdkError::Config(msg)) if msg.contains("file does not exist")));

        let config_empty_key_path = SslConfig { ca_cert: ca_p.clone(), client_key: "".to_string(), client_cert: cert_p.clone() };
        assert!(matches!(config_empty_key_path.validate(), Err(SdkError::Config(msg)) if msg.contains("is empty or file does not exist")));
    }

    #[test]
    fn test_default_ssl_config_paths_validation_outcome() {
        let default_ssl_cfg = SslConfig::default();
        // This test's outcome depends on whether "certs/server.cer.pem" etc. exist
        // relative to where the test executable is run. In a typical `cargo test` invocation,
        // this is from the package root.
        // If these files are *not* present (as they wouldn't be in a clean checkout unless added),
        // validation *should* fail.
        if !Path::new("certs").exists() { // Check if "certs" dir itself is missing at project root
            warn!("Directory 'certs/' not found at project root. Default SslConfig validation will likely fail, which is expected in this case.");
        }
        let validation_result = default_ssl_cfg.validate();
        if Path::new(&default_ssl_cfg.ca_cert).exists() && Path::new(&default_ssl_cfg.client_key).exists() && Path::new(&default_ssl_cfg.client_cert).exists() {
            assert!(validation_result.is_ok(), "Default SslConfig validation should PASS if dummy cert files exist at default paths.");
        } else {
            assert!(validation_result.is_err(), "Default SslConfig validation should FAIL if dummy cert files DO NOT exist at default paths.");
            if let Err(SdkError::Config(msg)) = validation_result {
                assert!(msg.contains("file does not exist"), "Error message should indicate a missing SSL file.");
            }
        }
    }

    #[test]
    fn test_processing_config_validation_rules() {
        let mut proc_cfg = ProcessingConfig::default();
        assert!(proc_cfg.validate().is_ok(), "Default ProcessingConfig should be valid.");

        proc_cfg.parallel_workers = 0;
        assert!(matches!(proc_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("parallel_workers must be at least 1")));
        proc_cfg.parallel_workers = 1; // Reset to valid

        proc_cfg.batch_size = 0;
        // The condition `proc_cfg.parallel_workers > 0` is true here.
        assert!(matches!(proc_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("batch_size must be positive if using batch processing")));
        proc_cfg.batch_size = 1; // Reset to valid

        proc_cfg.buffer_size = 0;
        assert!(matches!(proc_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("buffer_size must be positive")) );
    }

    #[test]
    fn test_resource_limits_validation_rules() {
        let mut limits_cfg = ResourceLimits::default();
        assert!(limits_cfg.validate().is_ok(), "Default ResourceLimits should be valid.");

        limits_cfg.max_memory_bytes = 0;
        assert!(matches!(limits_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("max_memory_bytes must be positive")));
        limits_cfg.max_memory_bytes = 1024 * 1024; // Reset

        limits_cfg.max_messages_in_flight = 0;
        assert!(matches!(limits_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("max_messages_in_flight must be positive")));
        limits_cfg.max_messages_in_flight = 100; // Reset

        limits_cfg.max_queue_size = 0;
        assert!(matches!(limits_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("max_queue_size must be positive")));
        limits_cfg.max_queue_size = 1000; // Reset

        limits_cfg.backpressure_threshold = 0.05; // Too low (valid range is 0.1 to 1.0)
        assert!(matches!(limits_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("backpressure_threshold must be between")));

        limits_cfg.backpressure_threshold = 1.01; // Too high
        assert!(matches!(limits_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("backpressure_threshold must be between")));
        limits_cfg.backpressure_threshold = 0.8; // Reset

        limits_cfg.memory_check_interval = Duration::from_millis(50); // Too short
        assert!(matches!(limits_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("memory_check_interval is too short")));
    }

    #[test]
    fn test_retry_config_validation_rules() {
        let mut retry_cfg = RetryConfig::default();
        assert!(retry_cfg.validate().is_ok(), "Default RetryConfig should be valid.");

        retry_cfg.multiplier = 0.9; // Invalid, must be >= 1.0
        assert!(matches!(retry_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("Retry multiplier must be >= 1.0")));
        retry_cfg.multiplier = 2.0; // Reset

        retry_cfg.initial_delay = Duration::from_secs(15);
        retry_cfg.max_delay = Duration::from_secs(10); // Invalid, initial > max
        assert!(matches!(retry_cfg.validate(), Err(SdkError::Config(msg)) if msg.contains("Initial retry delay cannot exceed max retry delay")));
    }

    #[test]
    fn test_complete_sdk_config_validation() {
        let mut sdk_config = SdkFullConfig::default();
        // To make the default SdkFullConfig pass its own validation, we need to handle the SSL paths.
        // The SdkFullConfig::validate() calls kafka.validate() which calls ssl.validate().
        // If default "certs/..." files don't exist, this will fail.
        // So, for this test, we provide valid (dummy) SSL paths.
        let temp_ssl_dir = tempdir().expect("Failed to create temp dir for SSL certs in full config test");
        let (ca, key, cert) = create_dummy_ssl_files(temp_ssl_dir.path(), "full_config_ssl_certs").unwrap();
        sdk_config.kafka.ssl = SslConfig { ca_cert: ca, client_key: key, client_cert: cert };

        let validation_result = sdk_config.validate();
        assert!(validation_result.is_ok(), "Full SdkConfig validation failed: {:?}", validation_result.err());
    }

    #[test]
    fn test_kafka_config_password_serialization() {
        let mut config = SdkFullConfig::default();
        // KafkaConfig.password is now a plain String.
        config.kafka.password = "MyTestPassword123".to_string();

        let json_string = serde_json::to_string(&config).expect("Failed to serialize SdkFullConfig to JSON");

        // Password (being a plain String) SHOULD be present in the serialized JSON.
        assert!(json_string.contains("\"password\":\"MyTestPassword123\""),
            "Serialized config should contain the plain string password. Got: {}", json_string);

        // Deserialize and check if the password is correctly restored.
        let deserialized_config: SdkFullConfig = serde_json::from_str(&json_string)
            .expect("Failed to deserialize SdkFullConfig from JSON");
        assert_eq!(deserialized_config.kafka.password, "MyTestPassword123", "Deserialized password does not match original.");
    }
}
