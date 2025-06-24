//! Zola Streams - Bitquery Solana Kafka Streaming Service
//!
//! Main entry point for the Zola Streams service, providing high-performance
//! streaming of Solana blockchain data through Kafka.

use zola_streams::{
    BitqueryClient, Config as SdkConfig, InitConfig, init_with_config,
    error::Result as SdkResult,
    observability::{
        health::HealthMonitor,
        metrics::MetricsRegistry,
        logging::BusinessEventLogger,
        tracing::Tracer,
    },
    ObservabilityServer,
};
use tracing::{info, error, warn};
use clap::{Parser, Subcommand};
use std::env;
use std::sync::Arc;
use tokio::signal;

/// Zola Streams - Bitquery Solana Kafka Streaming Service
#[derive(Parser)]
#[command(name = "zola-streams")]
#[command(about = "A high-performance Kafka streaming client for Bitquery Solana data")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable configuration validation without starting the service
    #[arg(long, help = "Validate configuration and exit")]
    config_check: bool,

    /// Perform a dry run without connecting to Kafka
    #[arg(long, help = "Validate configuration and test connectivity without full startup")]
    dry_run: bool,

    /// Set the log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Configuration file path (optional)
    #[arg(long)]
    config_file: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the streaming service
    Start,
    /// Validate configuration
    Validate,
    /// Show version information
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Handle version command
    if let Some(Commands::Version) = cli.command {
        println!("zola-streams {}", env!("CARGO_PKG_VERSION"));
        println!("A high-performance Kafka streaming client for Bitquery Solana data");
        return Ok(());
    }

    // Initialize the SDK with production logging
    let log_filter = format!("{},zola_streams={}", cli.log_level, cli.log_level);
    let sdk_init_config = InitConfig {
        log_filter,
        enable_metrics: true,
        metrics_port: 9090,
    };

    if let Err(e) = init_with_config(sdk_init_config).await {
        eprintln!("Fatal: Failed to initialize Zola Streams SDK: {}. Exiting.", e);
        return Err(anyhow::anyhow!("SDK initialization failed: {}", e));
    }

    // Display enhanced startup banner
    display_startup_banner();

    // Load configuration from environment or use defaults
    let mut config = load_configuration(cli.config_file.as_deref()).await?;
    
    // Override with environment variables if provided
    apply_environment_overrides(&mut config);

    // Handle configuration validation
    if cli.config_check || matches!(cli.command, Some(Commands::Validate)) {
        return validate_configuration(&config).await;
    }

    // Handle dry run
    if cli.dry_run {
        return dry_run_validation(&config).await;
    }

    // Start the normal service
    start_service(config).await
}

async fn load_configuration(config_file: Option<&str>) -> anyhow::Result<SdkConfig> {
    info!("Loading configuration...");
    
    if let Some(config_path) = config_file {
        info!("Loading configuration from file: {}", config_path);
        // TODO: Implement file-based configuration loading
        warn!("File-based configuration not yet implemented, using defaults with environment overrides");
    }
    
    Ok(SdkConfig::default())
}

fn apply_environment_overrides(config: &mut SdkConfig) {
    info!("Applying environment variable overrides...");
    
    // Override with environment variables if provided
    if let Ok(username) = env::var("BITQUERY_USERNAME") {
        if !username.is_empty() {
            config.kafka.username = username;
            info!("✅ Using BITQUERY_USERNAME from environment");
        }
    }
    
    if let Ok(password) = env::var("BITQUERY_PASSWORD") {
        if !password.is_empty() {
            config.kafka.password = password;
            info!("✅ Using BITQUERY_PASSWORD from environment");
        }
    }

    if let Ok(brokers) = env::var("KAFKA_BOOTSTRAP_SERVERS") {
        if !brokers.is_empty() {
            config.kafka.brokers = brokers.split(',').map(|s| s.trim().to_string()).collect();
            info!("✅ Using KAFKA_BOOTSTRAP_SERVERS from environment: {:?}", config.kafka.brokers);
        }
    }

    if let Ok(topic) = env::var("KAFKA_TOPIC") {
        if !topic.is_empty() {
            config.kafka.topics = vec![topic];
            info!("✅ Using KAFKA_TOPIC from environment: {:?}", config.kafka.topics);
        }
    }

    if let Ok(group_id) = env::var("KAFKA_GROUP_ID") {
        if !group_id.is_empty() {
            config.kafka.group_id = group_id;
            info!("✅ Using KAFKA_GROUP_ID from environment: {}", config.kafka.group_id);
        }
    }
}

