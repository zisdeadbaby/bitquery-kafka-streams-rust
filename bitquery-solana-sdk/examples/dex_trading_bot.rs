use bitquery_solana_sdk::{
    // Initialization
    init_with_config, InitConfig,
    // Core client and config
    BitqueryClient, Config as SdkConfig,
    // Event handling & filtering
    SolanaEvent, EventType, EventFilter, FilterBuilder, // Added FilterBuilder
    // Processors
    EventProcessor, DexProcessor,
    // Error handling
    Result as SdkResult,
};
use std::sync::Arc;
use tokio::runtime::Handle; // For block_on if needed, or manage async tasks
use tracing::{info, error, warn, debug};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize SDK (logging, metrics exporter)
    let sdk_init_config = InitConfig {
        log_filter: "info,dex_trading_bot=debug,bitquery_solana_sdk=info".to_string(),
        enable_metrics: true,
        metrics_port: 9092, // Different port for this example
    };
    if let Err(e) = init_with_config(sdk_init_config).await {
        eprintln!("Failed to initialize SDK: {}. Exiting.", e);
        return Err(anyhow::anyhow!("SDK initialization failed: {}", e));
    }

    info!("DEX Trading Bot Example (Enhanced SDK) starting...");

    // 2. Load Credentials (if overriding defaults)
    let username_from_env = std::env::var("BITQUERY_USERNAME");
    let password_from_env = std::env::var("BITQUERY_PASSWORD");

    // 3. Configure the SDK Client
    let mut sdk_config = SdkConfig::default(); // Start with SDK defaults

    if let (Ok(user), Ok(pass)) = (username_from_env, password_from_env) {
        info!("Overriding default credentials with ENV VARS.");
        sdk_config.kafka.username = user;
        sdk_config.kafka.password = pass;
    } else {
        info!("Using hardcoded default credentials. Ensure these are valid or override.");
    }

    // Specifically for DEX trades, filter topics
    sdk_config.kafka.topics = vec!["solana.dextrades.proto".to_string()];
    info!("Set Kafka topics to: {:?}", sdk_config.kafka.topics);

    // Example: Customize processing settings (though not using BatchProcessor in this example)
    // sdk_config.processing.dedup_window = std::time::Duration::from_secs(120); // 2 min dedup

    // SSL: Assuming default cert paths "certs/..." are valid or not strictly needed.

    // 4. Create the BitqueryClient
    let client = BitqueryClient::new(sdk_config).await
        .map_err(|e| anyhow::anyhow!("BitqueryClient creation error: {}", e))?;
    info!("BitqueryClient created.");

    // 5. Set up Event Pre-filtering (optional)
    // Example: Filter for specific DEX program IDs even before processor.should_process
    let pre_filter = FilterBuilder::new()
        .event_types(vec![EventType::DexTrade]) // Only interested in DEX trades
        .program_ids(vec![ // Only from these specific DEX programs
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(), // Raydium
            "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),  // Jupiter
        ])
        .min_amount(1000.0) // Trades where base amount is at least 1000 (e.g. 1000 SOL or 1000 BTC if it's base)
        .build();
    client.set_filter(pre_filter).await?;
    info!("Event pre-filter applied to client.");


    // 6. Create a DEX Processor with custom settings
    let dex_processor: Arc<dyn EventProcessor> = Arc::new(DexProcessor {
        min_amount_usd: 10000.0, // Processor's own filter: only trades > $10k USD value
        target_programs: vec![ // This list can be same or subset of pre_filter.program_ids
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(),
            "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc".to_string(),  // Orca (Whirlpools)
        ],
    });
    info!("DexProcessor configured for trades > $10k USD and specific programs.");

    // 7. Start the Client
    client.start().await
        .map_err(|e| anyhow::anyhow!("Client start error: {}", e))?;
    info!("BitqueryClient started. Listening for DEX trades...");

    // 8. Process Events using `next_event()` for async processor execution
    // Since `EventProcessor::process` is async, we need to manage its execution.
    // The `client.process_events` takes a sync handler, so it's not suitable here.
    info!("Starting event loop using client.next_event()...");
    loop {
        tokio::select! {
            biased; // Prioritize shutdown signal

            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl+C received. Shutting down DEX trading bot...");
                client.shutdown().await; // Assuming client has an async shutdown
                break;
            }

            event_result = client.next_event() => {
                match event_result {
                    Ok(Some(event)) => {
                        // `next_event` has already applied the client-level pre-filter.
                        // Now, apply the processor's own `should_process` logic.
                        if dex_processor.should_process(&event) {
                            debug!("Event (Sig: {}) passed DexProcessor.should_process. Processing...", event.signature());
                            // Spawn a task for each event to allow concurrent processing
                            // if the processor itself involves I/O or significant computation.
                            // Or, if processing is quick, can await directly.
                            let processor_clone = dex_processor.clone();
                            tokio::spawn(async move {
                                if let Err(e) = processor_clone.process(&event).await {
                                    warn!("Error processing DEX trade (Sig: {}): {}", event.signature(), e);
                                }
                            });
                        } else {
                            // Event was received from Kafka, passed client pre-filter, but this specific processor doesn't want it.
                            trace_event_details_skipped(&event, "DexProcessor.should_process returned false");
                        }
                    }
                    Ok(None) => {
                        // No event currently available (e.g. backpressure delay, Kafka poll empty)
                        // Brief sleep to prevent tight loop if Kafka is idle.
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                    Err(e) => {
                        error!("Error fetching next event: {}. Terminating loop.", e);
                        client.shutdown().await;
                        return Err(anyhow::anyhow!("Event stream error: {}", e));
                    }
                }
            }
        }
    }

    info!("DEX Trading Bot example finished.");
    Ok(())
}

fn trace_event_details_skipped(event: &SolanaEvent, reason: &str) {
    use tracing::trace;
    trace!(
        "Event Skipped: Type={:?}, Slot={}, Sig={}, Reason={}",
        event.event_type(),
        event.slot(),
        event.signature(),
        reason
    );
}
