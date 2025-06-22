use bitquery_solana_kafka::{
    init_with_config, InitConfig,
    BitqueryClient, Config as SdkConfig,
};
use tracing::{info, error};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize SDK with debug logging
    let init_config = InitConfig {
        log_filter: "debug,bitquery_solana_kafka=trace".to_string(),
        enable_metrics: true,
        metrics_port: 9090,
    };
    init_with_config(init_config).await?;

    info!("Testing Bitquery Kafka connection...");

    // Create client with environment overrides if needed
    let config = SdkConfig::default();
    
    // Only verify SSL paths if using SSL
    if std::env::var("KAFKA_SECURITY_PROTOCOL").unwrap_or_default() == "SASL_SSL" {
        if !std::path::Path::new(&config.kafka.ssl.ca_cert).exists() {
            error!("CA certificate not found at: {}", config.kafka.ssl.ca_cert);
            return Err(anyhow::anyhow!("Missing SSL certificates"));
        }
        info!("Using SSL connection");
    } else {
        info!("Using non-SSL (SASL_PLAINTEXT) connection");
    }

    // Create client
    let client = BitqueryClient::new(config).await?;
    
    // Start consumer
    client.start().await?;
    info!("Successfully connected to Bitquery Kafka!");
    
    // Test receiving one message
    match client.next_event().await? {
        Some(event) => {
            info!("Received event: {} in slot {}", event.signature(), event.slot());
        }
        None => {
            info!("No events available yet");
        }
    }
    
    client.shutdown().await;
    Ok(())
}