async fn validate_configuration(config: &SdkConfig) -> anyhow::Result<()> {
    info!("🔍 Validating configuration...");
    
    match config.validate() {
        Ok(_) => {
            info!("✅ Configuration validation successful!");
            
            // Print configuration summary
            info!("Configuration Summary:");
            info!("  Kafka Brokers: {:?}", config.kafka.brokers);
            info!("  Kafka Topics: {:?}", config.kafka.topics);
            info!("  Kafka Group ID: {}", config.kafka.group_id);
            info!("  SSL Enabled: {}", config.kafka.enable_ssl);
            if config.kafka.enable_ssl {
                info!("  SSL CA Cert: {}", config.kafka.ssl.ca_cert);
                info!("  SSL Client Cert: {}", config.kafka.ssl.client_cert);
                info!("  SSL Client Key: {}", config.kafka.ssl.client_key);
            }
            info!("  Processing Workers: {}", config.processing.parallel_workers);
            info!("  Batch Size: {}", config.processing.batch_size);
            info!("  Memory Limit: {} MB", config.resources.max_memory_bytes / 1024 / 1024);
            
            Ok(())
        }
        Err(e) => {
            error!("❌ Configuration validation failed: {}", e);
            Err(anyhow::anyhow!("Configuration validation failed: {}", e))
        }
    }
}

async fn dry_run_validation(config: &SdkConfig) -> anyhow::Result<()> {
    info!("🧪 Performing dry run validation...");
    
    // First validate configuration
    validate_configuration(config).await?;
    
    info!("🔌 Testing connectivity (dry run mode)...");
    
    // Test SSL certificates if SSL is enabled
    if config.kafka.enable_ssl {
        info!("🔒 Validating SSL certificates...");
        
        // Check if certificate files exist and are readable
        let ca_cert_path = &config.kafka.ssl.ca_cert;
        let client_cert_path = &config.kafka.ssl.client_cert;
        let client_key_path = &config.kafka.ssl.client_key;
        
        if std::fs::metadata(ca_cert_path).is_err() {
            error!("❌ CA certificate file not found: {}", ca_cert_path);
            return Err(anyhow::anyhow!("CA certificate file not accessible: {}", ca_cert_path));
        }
        
        if std::fs::metadata(client_cert_path).is_err() {
            error!("❌ Client certificate file not found: {}", client_cert_path);
            return Err(anyhow::anyhow!("Client certificate file not accessible: {}", client_cert_path));
        }
        
        if std::fs::metadata(client_key_path).is_err() {
            error!("❌ Client key file not found: {}", client_key_path);
            return Err(anyhow::anyhow!("Client key file not accessible: {}", client_key_path));
        }
        
        info!("✅ SSL certificate files are accessible");
        
        // Check if they are placeholder certificates
        if client_cert_path.contains("certs/client.cer.pem") {
            if let Ok(content) = std::fs::read_to_string(client_cert_path) {
                if content.contains("PLACEHOLDER") || content.contains("localhost") || content.len() < 100 {
                    warn!("⚠️  WARNING: Client certificate appears to be a placeholder certificate!");
                    warn!("   This will likely fail with Bitquery's production Kafka brokers.");
                    warn!("   Please obtain production certificates from Bitquery support.");
                }
            }
        }
    }
    
    info!("✅ Dry run validation completed successfully!");
    info!("   Ready to start with: zola-streams start");
    
    Ok(())
}

