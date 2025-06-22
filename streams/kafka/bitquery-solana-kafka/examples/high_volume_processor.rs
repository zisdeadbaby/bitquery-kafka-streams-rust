use bitquery_solana_kafka::{
    // SDK Initialization
    init_with_config, InitConfig,
    // Core Client & Configuration
    BitqueryClient, Config as SdkFullConfig, // Aliasing Config
    ResourceLimits, // Specific config structs
    // Event Handling, Filtering
    SolanaEvent, EventType, FilterBuilder,
    // Processors & Error Handling
    EventProcessor, DexProcessor,
    // Utilities (e.g., for custom metrics if needed, or direct access to SDK's metrics utils)
    utils::metrics as sdk_metrics_utils,
    // Error handling
    error::Result as SdkResult,
};
use async_trait::async_trait; // For implementing EventProcessor for custom wrappers
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering}; // For custom counters
use std::time::Duration;
use tracing::{info, error, warn, debug}; // Logging utilities

/// A custom `EventProcessor` that wraps `DexProcessor` to add specific monitoring
/// or metrics for events processed by this instance.
struct MonitoredDexProcessor {
    inner_dex_processor: DexProcessor, // The actual DEX processing logic
    events_attempted_count: Arc<AtomicU64>, // Counts events this processor attempts to handle
    events_succeeded_count: Arc<AtomicU64>, // Counts events successfully processed
}

#[async_trait]
impl EventProcessor for MonitoredDexProcessor {
    async fn process(&self, event: &SolanaEvent) -> SdkResult<()> {
        // Use SDK's Timer for measuring duration of this specific processor's `process` call
        let _timer = sdk_metrics_utils::Timer::new("monitored_dex_processor_event_duration");
        debug!("MonitoredDexProcessor: Processing event (Sig: {})", event.signature());

        // Delegate to the inner DexProcessor's processing logic
        let result = self.inner_dex_processor.process(event).await;

        self.events_attempted_count.fetch_add(1, AtomicOrdering::Relaxed);
        if result.is_ok() {
            self.events_succeeded_count.fetch_add(1, AtomicOrdering::Relaxed);
        }

        // Use SDK's standard metric recording for event processing status
        sdk_metrics_utils::record_event_processed("monitored_dex_trade", result.is_ok());

        if let Err(ref e) = result {
            warn!("MonitoredDexProcessor: Error during inner_dex_processor.process for event (Sig: {}): {}", event.signature(), e);
        }
        result // Propagate the result
    }

    fn should_process(&self, event: &SolanaEvent) -> bool {
        // Delegate the decision to the inner DexProcessor
        let should = self.inner_dex_processor.should_process(event);
        if should {
            debug!("MonitoredDexProcessor: Event (Sig: {}) WILL be processed based on inner DexProcessor rules.", event.signature());
        } else {
            // Using trace for potentially noisy "skipped" messages
            tracing::trace!("MonitoredDexProcessor: Event (Sig: {}) WILL NOT be processed by inner DexProcessor.", event.signature());
        }
        should
    }
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize SDK (Logging and Metrics Exporter)
    // Configure for potentially verbose logging from SDK and this example, enable metrics.
    let sdk_init_config = InitConfig {
        log_filter: "info,high_volume_processor=debug,bitquery_solana_sdk=info".to_string(),
        enable_metrics: true, // Metrics are crucial for high-volume scenarios
        metrics_port: 9090,   // Default Prometheus port, ensure it's available
    };
    if let Err(e) = init_with_config(sdk_init_config).await {
        eprintln!("Fatal: Failed to initialize Bitquery Solana SDK: {}. Exiting.", e);
        return Err(anyhow::anyhow!("SDK initialization failed: {}", e));
    }

    info!("High-Volume Processor Example (Enhanced SDK) - Starting...");
    if cfg!(feature = "metrics") {
        info!("Metrics enabled. Prometheus exporter should be running (check SDK init log for port details, e.g., http://localhost:9090/metrics).");
    }

    // 2. Configure the SDK Client for High Volume Processing
    let mut sdk_config = SdkFullConfig::default(); // Start with defaults, then customize

    // Kafka settings optimized for higher throughput
    sdk_config.kafka.topics = vec![ // Subscribe to multiple relevant topics
        "solana.transactions.proto".to_string(),
        "solana.dextrades.proto".to_string(),
        // "solana.tokens.proto".to_string(), // Uncomment if token transfers are also of interest
    ];
    // `max_poll_records` is a conceptual setting for some Kafka clients;
    // `rdkafka` (used by this SDK) manages fetching with byte-based settings.
    // We can increase `max.partition.fetch.bytes` in `client.rs` if needed,
    // or rely on `BitqueryClient`'s internal batching/queuing.
    // The prompt's `KafkaConfig` has `max_poll_records`, so we set it.
    sdk_config.kafka.max_poll_records = 1000; // Conceptual; actual impact depends on rdkafka tuning.
    sdk_config.kafka.session_timeout = Duration::from_secs(45); // Slightly longer for stability under load

    // Processing settings: increased parallelism, larger batches, adjusted timeouts
    sdk_config.processing.parallel_workers = num_cpus::get().max(4); // Use more workers
    sdk_config.processing.buffer_size = 50_000;      // Larger internal SDK buffers (e.g., for consumer)
    sdk_config.processing.batch_size = 500;          // Larger batches for BatchProcessor
    sdk_config.processing.batch_timeout = Duration::from_millis(250); // Shorter batch timeout
    sdk_config.processing.enable_pre_filtering = true; // Enable client-side pre-filtering

