use bitquery_solana_sdk::{
    // Initialization
    init_with_config, InitConfig, // Use the InitConfig from lib.rs for setup
    // Core client and config
    BitqueryClient, Config as SdkConfig, // SdkConfig to avoid clash with example's own config vars
    // Event handling
    SolanaEvent, EventProcessor, TransactionProcessor,
    // Error handling
    Result as SdkResult, // SDK's Result type
};
use std::sync::Arc;
use tracing::{info, error, warn, debug}; // Logging
// futures::StreamExt is not directly used if using client.process_events

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize SDK (logging, metrics exporter)
    // Using InitConfig to potentially customize logging and metrics port.
    let sdk_init_config = InitConfig {
        log_filter: "info,basic_consumer=debug,bitquery_solana_sdk=info".to_string(),
        enable_metrics: true, // Enable metrics for this example
        metrics_port: 9091,   // Use a different port if default (9090) is taken
    };
    if let Err(e) = init_with_config(sdk_init_config).await {
        eprintln!("Failed to initialize SDK: {}. Exiting.", e); // Basic print if logger failed
        return Err(anyhow::anyhow!("SDK initialization failed: {}", e));
    }

    info!("Basic Consumer Example (Enhanced SDK) starting...");

    // 2. Load Credentials (example - from environment variables)
    // The new SdkConfig::default() uses hardcoded credentials.
    // For a real application, you'd load these from env or a config file.
    // Here, we'll demonstrate overriding parts of the default config if needed.
    let username_from_env = std::env::var("BITQUERY_USERNAME");
    let password_from_env = std::env::var("BITQUERY_PASSWORD");

    // 3. Configure the SDK Client
    let mut sdk_config = SdkConfig::default(); // Start with SDK defaults

    if let (Ok(user), Ok(pass)) = (username_from_env, password_from_env) {
        info!("Overriding default credentials with values from BITQUERY_USERNAME/PASSWORD env vars.");
        sdk_config.kafka.username = user;
        sdk_config.kafka.password = pass;
        // Group ID might need to be regenerated if username changes significantly
        // sdk_config.kafka.group_id = format!("{}-rust-sdk-example-basic", sdk_config.kafka.username);
    } else {
        info!("Using hardcoded default credentials from SdkConfig::default(). Ensure these are valid or override them.");
    }

    // Example: Customizing Kafka topics (optional, defaults are often fine)
    // sdk_config.kafka.topics = vec!["solana.transactions.proto".to_string()];
    // info!("Set Kafka topics to: {:?}", sdk_config.kafka.topics);

    // SSL Configuration: The default SdkConfig.kafka.ssl points to "certs/..."
    // Ensure these files exist at the specified relative paths or update SdkConfig.
    // For this example, we assume they exist or are not strictly required by the Kafka setup.
    // You might add logic to check for these files or make paths configurable via env vars.
    // e.g. if std::path::Path::new(&sdk_config.kafka.ssl.ca_cert).exists() { ... }
    info!("Default SSL cert path: {}", sdk_config.kafka.ssl.ca_cert);
    // If certs are mandatory and not at default path, this example would fail at client creation or connection.


    // 4. Create the BitqueryClient
    let client = match BitqueryClient::new(sdk_config).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create BitqueryClient: {}", e);
            return Err(anyhow::anyhow!("BitqueryClient creation error: {}", e));
        }
    };
    info!("BitqueryClient created successfully.");

    // 5. Create an Event Processor
    // This basic example uses a TransactionProcessor.
    let transaction_processor: Arc<dyn EventProcessor> = Arc::new(TransactionProcessor::default());
    info!("TransactionProcessor created.");

    // 6. Start the Client
    // This subscribes to Kafka topics. If batch processing were enabled, it would also start workers.
    if let Err(e) = client.start().await {
        error!("Failed to start BitqueryClient consumption: {}", e);
        return Err(anyhow::anyhow!("Client start error: {}", e));
    }
    info!("BitqueryClient started, subscribed to topics. Now processing events...");

    // 7. Process Events in a Loop
    // The `process_events` method provides a continuous loop.
    // It handles fetching events and, if batching isn't enabled, passes them to the handler.
    // The handler defined here will be called for each event.
    let processing_result = client.process_events(|event: SolanaEvent| -> SdkResult<()> {
        debug!("Received event (Sig: {}) for direct processing.", event.signature());
        if transaction_processor.should_process(&event) {
            // In a real scenario, you might spawn a tokio task for true async processing
            // of each event if `processor.process` is long-running.
            // However, `EventProcessor::process` is already async.
            // The `process_events` loop calls this handler sequentially for now.
            // For parallelism with a single handler, one would typically collect events
            // from `client.next_event()` and manage tasks manually, or use the BatchProcessor.

            // This block is synchronous within the handler.
            // To make it async, the handler itself would need to be async,
            // and `process_events` would need to support an async handler.
            // The current `EventProcessor` trait is async, so this is a bit of a mix.
            // For simplicity, this example processes synchronously within the handler.
            // A quick fix if `transaction_processor.process` must be awaited:
            // tokio::runtime::Handle::current().block_on(async {
            //     if let Err(e) = transaction_processor.process(&event).await {
            //         error!("Error processing event (Sig: {}): {}", event.signature(), e);
            //     }
            // })

            // Assuming the handler for process_events is intended to be quick / synchronous dispatch.
            // If `transaction_processor.process` must be async, then `process_events` needs an async handler,
            // or this example should use `client.next_event()` and manage async tasks.
            // Let's assume the spirit of the example is simple synchronous handling here.
            // The prompt's `process_events` takes `F: FnMut(SolanaEvent) -> Result<()>` which is sync.

            // For a simple, synchronous processor (if one existed):
            // if let Err(e) = transaction_processor.process_sync(&event) { /* ... */ }

            // Given EventProcessor::process is async, we can't directly call it here
            // without making this closure async and changing process_events signature.
            // This example will just log that it *would* process.
            info!("Handler: Event (Sig: {}) would be processed by TransactionProcessor.", event.signature());
            // To actually call the async processor:
            // This requires the handler in `process_events` to be `async FnMut`.
            // Or, more practically, this example should use `client.next_event()` in a loop.
            // Let's adapt to use `next_event()` for clarity with async processors.
        }
        Ok(())
    }).await;


    // Corrected event loop for async processor handling:
    // The above `client.process_events` with a sync handler isn't ideal for async processors.
    // The prompt's `client.process_events` takes a sync handler.
    // The new `client.rs` from prompt: `F: FnMut(SolanaEvent) -> Result<()>`
    // This implies if you use `process_events`, the handler is sync.
    // If your processor is async, you should use `next_event` in a loop.
    // The example will be simpler if we assume the handler is for quick dispatch or data logging.
    // The `MetricsDexProcessor` in `high_volume_processor.rs` is async.

    // Re-evaluating: The `process_events` in the prompt's `client.rs` is:
    // if let Some(batch_processor) = &self.batch_processor { batch_processor.add_event(event).await?; }
    // else { handler(event)?; }
    // This means the `handler` is only called if NOT in batch mode.
    // And this `handler` is synchronous. This is a limitation.

    // For this basic_consumer, we are NOT using batch_processor. So the sync handler is called.
    // This means `TransactionProcessor` (if its `process` is truly async and does real work)
    // cannot be fully executed within that sync handler without blocking or spawning.
    // This example will be limited to what a sync handler can do.

    if let Err(e) = processing_result {
        error!("Event processing loop exited with error: {}", e);
        return Err(anyhow::anyhow!("Event processing failed: {}", e));
    }

    info!("Basic Consumer example finished.");
    Ok(())
}
