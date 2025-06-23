//! Zola Streams - Bitquery Solana Kafka Streaming Service
//!
//! Main entry point for the Zola Streams service, providing high-performance
//! streaming of Solana blockchain data through Kafka.

use zola_streams::{
    BitqueryClient, Config as SdkConfig, InitConfig, init_with_config,
    error::Result as SdkResult,
};
use tracing::{info, error};
use std::env;
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
        result = run_service(client) => {
            match result {
                Ok(_) => info!("Service completed successfully"),
                Err(e) => error!("Service error: {}", e),
            }
        }
        _ = shutdown_signal => {
            info!("🛑 Shutdown signal received, stopping service...");
        }
    }

    info!("👋 Zola Streams service stopped");
    Ok(())
}

async fn run_service(client: BitqueryClient) -> SdkResult<()> {
    info!("Service is running. Processing Solana events from Kafka...");
    
    // Main event processing loop
    loop {
        match client.next_event().await {
            Ok(Some(event)) => {
                // Process the event - in a real implementation, this would be more sophisticated
                info!("Received event: {:?}", event);
            }
            Ok(None) => {
                // No events available, continue
                continue;
            }
            Err(e) => {
                error!("Error receiving event: {}", e);
                // In production, you might want to implement retry logic or circuit breakers
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
    }
}
