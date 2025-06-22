use bitquery_solana_kafka::{
    init_with_config, InitConfig,
    BitqueryClient, Config as SdkConfig,
    EventType, FilterBuilder,
    DexProcessor, EventProcessor,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{info, error};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_with_config(InitConfig::default()).await?;

    let mut config = SdkConfig::default();
    // Subscribe only to DEX trades topic
    config.kafka.topics = vec!["solana.dextrades.proto".to_string()];
    
    let client = BitqueryClient::new(config).await?;
    
    // Apply pre-filter at client level
    let filter = FilterBuilder::new()
        .event_types(vec![EventType::DexTrade])
        .min_amount(1000.0) // Filter for significant trades
        .build();
    
    client.set_filter(filter).await?;
    
    // Track metrics
    let trade_count = Arc::new(AtomicU64::new(0));
    let volume_usd = Arc::new(AtomicU64::new(0));
    
    // Start monitoring
    let processor: Arc<dyn EventProcessor> = Arc::new(DexProcessor::default());
    client.start().await?;
    
    // Spawn metrics reporter
    let count_clone = trade_count.clone();
    let volume_clone = volume_usd.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let trades = count_clone.load(Ordering::Relaxed);
            let vol = volume_clone.load(Ordering::Relaxed);
            info!("Last minute: {} trades, ${} volume", trades, vol);
            count_clone.store(0, Ordering::Relaxed);
            volume_clone.store(0, Ordering::Relaxed);
        }
    });
    
    // Process events
    loop {
        match client.next_event().await {
            Ok(Some(event)) => {
                if processor.should_process(&event) {
                    trade_count.fetch_add(1, Ordering::Relaxed);
                    
                    // Extract trade value
                    if let (Some(amount), Some(price)) = 
                        (event.amount_base(), event.price()) {
                        let value = (amount * price) as u64;
                        volume_usd.fetch_add(value, Ordering::Relaxed);
                    }
                    
                    processor.process(&event).await?;
                }
            }
            Ok(None) => tokio::time::sleep(Duration::from_millis(10)).await,
            Err(e) => {
                error!("Stream error: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}