    // Resource limits: allow more memory and in-flight messages
    sdk_config.resources = ResourceLimits {
        max_memory_bytes: 8 * 1024 * 1024 * 1024, // 8GB max estimated SDK memory footprint
        max_messages_in_flight: 50_000,          // Allow more messages in the processing pipeline
        max_queue_size: 100_000,                 // Larger internal queues for BatchProcessor's input
        memory_check_interval: Duration::from_secs(2), // Check resources periodically
        backpressure_threshold: 0.75,            // Activate backpressure at 75% of resource limits
    };

    // SSL: Assuming default "certs/..." paths are valid or Kafka doesn't require client certs.
    if !std::path::Path::new(&sdk_config.kafka.ssl.ca_cert).exists() {
        warn!("Default SSL CA certificate not found at: '{}'. Connection may fail if SSL client certs are required.", sdk_config.kafka.ssl.ca_cert);
    }

    // 3. Create the BitqueryClient (mutable for enabling batch processing)
    let mut client = BitqueryClient::new(sdk_config).await
        .map_err(|e| anyhow::anyhow!("BitqueryClient creation error: {}", e))?;
    info!("BitqueryClient created with high-volume configuration.");

    // 4. Set up Client-Level Event Pre-filtering
    // This filter is applied by `StreamConsumer` before events reach `BatchProcessor` or direct handlers.
    let client_side_filter = FilterBuilder::new()
        .event_types(vec![EventType::DexTrade]) // Focus only on DEX trades for this example
        .program_ids(vec![ // From specific DEX programs
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(), // Raydium
            "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),  // Jupiter
        ])
        .min_amount(50000.0) // Filter for trades with base amount > 50,000 (e.g., 50k SOL)
                             // Note: `min_amount` in `EventFilter` applies to raw amount, not USD value.
        .build();
    client.set_filter(client_side_filter).await?;
    info!("Client-level event pre-filter applied: focusing on large DEX trades from specific programs.");

    // 5. Create the Event Processor (MonitoredDexProcessor)
    let events_processed_by_monitor = Arc::new(AtomicU64::new(0));
    let monitored_processor_instance = Arc::new(MonitoredDexProcessor {
        inner_dex_processor: DexProcessor { // Configure the inner DexProcessor
            min_amount_usd: 100_000.0, // This processor instance looks for trades > $100k USD value
            target_programs: vec![ // It might have its own list of target programs or reuse.
                "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(),
                "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),
            ],
        },
        events_attempted_count: events_processed_by_monitor.clone(),
        events_succeeded_count: Arc::new(AtomicU64::new(0)), // Separate counter for successes by this processor
    });
    info!("MonitoredDexProcessor created, targeting trades > $100k USD.");

    // 6. Enable Batch Processing
    // This configures the client to route events through `BatchProcessor` using `monitored_processor_instance`.
    client.enable_batch_processing(monitored_processor_instance.clone() as Arc<dyn EventProcessor>).await?;
    info!("Batch processing enabled. Events will be processed in batches by parallel workers.");

    // 7. Start the Client's Kafka Consumption
    // This subscribes to topics and starts `BatchProcessor` workers.
    client.start().await
        .map_err(|e| anyhow::anyhow!("Client start error: {}", e))?;
    info!("BitqueryClient started. Processing events via BatchProcessor...");

    // 8. Monitor Processing Rate (Example using the processor's counter)
    let monitor_loop_counter = events_processed_by_monitor.clone();
    tokio::spawn(async move {
        let mut last_snapshot_count = 0u64;
        let mut logging_interval = tokio::time::interval(Duration::from_secs(10)); // Log every 10 seconds
        loop {
            logging_interval.tick().await;
            let current_total_processed = monitor_loop_counter.load(AtomicOrdering::Relaxed);
            let events_since_last_log = current_total_processed - last_snapshot_count;
            let rate_per_second = events_since_last_log as f64 / 10.0; // Avg rate over last 10s

            info!(
                "Processing Monitor (MonitoredDexProcessor attempts): ~{:.2} events/sec. Total attempts: {}.",
                rate_per_second, current_total_processed
            );
            last_snapshot_count = current_total_processed;
            // For more detailed resource monitoring, can query `client.resource_manager()`
            // if it's made accessible or if ResourceManager logs its own status.
        }
    });

    // 9. Process Events using the main loop
    // Since batch processing is enabled, `client.process_events` will internally send events
    // to the `BatchProcessor`. The closure provided here will NOT be called.
    let main_processing_loop_result = client.process_events(|event: SolanaEvent| -> SdkResult<()> {
        // This closure is effectively a no-op when batch processing is enabled.
        // It's here to satisfy the `process_events` signature.
        warn!("Direct event handler called in high_volume_processor (event Sig: {}). This should not occur when batch processing is enabled.", event.signature());
        Ok(())
    }).await;

    // Handle potential error from the main processing loop.
    if let Err(e) = main_processing_loop_result {
        error!("High-volume event processing loop exited with a critical error: {}", e);
        client.shutdown().await; // Attempt graceful shutdown of components
        return Err(anyhow::anyhow!("Event processing failed critically: {}", e));
    }

    // For an indefinitely running service, you might await a shutdown signal here
    // if `process_events` could return Ok(()) under some conditions (e.g., stream ends).
    // However, `process_events` as designed typically loops until a critical error.
    // A Ctrl+C handler can be added around the `process_events` call if needed for manual shutdown.
    // Example:
    // tokio::select! {
    //     res = client.process_events(|_| Ok(())) => { /* handle res */ },
    //     _ = tokio::signal::ctrl_c() => { info!("Ctrl+C received by high_volume_processor, initiating shutdown..."); }
    // }
    // client.shutdown().await;


    info!("High-Volume Processor example finished (or was interrupted by error).");
    Ok(())
}
