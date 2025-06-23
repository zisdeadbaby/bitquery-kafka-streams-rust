use zola_streams::{
    // SDK Initialization
    init_with_config, InitConfig,
    // Core Client & Configuration
    BitqueryClient, Config as SdkFullConfig, // Aliasing Config
    // Event Handling, Filtering
    EventType, FilterBuilder,
    // Processors & Error Handling
    EventProcessor, DexProcessor,
    // Error handling
    error::Result as SdkResult,
};
use std::sync::Arc;
use tracing::{info, error, warn, debug, trace}; // Logging utilities
// futures::StreamExt not directly used with client.next_event() or client.process_events()

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize SDK (Logging and Metrics Exporter)
    let sdk_init_config = InitConfig {
        log_filter: "info,dex_trading_bot=debug,bitquery_solana_sdk=info".to_string(), // Customize log levels
        enable_metrics: true,  // Enable metrics for this example
        metrics_port: 9092,    // Example port, ensure it's free
    };
    if let Err(e) = init_with_config(sdk_init_config).await {
        eprintln!("Fatal: Failed to initialize Bitquery Solana SDK: {}. Exiting.", e);
        return Err(anyhow::anyhow!("SDK initialization failed: {}", e));
    }

    info!("DEX Trading Bot Example (Enhanced SDK) - Starting...");
    if cfg!(feature = "metrics") {
        info!("Metrics enabled. Prometheus exporter might be available (check SDK init log).");
    }

    // 2. Load Credentials (Example: from environment variables)
    // Overrides default hardcoded credentials in SdkFullConfig if env vars are set.
    let username_from_env = std::env::var("BITQUERY_USERNAME");
    let password_from_env = std::env::var("BITQUERY_PASSWORD");

    // 3. Configure the SDK Client
    let mut sdk_config = SdkFullConfig::default(); // Start with SDK defaults

    if let (Ok(user), Ok(pass)) = (username_from_env.as_ref(), password_from_env.as_ref()) {
        if !user.is_empty() && !pass.is_empty() {
            info!("Overriding default Kafka credentials with ENV VARS.");
            sdk_config.kafka.username = user.clone();
            sdk_config.kafka.password = pass.clone();
        } else {
            info!("Env vars for credentials found but empty. Using SDK's default credentials.");
        }
    } else {
        info!("BITQUERY_USERNAME or BITQUERY_PASSWORD env vars not set. Using SDK's default credentials.");
        warn!("Ensure default credentials in SdkFullConfig are valid or provide overrides.");
    }

    // Specifically configure for DEX trades
    sdk_config.kafka.topics = vec!["solana.dextrades.proto".to_string()];
    info!("Kafka topics set for DEX trades: {:?}", sdk_config.kafka.topics);

    // SSL Configuration: Default SdkFullConfig points to "certs/...".
    // Ensure these files exist or paths are correctly overridden for your setup.
    if !std::path::Path::new(&sdk_config.kafka.ssl.ca_cert).exists() {
        warn!("Default SSL CA certificate not found at: '{}'. Connection may fail if SSL client certs are required.", sdk_config.kafka.ssl.ca_cert);
    }


    // 4. Create the BitqueryClient
    let client = BitqueryClient::new(sdk_config).await
        .map_err(|e| {
            error!("Failed to create BitqueryClient: {}", e);
            // Provide hint for common SSL cert issue with default paths
            if e.to_string().contains("SSL") && e.to_string().contains("file does not exist") {
                 error!("Hint: This might be due to missing SSL certificate files. Check SslConfig paths in your configuration or ensure default 'certs/' directory is correctly populated.");
            }
            anyhow::anyhow!("BitqueryClient creation error: {}", e)
        })?;
    info!("BitqueryClient created successfully.");

    // 5. Set up Client-Level Event Pre-filtering (Optional)
    // This filter is applied by the StreamConsumer before events are even passed to processors or batchers.
    let client_side_filter = FilterBuilder::new()
        .event_types(vec![EventType::DexTrade]) // Ensure only DEX trades are considered
        .program_ids(vec![ // Example: Only interested in trades from these specific DEX programs
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(), // Raydium
            "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),  // Jupiter
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc".to_string(),  // Orca (Whirlpools)
        ])
        // .min_amount(100.0) // Example: Filter out trades with base amount less than 100 (e.g., 100 SOL)
        .build();
    client.set_filter(client_side_filter).await?;
    info!("Client-level event pre-filter applied for specific DEX trades.");


    // 6. Create a DEX Processor with its own fine-grained logic
    // This processor will further evaluate events that pass the client-level filter.
    let dex_processor: Arc<dyn EventProcessor> = Arc::new(DexProcessor {
        min_amount_usd: 10000.0, // Processor's own filter: only interested in trades > $10,000 USD value
        target_programs: vec![ // This list can be same, subset, or superset of client_filter.program_ids
                               // If more restrictive, it further narrows down. If broader, client_filter takes precedence.
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(),
            "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc".to_string(),
        ],
    });
    info!("DexProcessor configured for trades > $10k USD on specified DEX programs.");

    // 7. Start the Client's Kafka Consumption
    client.start().await
        .map_err(|e| anyhow::anyhow!("Client start error: {}", e))?;
    info!("BitqueryClient started and subscribed. Listening for DEX trades...");

    // 8. Process Events using `client.next_event()` in an async loop
    // This approach is suitable for handling async `EventProcessor::process` calls.
    info!("Starting event loop using client.next_event() for DEX trade processing...");
    let processing_loop_result: SdkResult<()> = loop {
        tokio::select! {
            biased; // Prioritize the Ctrl+C signal for immediate shutdown.

            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl+C signal received. Initiating graceful shutdown of DEX trading bot...");
                client.shutdown().await; // Call the client's shutdown method
                break Ok(()); // Exit the loop successfully.
            }

            next_event_result = client.next_event() => {
                match next_event_result {
                    Ok(Some(event)) => {
                        // `client.next_event()` has already applied the client-level pre-filter.
                        // Now, apply the DexProcessor's specific `should_process` logic.
                        if dex_processor.should_process(&event) {
                            debug!("Event (Sig: {}) passed DexProcessor.should_process. Dispatching for async processing...", event.signature());

                            // Clone Arcs for the spawned task.
                            let processor_clone = dex_processor.clone();
                            // Spawn a new Tokio task to process the event asynchronously.
                            // This allows the main loop to continue fetching new events while
                            // current events are being processed, achieving concurrency.
                            tokio::spawn(async move {
                                if let Err(e) = processor_clone.process(&event).await {
                                    warn!("Error during async processing of DEX trade (Sig: {}): {}", event.signature(), e);
                                }
                            });
                        } else {
                            // Event was received from Kafka and passed client's pre-filter,
                            // but this specific DexProcessor instance decided not to process it.
                            trace!("Event (Sig: {}) skipped by DexProcessor.should_process. Event type: {:?}, Program: {:?}",
                                event.signature(), event.event_type(), event.program_id().unwrap_or("N/A"));
                        }
                    }
                    Ok(None) => {
                        // `next_event()` returned `None`. This indicates no event is currently available
                        // (e.g., due to internal backpressure delay in consumer, or Kafka poll was empty).
                        // The loop will continue, and `next_event()` will eventually yield another event or an error.
                        // A brief sleep can prevent a very tight loop if Kafka topic is genuinely idle.
                        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await; // Short pause
                    }
                    Err(e) => {
                        error!("Critical error encountered while fetching next event: {}. Terminating event loop.", e);
                        client.shutdown().await; // Attempt graceful shutdown on critical error.
                        break Err(e); // Exit the loop, propagating the error.
                    }
                }
            }
        }
    };

    if let Err(e) = processing_loop_result {
        error!("DEX trading bot event processing loop exited with error: {}", e);
        return Err(anyhow::anyhow!("Event processing failed: {}", e));
    }

    info!("DEX Trading Bot example finished gracefully.");
    Ok(())
}
