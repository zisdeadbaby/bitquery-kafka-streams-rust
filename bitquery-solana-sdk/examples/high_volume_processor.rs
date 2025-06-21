use bitquery_solana_sdk::{
    // SDK Initialization
    init_with_config, InitConfig, // Using the enhanced InitConfig from lib.rs
    // Core Client & Configuration
    BitqueryClient, Config as SdkConfig, // Aliasing SdkConfig
    ProcessingConfig, ResourceLimits, KafkaConfig, SslConfig, // Specific config parts
    // Event Handling & Filtering
    SolanaEvent, EventType, EventFilter, FilterBuilder,
    // Processors & Error Handling
    EventProcessor, DexProcessor, Result as SdkResult,
    // Utilities (if directly used, e.g. metrics)
    utils::metrics as sdk_metrics,
};
use async_trait::async_trait; // For implementing EventProcessor
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Duration;
use tracing::{info, error, warn, debug};

// Custom processor that wraps DexProcessor and adds specific metrics/logging
struct MonitoredDexProcessor {
    inner_dex_processor: DexProcessor,
    events_processed_count: Arc<AtomicU64>, // Counter for this processor instance
}

#[async_trait]
impl EventProcessor for MonitoredDexProcessor {
    async fn process(&self, event: &SolanaEvent) -> SdkResult<()> {
        let _timer = sdk_metrics::Timer::new("monitored_dex_processor_process_event");
        debug!("MonitoredDexProcessor processing event (Sig: {})", event.signature());

        // Call the inner DexProcessor's process method
        let result = self.inner_dex_processor.process(event).await;

        // Increment count regardless of result (as an attempt was made)
        self.events_processed_count.fetch_add(1, AtomicOrdering::Relaxed);

        // Record specific metric for this processor
        sdk_metrics::record_event_processed("monitored_dex_trade", result.is_ok());

        if let Err(ref e) = result {
            warn!("MonitoredDexProcessor: Error processing event (Sig: {}): {}", event.signature(), e);
        }
        result
    }

    fn should_process(&self, event: &SolanaEvent) -> bool {
        // Delegate to inner DexProcessor's should_process logic
        let should = self.inner_dex_processor.should_process(event);
        if should {
            debug!("MonitoredDexProcessor: Event (Sig: {}) PASSED should_process.", event.signature());
        } else {
            // trace!("MonitoredDexProcessor: Event (Sig: {}) FAILED should_process.", event.signature());
        }
        should
    }
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize SDK (logging, metrics exporter)
    let sdk_init_config = InitConfig {
        log_filter: "info,high_volume_processor=debug,bitquery_solana_sdk=info".to_string(),
        enable_metrics: true, // Metrics are key for high-volume
        metrics_port: 9090,   // Standard Prometheus port for this example
    };
    if let Err(e) = init_with_config(sdk_init_config).await {
        eprintln!("Failed to initialize SDK: {}. Exiting.", e);
        return Err(anyhow::anyhow!("SDK initialization failed: {}", e));
    }

    info!("High-Volume Processor Example (Enhanced SDK) starting...");
    info!("Metrics will be available at http://localhost:9090/metrics (if enabled).");

    // 2. Configure the SDK Client for High Volume
    // Start with SDK defaults and then customize extensively.
    let mut sdk_config = SdkConfig::default();

    // Kafka settings for higher throughput
    sdk_config.kafka.topics = vec![
        "solana.transactions.proto".to_string(),
        "solana.dextrades.proto".to_string(),
        // "solana.tokens.proto".to_string(), // Add if token transfers are also processed
    ];
    sdk_config.kafka.max_poll_records = 1000; // Fetch more records per poll
    sdk_config.kafka.session_timeout = Duration::from_secs(60); // Longer session timeout

    // Processing settings for parallelism and batching
    sdk_config.processing.parallel_workers = num_cpus::get().max(4); // Use more workers, e.g., num CPUs or more if I/O bound
    sdk_config.processing.buffer_size = 50_000;      // Larger internal buffers
    sdk_config.processing.batch_size = 500;          // Larger batches
    sdk_config.processing.batch_timeout = Duration::from_millis(200); // Shorter batch timeout for quicker dispatch
    sdk_config.processing.enable_pre_filtering = true; // Enable client-side pre-filtering

    // Resource limits tuned for higher capacity
    sdk_config.resources = ResourceLimits {
        max_memory_bytes: 8 * 1024 * 1024 * 1024, // 8GB max estimated memory
        max_messages_in_flight: 50_000,          // More messages allowed in the system
        max_queue_size: 100_000,                 // Larger internal queues for batch processor
        memory_check_interval: Duration::from_secs(5), // Check memory less frequently if stable
        backpressure_threshold: 0.75,            // Activate backpressure at 75% of memory limit
    };

