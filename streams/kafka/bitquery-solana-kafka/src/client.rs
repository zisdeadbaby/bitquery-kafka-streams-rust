use crate::{
    batch_processor::BatchProcessor,
    config::Config as SdkMainConfig, // Aliased to avoid conflict
    consumer::StreamConsumer,
    error::Result as SdkResult, // Using SDK's Result type consistently
    events::SolanaEvent,
    filters::EventFilter,
    processors::EventProcessor, // Trait for custom event processors
    resource_manager::ResourceManager,
};
use rdkafka::config::ClientConfig as RdKafkaClientConfig; // For Kafka client configuration
use rdkafka::consumer::StreamConsumer as RdKafkaStreamConsumer;
// No direct import of rdkafka::consumer::Consumer trait, as StreamConsumer encapsulates this.
use std::sync::Arc;
use tokio::sync::RwLock; // For read-write lock on shared components like StreamConsumer
use tracing::{info, debug, error, warn}; // Logging utilities
use crate::Error;

/// `BitqueryClient` is the primary interface for applications to interact with the SDK.
///
/// It orchestrates:
/// - Kafka connection and message consumption via `StreamConsumer`.
/// - Optional event filtering using `EventFilter`.
/// - Optional batch processing via `BatchProcessor`.
/// - Resource management and backpressure via `ResourceManager`.
/// - Exposing consumed `SolanaEvent`s to the application.
pub struct BitqueryClient {
    config: Arc<SdkMainConfig>, // Shared SDK configuration
    // StreamConsumer is wrapped in Arc<RwLock<>> to allow mutable operations like setting a filter
    // while still enabling shared read access for event consumption.
    consumer: Arc<RwLock<StreamConsumer>>,
    resource_manager: Arc<ResourceManager>, // Shared resource manager
    // BatchProcessor is optional. If None, events are handled directly.
    // If Some, events are routed through the batch processor.
    batch_processor: Option<Arc<BatchProcessor>>,
}

impl BitqueryClient {
    /// Creates a new `BitqueryClient` with the specified SDK configuration.
    ///
    /// # Arguments
    /// * `config`: The complete `SdkMainConfig` for the client.
    ///
    /// # Returns
    /// A `SdkResult<Self>` which is `Ok(BitqueryClient)` on successful initialization,
    /// or an `Error` if configuration is invalid or components fail to initialize.
    pub async fn new(config: SdkMainConfig) -> SdkResult<Self> {
        info!("Initializing BitqueryClient (Enhanced SDK Version)...");

        // Validate the overall configuration first.
        config.validate().map_err(|e| {
            error!("Invalid SDK configuration provided: {}", e);
            e // Return the config error
        })?;

        let arc_config = Arc::new(config);

        // Initialize ResourceManager first, as it's needed by StreamConsumer and BatchProcessor.
        let resource_manager = Arc::new(ResourceManager::new(arc_config.resources.clone()));

        // Create the underlying StreamConsumer.
        let stream_consumer = Self::initialize_kafka_consumer(arc_config.clone(), resource_manager.clone()).await?;

        info!("BitqueryClient initialized successfully.");
        Ok(Self {
            config: arc_config,
            consumer: Arc::new(RwLock::new(stream_consumer)),
            resource_manager,
            batch_processor: None, // Batch processing is opt-in.
        })
    }

    /// Creates a new `BitqueryClient` using `SdkMainConfig::default()`.
    /// Useful for quick starts if default settings (including hardcoded credentials/paths) are acceptable.
    pub async fn default() -> SdkResult<Self> {
        info!("Creating BitqueryClient with default SDK configuration.");
        Self::new(SdkMainConfig::default()).await
    }

    /// Applies an `EventFilter` to the `StreamConsumer`.
    /// Once set, only events matching the filter criteria will be yielded by `next_event()`.
    pub async fn set_filter(&self, filter: EventFilter) -> SdkResult<()> {
        let consumer_guard = self.consumer.write().await;
        consumer_guard.set_filter(filter).await;
        info!("Event filter has been set for the BitqueryClient.");
        Ok(())
    }

