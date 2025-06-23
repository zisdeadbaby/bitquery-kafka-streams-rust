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
use tracing::{info, error};
use std::env;
use std::sync::Arc;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the SDK with production logging
    let sdk_init_config = InitConfig {
        log_filter: "info,zola_streams=info".to_string(),
        enable_metrics: true,
        metrics_port: 9090,
    };

    if let Err(e) = init_with_config(sdk_init_config).await {
        eprintln!("Fatal: Failed to initialize Zola Streams SDK: {}. Exiting.", e);
        return Err(anyhow::anyhow!("SDK initialization failed: {}", e));
    }

    info!("🚀 Zola Streams - Bitquery Solana Kafka Service Starting...");

    // Initialize observability components
    info!("Initializing observability components...");
    
    let health_monitor = Arc::new(HealthMonitor::new());
    let metrics_registry = Arc::new(MetricsRegistry::new());
    let _logger = Arc::new(BusinessEventLogger::new());
    let _tracer = Arc::new(Tracer::new("zola-streams", "1.0.0"));
    
    info!("✅ Observability components initialized");

    // Load configuration from environment or use defaults
    let mut config = SdkConfig::default();
    
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