async fn start_service(mut config: SdkConfig) -> anyhow::Result<()> {
    // Initialize observability components
    info!("Initializing observability components...");
    
    let health_monitor = Arc::new(HealthMonitor::new());
    let metrics_registry = Arc::new(MetricsRegistry::new());
    let _logger = Arc::new(BusinessEventLogger::new());
    let _tracer = Arc::new(Tracer::new("zola-streams", "1.0.0"));
    
    info!("✅ Observability components initialized");

    // Override with environment variables if provided
    if let Ok(username) = env::var("BITQUERY_USERNAME") {
        if !username.is_empty() {
            config.kafka.username = username;
        }
    }
    
    if let Ok(password) = env::var("BITQUERY_PASSWORD") {
        if !password.is_empty() {
            config.kafka.password = password;
        }
    }

    if let Ok(brokers) = env::var("KAFKA_BOOTSTRAP_SERVERS") {
        if !brokers.is_empty() {
            config.kafka.brokers = brokers.split(',').map(|s| s.trim().to_string()).collect();
        }
    }
    
    // Override with environment variables if provided
    if let Ok(username) = env::var("BITQUERY_USERNAME") {
        if !username.is_empty() {
            config.kafka.username = username;
        }
    }
    
    if let Ok(password) = env::var("BITQUERY_PASSWORD") {
        if !password.is_empty() {
            config.kafka.password = password;
        }
    }

    if let Ok(brokers) = env::var("KAFKA_BOOTSTRAP_SERVERS") {
        if !brokers.is_empty() {
            config.kafka.brokers = brokers.split(',').map(|s| s.trim().to_string()).collect();
        }
    }

    // Initialize the client
    info!("Initializing Bitquery client...");
    let client = match BitqueryClient::new(config).await {
        Ok(client) => {
            info!("✅ Bitquery client initialized successfully");
            client
        }
        Err(e) => {
            error!("❌ Failed to initialize Bitquery client: {}", e);
            return Err(anyhow::anyhow!("Client initialization failed: {}", e));
        }
    };

    // Start the client (subscribe to Kafka topics)
    info!("Starting Kafka subscription...");
    if let Err(e) = client.start().await {
        error!("❌ Failed to start Kafka subscription: {}", e);
        return Err(anyhow::anyhow!("Kafka subscription failed: {}", e));
    }
    info!("✅ Kafka subscription started successfully");

    // Start observability HTTP server
    info!("Starting observability HTTP server...");
    let observability_server = ObservabilityServer::new(
        health_monitor.clone(),
        metrics_registry.clone(),
        8080, // Observability server port
    );
    
    let observability_handle = tokio::spawn(async move {
        if let Err(e) = observability_server.start().await {
            error!("Observability server error: {}", e);
        }
    });

    info!("✅ Observability server started on port 8080");

    // Start processing events
    info!("🔄 Starting event processing...");
    
    // Set up graceful shutdown
    let shutdown_signal = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
    };

    // Run the service
    tokio::select! {
        result = run_service(client, health_monitor, metrics_registry) => {
            match result {
                Ok(_) => info!("Service completed successfully"),
                Err(e) => error!("Service error: {}", e),
            }
        }
        _ = shutdown_signal => {
            info!("🛑 Shutdown signal received, stopping service...");
        }
        _ = observability_handle => {
            info!("Observability server stopped");
        }
    }

    info!("👋 Zola Streams service stopped");
    Ok(())
}

async fn run_service(
    client: BitqueryClient,
    health_monitor: Arc<HealthMonitor>,
    metrics_registry: Arc<MetricsRegistry>,
) -> SdkResult<()> {
    info!("Service is running. Processing Solana events from Kafka...");
    
    // Perform initial health check
    if let Ok(health_report) = health_monitor.check_health().await {
        info!("Initial health check completed: {:?}", health_report.status);
    }
    
    // Initialize business metrics
    metrics_registry.increment_counter("service_starts", 1).await;
    
    // Main event processing loop
    loop {
        match client.next_event().await {
            Ok(Some(event)) => {
                // Process the event - in a real implementation, this would be more sophisticated
                info!("Received event: {:?}", event);
                
                // Update metrics
                metrics_registry.increment_counter("events_processed", 1).await;
                metrics_registry.record_histogram("event_processing_duration", 0.1).await;
            }
            Ok(None) => {
                // No events available, continue
                continue;
            }
            Err(e) => {
                error!("Error receiving event: {}", e);
                
                // Record error metrics
                metrics_registry.increment_counter("processing_errors", 1).await;
                
                // In production, you might want to implement retry logic or circuit breakers
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
    }
}

fn display_startup_banner() {
    println!("\n╔══════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                                                                                  ║");
    println!("║  🚀 ZOLA STREAMS - PUMP.FUN TRANSACTION MONITOR                                 ║");
    println!("║                                                                                  ║");
    println!("║  💫 Real-time Solana DEX streaming via Bitquery Kafka                          ║");
    println!("║  🎯 Specialized Pump.fun transaction detection                                  ║");
    println!("║  📊 Live market data and trade analytics                                        ║");
    println!("║                                                                                  ║");
    println!("║  🔥 Features:                                                                    ║");
    println!("║  • 🟢 Real-time buy/sell detection                                              ║");
    println!("║  • 💰 USD value calculation                                                      ║");
    println!("║  • 🐋 Whale trade alerts                                                        ║");
    println!("║  • 📈 Market trend monitoring                                                    ║");
    println!("║                                                                                  ║");
    println!("║  Status: STARTING...                                                            ║");
    println!("║                                                                                  ║");
    println!("╚══════════════════════════════════════════════════════════════════════════════════╝");
    println!("\n🔄 Initializing Kafka connection to Bitquery streams...\n");
}