    /// Enables batch processing mode for this client.
    ///
    /// When batch processing is enabled:
    /// - Events fetched via `process_events` (or `next_event` if manually managed)
    ///   are routed to the internal `BatchProcessor`.
    /// - The `BatchProcessor` collects events into batches and processes them using
    ///   the provided `event_processor` with a pool of worker tasks.
    ///
    /// # Arguments
    /// * `event_processor`: An `Arc<dyn EventProcessor>` that defines the logic for handling
    ///                      individual events within the batches.
    pub async fn enable_batch_processing(&mut self, event_processor: Arc<dyn EventProcessor>) -> SdkResult<()> {
        if self.batch_processor.is_some() {
            warn!("Batch processing is already enabled. Re-enabling will replace the existing BatchProcessor setup.");
        }
        info!("Enabling batch processing mode for BitqueryClient...");

        let bp_instance = BatchProcessor::new(
            &self.config.processing, // Pass ProcessingConfig
            event_processor,         // User-provided event handler logic
            self.resource_manager.clone(), // Shared ResourceManager
            self.config.resources.max_queue_size, // Max items in BatchProcessor's input queue
        );

        self.batch_processor = Some(Arc::new(bp_instance));
        info!("Batch processing enabled. Workers (count: {}) started by BatchProcessor::new.", self.config.processing.parallel_workers);
        Ok(())
    }

    /// Starts the underlying Kafka consumer by subscribing to the configured topics.
    /// This method must be called before attempting to fetch events.
    pub async fn start(&self) -> SdkResult<()> {
        info!("BitqueryClient: Starting Kafka message consumption...");

        let consumer_guard = self.consumer.read().await; // Read lock for subscribe
        consumer_guard.subscribe(&self.config.kafka.topics)?;

        info!("BitqueryClient: Consumer subscribed successfully to topics: {:?}.", self.config.kafka.topics);

        // Note: In this revised design, BatchProcessor::new already starts its collector and worker tasks.
        // So, no explicit call to `batch_processor.start_workers()` is needed here,
        // differing from some earlier design thoughts or prompt interpretations.
        // This makes `enable_batch_processing` the single point for setting up and activating batching.
        if self.batch_processor.is_some() {
            info!("BitqueryClient: Batch processing is enabled; its workers are active.");
        }

        Ok(())
    }

    /// Fetches the next available `SolanaEvent` from the `StreamConsumer`.
    /// This method respects all configured filters, deduplication, and resource limits.
    /// It's suitable for manual event loop management by the application.
    pub async fn next_event(&self) -> SdkResult<Option<SolanaEvent>> {
        let consumer_guard = self.consumer.read().await; // Read lock for next_event
        consumer_guard.next_event().await // StreamConsumer::next_event handles internal logic
    }

    /// Enters a continuous loop to fetch and process events.
    ///
    /// Behavior depends on whether batch processing is enabled:
    /// - **Batch Mode**: Events from `next_event()` are sent to `BatchProcessor.add_event()`.
    ///   The `handler` closure provided to this method is NOT called.
    /// - **Direct Mode**: The `handler` closure is called synchronously for each event from `next_event()`.
    ///   If the `handler` needs to perform async operations, the application should use `next_event()`
    ///   in its own async loop and manage tasks.
    ///
    /// The loop continues until a critical error occurs in `next_event()`.
    ///
    /// # Arguments
    /// * `handler`: A synchronous closure `FnMut(SolanaEvent) -> SdkResult<()>` called for each event
    ///              ONLY if batch processing is NOT enabled.
    pub async fn process_events<F>(&self, mut handler: F) -> SdkResult<()>
    where
        F: FnMut(SolanaEvent) -> SdkResult<()>,
    {
        info!("BitqueryClient: Starting continuous event processing loop...");
        if self.batch_processor.is_some() {
            info!("Mode: Batch Processing. Events will be sent to BatchProcessor.");
        } else {
            info!("Mode: Direct Processing. Events will be passed to the provided handler.");
        }

        loop {
            // Global backpressure check before even trying to get an event.
            if self.resource_manager.is_backpressure_active() {
                warn!("BitqueryClient.process_events: Global backpressure active. Delaying event fetch.");
                // Use a slightly longer, configurable delay perhaps.
                tokio::time::sleep(self.config.retry.initial_delay.max(std::time::Duration::from_millis(200))).await;
                continue; // Re-check backpressure in the next iteration.
            }

            match self.next_event().await {
                Ok(Some(event)) => {
                    if let Some(ref active_batch_processor) = self.batch_processor {
                        // Batch Mode: Send event to the BatchProcessor.
                        if let Err(e) = active_batch_processor.add_event(event).await {
                            error!("BitqueryClient: Failed to add event to BatchProcessor: {}. This event may be lost.", e);
                            // This could indicate the BatchProcessor's input queue is full or closed.
                            // Potentially a fatal error for the loop, or retry with backoff.
                            // For now, log and continue, assuming BatchProcessor might recover.
                            // ResourceManager should ideally prevent this via consumer backpressure.
                        }
                    } else {
                        // Direct Mode: Call the synchronous handler.
                        if let Err(e) = handler(event) {
                            error!("BitqueryClient: Error in direct event handler: {}. Continuing event loop.", e);
                            // Application-level decision if this error is fatal for the loop.
                        }
                    }
                }
                Ok(None) => {
                    // `next_event()` returned None. This means no event is currently available
                    // (e.g., Kafka poll was empty, or consumer is internally delayed by backpressure).
                    // Brief sleep to avoid tight busy-looping.
                    tokio::time::sleep(std::time::Duration::from_millis(
                        self.config.retry.initial_delay.as_millis().min(50) as u64 // Use a small, configurable delay
                    )).await;
                }
                Err(e) => {
                    error!("BitqueryClient: Critical error fetching next event: {}. Terminating process_events loop.", e);
                    return Err(e); // Propagate critical errors (e.g., Kafka connection loss)
                }
            }
        }
    }