    // Retry config can also be tuned if needed
    // sdk_config.retry.max_retries = 3;

    // SSL: Assuming default cert paths "certs/..." are valid or not strictly needed for the target Kafka.
    // If they are needed and not present, client creation will fail.
    info!("Using default SSL cert paths: CA='{}', Key='{}', Cert='{}'",
        sdk_config.kafka.ssl.ca_cert, sdk_config.kafka.ssl.client_key, sdk_config.kafka.ssl.client_cert);


    // 3. Create the BitqueryClient
    // Note: `enable_batch_processing` takes `&mut self`, so client must be mutable.
    let mut client = BitqueryClient::new(sdk_config).await
        .map_err(|e| anyhow::anyhow!("BitqueryClient creation error: {}", e))?;
    info!("BitqueryClient created with high-volume configuration.");

    // 4. Set up Event Pre-filtering (Client-level)
    // Example: Filter for DEX trades from specific programs with a minimum USD value (estimated by base amount).
    let client_filter = FilterBuilder::new()
        .event_types(vec![EventType::DexTrade])
        .program_ids(vec![
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(), // Raydium
            "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),  // Jupiter
        ])
        // .min_amount(1.0) // Example: filter out very small base amounts even before processor logic
        .build();
    client.set_filter(client_filter).await?;
    info!("Client-level event pre-filter applied.");

    // 5. Create the Event Processor (MonitoredDexProcessor)
    let total_events_counter = Arc::new(AtomicU64::new(0));
    let monitored_processor = Arc::new(MonitoredDexProcessor {
        inner_dex_processor: DexProcessor { // Customize inner DexProcessor
            min_amount_usd: 5000.0, // Processor focuses on trades > $5k USD
            target_programs: vec![ // Can be same/subset/superset of client_filter.program_ids
                "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(),
                "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),
            ],
        },
        events_processed_count: total_events_counter.clone(),
    });
    info!("MonitoredDexProcessor created.");

    // 6. Enable Batch Processing
    // This routes events through BatchProcessor using the `monitored_processor`.
    client.enable_batch_processing(monitored_processor.clone() as Arc<dyn EventProcessor>).await?;
    info!("Batch processing enabled.");

    // 7. Start the Client
    // This subscribes to Kafka and starts BatchProcessor workers.
    client.start().await
        .map_err(|e| anyhow::anyhow!("Client start error: {}", e))?;
    info!("BitqueryClient started. Processing events via BatchProcessor...");

    // 8. Monitor Processing Rate (Example)
    let monitoring_counter_clone = total_events_counter.clone();
    tokio::spawn(async move {
        let mut last_processed_count = 0u64;
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            let current_processed_count = monitoring_counter_clone.load(AtomicOrdering::Relaxed);
            let events_in_interval = current_processed_count - last_processed_count;
            let rate_per_second = events_in_interval as f64 / 10.0;
            info!(
                "Processing Monitor: ~{:.2} events/sec. Total processed by MonitoredDexProcessor: {}.",
                rate_per_second, current_processed_count
            );
            last_processed_count = current_processed_count;

            // Also log resource manager status
            // This requires access to client.resource_manager(), which is tricky from a detached task
            // unless passed in. For simplicity, this example omits direct RM logging here.
            // RM logs its own status periodically via its internal monitoring_loop.
        }
    });

    // 9. Process Events
    // The `process_events` method will now send events to the BatchProcessor.
    // The handler passed here is only called if batch processing is *not* enabled.
    // Since we enabled it, this handler will effectively be ignored.
    let processing_result = client.process_events(|event: SolanaEvent| -> SdkResult<()> {
        // This closure will not be called because batch processing is enabled.
        // Events are routed to BatchProcessor.add_event instead.
        warn!("Direct event handler called in high_volume_processor - this should not happen if batching is enabled. Event: {:?}", event.signature());
        Ok(())
    }).await;

    if let Err(e) = processing_result {
        error!("High-volume event processing loop exited with error: {}", e);
        client.shutdown().await; // Attempt graceful shutdown
        return Err(anyhow::anyhow!("Event processing failed: {}", e));
    }

    // Add a Ctrl+C handler for graceful shutdown of the main loop if `process_events` were to run indefinitely without error.
    // However, `process_events` as implemented will return on major error.
    // For an indefinite run controlled by Ctrl+C:
    // tokio::select! {
    //     res = client.process_events(|_| Ok(())) => { if let Err(e) = res { error!("Processing error: {}", e); }},
    //     _ = tokio::signal::ctrl_c() => { info!("Ctrl+C received, shutting down."); }
    // }
    // client.shutdown().await;


    info!("High-Volume Processor example finished (or was interrupted).");
    Ok(())
}
