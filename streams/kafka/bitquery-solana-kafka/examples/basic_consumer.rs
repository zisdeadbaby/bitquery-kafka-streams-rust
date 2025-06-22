use bitquery_solana_core::{
    // SDK Initialization
    init_with_config, InitConfig, // Using InitConfig from lib.rs for SDK setup
    // Core SDK Client and Configuration
    BitqueryClient, Config as SdkFullConfig, // Aliasing Config to SdkFullConfig for clarity
    // Event Handling & Processing
    SolanaEvent, EventProcessor, TransactionProcessor, // Standard event types and processors
    // SDK's Result type for error handling
    Result as SdkResult,
};
use std::sync::Arc; // For Arc-wrapping the EventProcessor
use tracing::{info, error, warn, trace}; // Logging macros
// `futures::StreamExt` is not directly needed if using `client.next_event()` in a loop
// or `client.process_events()`.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize the SDK (Logging and Metrics Exporter)
    // Customize logging levels and metrics port if needed.
    let sdk_init_config = InitConfig {
        log_filter: "info,basic_consumer=debug,bitquery_solana_sdk=info".to_string(), // Adjust log levels
        enable_metrics: true,  // Enable metrics for this example
        metrics_port: 9091,    // Example port, ensure it's free or use a different one
    };
    if let Err(e) = init_with_config(sdk_init_config).await {
        // Use basic eprintln if logger initialization failed
        eprintln!("Fatal: Failed to initialize Bitquery Solana SDK: {}. Exiting.", e);
        return Err(anyhow::anyhow!("SDK initialization failed: {}", e));
    }

    info!("Basic Consumer Example (using Enhanced SDK) - Starting...");
    if cfg!(feature = "metrics") {
        info!("Metrics enabled. Prometheus exporter might be available (check SDK init log).");
    }


    // 2. Load Credentials (Example: from environment variables)
    // The new `SdkFullConfig::default()` uses hardcoded credentials from the prompt.
    // For a real application, these should be loaded securely.
    // This example demonstrates overriding default Kafka credentials if environment variables are set.
    let username_from_env = std::env::var("BITQUERY_USERNAME");
    let password_from_env = std::env::var("BITQUERY_PASSWORD");

    // 3. Configure the SDK Client
    let mut sdk_config = SdkFullConfig::default(); // Start with SDK defaults (includes hardcoded credentials)

    if let (Ok(user), Ok(pass)) = (username_from_env.as_ref(), password_from_env.as_ref()) {
        if !user.is_empty() && !pass.is_empty() {
            info!("Overriding default Kafka credentials with values from BITQUERY_USERNAME & BITQUERY_PASSWORD environment variables.");
            sdk_config.kafka.username = user.clone();
            sdk_config.kafka.password = pass.clone();
            // Optionally, regenerate group_id if username changes, to keep it user-specific
            // sdk_config.kafka.group_id = format!("{}-rust-sdk-basic-consumer-{}", user, uuid::Uuid::new_v4().to_string().get(0..8).unwrap_or("uid"));
        } else {
            info!("Environment variables for credentials found but are empty. Using SDK's default credentials.");
        }
    } else {
        info!("BITQUERY_USERNAME or BITQUERY_PASSWORD environment variables not set. Using SDK's default credentials.");
        warn!("Ensure default credentials in SdkFullConfig are valid or provide overrides for actual use.");
    }

    // SSL Configuration: SdkFullConfig::default().kafka.ssl points to "certs/...".
    // For this example to run, either these dummy files must exist at the specified relative paths,
    // or the Kafka setup must not require client certificates, or paths must be overridden.
    // Check if default cert paths exist; if not, log a warning.
    if !std::path::Path::new(&sdk_config.kafka.ssl.ca_cert).exists() {
        warn!("Default SSL CA certificate not found at: '{}'. Connection may fail if SSL client certs are required by Kafka.", sdk_config.kafka.ssl.ca_cert);
    }
    // Similar checks for client_key and client_cert could be added.

    // 4. Create the BitqueryClient
    // The client constructor (`BitqueryClient::new`) will validate the SdkFullConfig.
    let client = match BitqueryClient::new(sdk_config).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create BitqueryClient: {}", e);
            // Specific error handling for config validation might be useful here.
            if e.to_string().contains("SSL") && e.to_string().contains("file does not exist") {
                error!("Hint: This might be due to missing SSL certificate files. Check SslConfig paths.");
            }
            return Err(anyhow::anyhow!("BitqueryClient creation error: {}", e));
        }
    };
    info!("BitqueryClient created successfully.");

    // 5. Create an Event Processor
    // This basic example uses the `TransactionProcessor` to handle transaction events.
    // `Arc` is used because `EventProcessor` is a trait object and might be shared.
    let transaction_processor: Arc<dyn EventProcessor> = Arc::new(TransactionProcessor::default());
    info!("TransactionProcessor instance created.");

    // 6. Start the Client's Kafka Consumption
    // This subscribes the client to the configured Kafka topics.
    // If batch processing were enabled (it's not in this basic example),
    // this would also ensure BatchProcessor workers are ready.
    if let Err(e) = client.start().await {
        error!("Failed to start BitqueryClient's Kafka consumption: {}", e);
        return Err(anyhow::anyhow!("Client start error: {}", e));
    }
    info!("BitqueryClient started and subscribed to topics. Now entering event processing loop...");

    // 7. Process Events
    // Option A: Using `client.process_events` with a synchronous handler.
    // This is simpler if the handling logic per event is quick and synchronous.
    // The prompt's `client.rs` `process_events` takes `F: FnMut(SolanaEvent) -> Result<()>` (sync).
    // If `transaction_processor.process` (which is async) needs to be called, this isn't direct.
    // For this basic example, we'll simulate synchronous handling or just log.

    // info!("Using client.process_events (sync handler) for event loop...");
    // let processing_result = client.process_events(|event: SolanaEvent| -> SdkResult<()> {
    //     trace!("Handler received event (Sig: {})", event.signature());
    //     if transaction_processor.should_process(&event) {
    //         info!("SYNC_HANDLER: Event (Sig: {}) would be processed by TransactionProcessor.", event.signature());
    //         // To call the async `process` method, you'd need to block_on or spawn,
    //         // which is not ideal in a sync handler.
    //         // e.g., tokio::runtime::Handle::current().block_on(transaction_processor.process(&event))?;
    //         // This illustrates the limitation of a sync handler with async processors.
    //     }
    //     Ok(())
    // }).await;

    // Option B: Using `client.next_event()` in an async loop (more flexible for async processors).
    // This is generally preferred if your `EventProcessor::process` is async.
    info!("Using client.next_event() in an async loop for event processing...");
    let processing_result: SdkResult<()> = loop {
        tokio::select! {
            biased; // Prioritize shutdown signal.

            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl+C received. Shutting down basic consumer...");
                client.shutdown().await; // Call client's shutdown
                break Ok(()); // Exit loop successfully on Ctrl+C
            }

            next_event_result = client.next_event() => {
                match next_event_result {
                    Ok(Some(event)) => {
                        trace!("Received event (Sig: {}) via next_event().", event.signature());
                        if transaction_processor.should_process(&event) {
                            // Since transaction_processor.process is async, await it.
                            // Can spawn if concurrent processing of multiple events is desired,
                            // but for basic consumer, sequential await is simpler.
                            let proc_clone = transaction_processor.clone(); // Clone Arc for async move
                            if let Err(e) = proc_clone.process(&event).await {
                                error!("Error processing event (Sig: {}): {}", event.signature(), e);
                                // Decide if this error is fatal for the loop. For now, continue.
                            }
                        }
                    }
                    Ok(None) => {
                        // `next_event()` returned None, meaning no event right now (e.g. backpressure delay, empty poll).
                        // Loop will continue, and `next_event()` will eventually yield another event or error.
                        // A small sleep here can prevent a very tight loop if Kafka is truly idle.
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                    Err(e) => {
                        error!("Critical error fetching next event: {}. Terminating loop.", e);
                        client.shutdown().await; // Attempt graceful shutdown
                        break Err(e); // Exit loop with the error
                    }
                }
            }
        }
    };

    if let Err(e) = processing_result {
        error!("Event processing loop exited with error: {}", e);
        // Ensure error is propagated to main's Result for correct exit code.
        return Err(anyhow::anyhow!("Event processing failed: {}", e));
    }

    info!("Basic Consumer example finished gracefully.");
    Ok(())
}