    /// Returns an `Arc`-cloned reference to the `ResourceManager`.
    /// Allows the application to inspect resource status or interact with it if needed.
    pub fn resource_manager(&self) -> Arc<ResourceManager> {
        self.resource_manager.clone()
    }

    /// Internal helper to create and configure the `rdkafka::StreamConsumer`.
    async fn initialize_kafka_consumer(
        config: Arc<SdkMainConfig>,
        resource_manager: Arc<ResourceManager>
    ) -> SdkResult<StreamConsumer> {
        debug!("BitqueryClient: Initializing Kafka consumer (rdkafka::StreamConsumer)...");

        let kafka_settings = &config.kafka;
        let kafka_brokers_str = kafka_settings.brokers.join(",");

        let mut rd_kafka_client_config = RdKafkaClientConfig::new();
        rd_kafka_client_config
            .set("bootstrap.servers", &kafka_brokers_str)
            .set("group.id", &kafka_settings.group_id)
            .set("security.protocol", "SASL_SSL") // Defaulting to SASL_SSL
            .set("sasl.mechanisms", "SCRAM-SHA-512") // Common mechanism
            .set("sasl.username", &kafka_settings.username)
            .set("sasl.password", &kafka_settings.password) // Password is now plain String
            .set("ssl.ca.location", &kafka_settings.ssl.ca_cert)
            .set("ssl.key.location", &kafka_settings.ssl.client_key)
            .set("ssl.certificate.location", &kafka_settings.ssl.client_cert)
            // Prompt's client.rs has "none" for endpoint identification.
            // This disables hostname verification. For production, "https" is strongly recommended.
            // User must be aware if using default config with "none".
            .set("ssl.endpoint.identification.algorithm", "none")
            .set("auto.offset.reset", &kafka_settings.auto_offset_reset)
            .set("enable.auto.commit", "false") // Manual offset commit handled by StreamConsumer
            .set("session.timeout.ms", kafka_settings.session_timeout.as_millis().to_string())
            // Standard Kafka consumer properties for flow control and fetching behavior:
            .set("max.partition.fetch.bytes", (1024 * 1024).to_string()) // 1MB per partition fetch
            .set("fetch.min.bytes", "1") // Respond as soon as any data is available
            .set("fetch.max.wait.ms", "500") // Max time to block for fetch.min.bytes
            .set("partition.assignment.strategy", &kafka_settings.partition_assignment_strategy);

        // `max.poll.records` from KafkaConfig is a higher-level concept not directly mapped
        // to a single rdkafka property for StreamConsumer. Flow control is managed by byte sizes
        // and queue limits in rdkafka. It's used conceptually in SDK's batching or queue limits.

        let rd_consumer: RdKafkaStreamConsumer = rd_kafka_client_config.create()
            .map_err(|e| Error::Kafka(e))?; // Convert rdkafka::error::KafkaError to Error::Kafka

        debug!("rdkafka::StreamConsumer created. Wrapping in SDK's StreamConsumer.");
        // Pass the cloned Arc<SdkMainConfig> to StreamConsumer
        Ok(StreamConsumer::new(rd_consumer, (*config).clone(), resource_manager))
    }

    /// Initiates a graceful shutdown of the client and its active components (like BatchProcessor).
    pub async fn shutdown(&self) {
        info!("BitqueryClient: Initiating graceful shutdown...");
        if let Some(ref active_batch_processor) = self.batch_processor {
            active_batch_processor.shutdown(); // Signal BatchProcessor to stop accepting new events and drain queues.
            // Add delay or join handles if BatchProcessor shutdown needs time to complete.
            // For now, just signaling.
        }
        // The StreamConsumer (and its underlying rdkafka consumer) typically shuts down
        // when it's dropped or when its consuming stream (e.g., in `process_events`) is broken.
        // No explicit rdkafka consumer shutdown method is usually called from high-level client.
        info!("BitqueryClient shutdown process signaled to components.");
    }
}
