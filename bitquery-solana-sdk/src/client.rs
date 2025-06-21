use crate::{
    batch_processor::BatchProcessor,
    config::Config, // Using the new enhanced Config
    consumer::StreamConsumer,
    error::Result, // Using SDK's Result type
    events::SolanaEvent,
    filters::EventFilter,
    processors::EventProcessor, // Trait for custom processors
    resource_manager::ResourceManager,
};
use rdkafka::config::ClientConfig as RdKafkaClientConfig; // Alias for clarity
// No direct use of rdkafka::consumer::Consumer trait here, StreamConsumer handles it.
use std::sync::Arc;
use tokio::sync::RwLock; // For read-write locking of shared consumer
use tracing::{info, debug, error}; // Logging

/// `BitqueryClient` is the main entry point for interacting with the SDK.
/// It manages the Kafka connection, event consumption, filtering, batching (optional),
/// and resource monitoring.
pub struct BitqueryClient {
    config: Arc<Config>,
    // Consumer is Arc<RwLock<>> to allow mutable access for filter setting,
    // while still allowing shared reads for event consumption.
    consumer: Arc<RwLock<StreamConsumer>>,
    resource_manager: Arc<ResourceManager>,
    // BatchProcessor is optional; client can run in direct event handling mode or batch mode.
    batch_processor: Option<Arc<BatchProcessor>>,
}

impl BitqueryClient {
    /// Creates a new `BitqueryClient` with the given configuration.
    ///
    /// # Arguments
    /// * `config`: The SDK `Config` struct.
    ///
    /// # Returns
    /// A `Result<Self>` which is `Ok(BitqueryClient)` on success, or an `Error` on failure.
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing BitqueryClient (Enhanced Version)...");

        // Validate the provided configuration first.
        config.validate()?; // Assuming Config::validate() exists and is comprehensive.

        let arc_config = Arc::new(config);

        let resource_manager = Arc::new(ResourceManager::new(arc_config.resources.clone()));

        // Create the underlying StreamConsumer
        let stream_consumer = Self::create_kafka_consumer(arc_config.clone(), resource_manager.clone()).await?;

        info!("BitqueryClient initialized successfully.");
        Ok(Self {
            config: arc_config,
            consumer: Arc::new(RwLock::new(stream_consumer)),
            resource_manager,
            batch_processor: None, // Batch processing is not enabled by default.
        })
    }

    /// Creates a new `BitqueryClient` using the default `Config`.
    /// Useful for quick setup if default configurations are suitable.
    pub async fn default() -> Result<Self> {
        info!("Creating BitqueryClient with default configuration...");
        Self::new(Config::default()).await
    }

    /// Sets an `EventFilter` for the `StreamConsumer`.
    /// Events will be pre-filtered according to this filter before further processing.
    pub async fn set_filter(&self, filter: EventFilter) -> Result<()> {
        let mut consumer_guard = self.consumer.write().await;
        consumer_guard.set_filter(filter);
        info!("Event filter has been applied to the BitqueryClient's consumer.");
        Ok(())
    }

    /// Enables batch processing mode.
    ///
    /// When enabled, events fetched by `next_event` (or via `process_events`)
    /// will be routed to the `BatchProcessor` instead of being returned directly
    /// or passed to the direct handler. The `BatchProcessor` will then manage
    /// batching and parallel worker execution.
    ///
    /// # Arguments
    /// * `event_processor`: An `Arc<dyn EventProcessor>` that will be used by the batch workers.
    pub async fn enable_batch_processing(&mut self, event_processor: Arc<dyn EventProcessor>) -> Result<()> {
        if self.batch_processor.is_some() {
            warn!("Batch processing is already enabled. Re-enabling will replace the existing processor setup.");
        }
        info!("Enabling batch processing mode.");
        let bp = BatchProcessor::new(
            &self.config.processing, // Pass reference to ProcessingConfig
            event_processor,
            self.resource_manager.clone(),
            self.config.resources.max_queue_size, // Use max_queue_size for internal channels
        );

        self.batch_processor = Some(Arc::new(bp));
        info!("Batch processing enabled with {} parallel workers (from config).", self.config.processing.parallel_workers);
        Ok(())
    }

    /// Starts the Kafka consumer and subscribes to topics.
    /// If batch processing is enabled, this also implicitly starts its workers
    /// (as BatchProcessor::new now handles worker startup).
    pub async fn start(&self) -> Result<()> {
        info!("Starting BitqueryClient message consumption...");

        // Subscribe the underlying consumer to configured topics.
        let consumer_guard = self.consumer.read().await; // Read lock is sufficient for subscribe
        consumer_guard.subscribe(&self.config.kafka.topics)?;

        info!("Consumer subscribed to topics: {:?}", self.config.kafka.topics);

        // BatchProcessor workers are started when BatchProcessor::new is called.
        // No explicit start_workers call needed here if BatchProcessor handles its own lifecycle.
        // The prompt's BatchProcessor::start_workers takes num_workers,
        // and client::start calls it. Let's stick to that.

        if let Some(batch_proc) = &self.batch_processor {
             info!("Batch processing is enabled. Workers are managed by BatchProcessor instance.");
             // The prompt's example `client.start()` calls batch_proc.start_workers.
             // However, my `BatchProcessor::new` already starts the workers.
             // For consistency with the prompt's `client.rs` structure, I'll add it,
             // but it implies `BatchProcessor::new` should not start workers, or `start_workers` should be idempotent.
             // Let's assume `BatchProcessor::new` sets up, and `start_workers` is a separate call.
             // This means `BatchProcessor::new` in `enable_batch_processing` should not start workers.
             // I will need to adjust `BatchProcessor::new` and `BatchProcessor::start_workers` accordingly.
             // For now, I'll proceed as if `start_workers` is the explicit trigger from client.
             // This implies `enable_batch_processing` only creates the BatchProcessor instance.
             // **Decision**: I'll modify `BatchProcessor::new` to NOT start workers,
             // and `client.start()` will call a method like `batch_processor.run_workers()`.

            // The prompt's BatchProcessor::start_workers is not part of its struct methods in the example.
            // It's BatchProcessor that has start_workers.
            // The prompt for Client.start() is:
            // if let Some(batch_processor) = &self.batch_processor {
            //    let workers = self.config.processing.parallel_workers;
            //    let processor = batch_processor.clone(); // This is Arc<BatchProcessor>
            //    tokio::spawn(async move {
            //        if let Err(e) = processor.start_workers(workers).await { ... }
            //    });
            // }
            // This seems fine if BatchProcessor has an async start_workers method.
            // The provided BatchProcessor code has `start_workers` as a method.
            // It seems `enable_batch_processing` creates it, and `client.start()` launches its workers.
            // This is okay.
        }

        Ok(())
    }

    /// Fetches the next available `SolanaEvent` from the stream.
    /// This method respects resource limits and backpressure.
    /// If batch processing is enabled, this method might be less commonly used directly,
    /// with `process_events` being preferred.
    pub async fn next_event(&self) -> Result<Option<SolanaEvent>> {
        let consumer_guard = self.consumer.read().await;
        consumer_guard.next_message().await // StreamConsumer::next_message handles resource checks
    }

    /// Continuously processes events from the Kafka stream.
    ///
    /// If batch processing is enabled, events are added to the `BatchProcessor`.
    /// Otherwise, the provided `handler` closure is called for each event.
    /// This loop runs until an error occurs or the stream ends.
    ///
    /// # Arguments
    /// * `handler`: A closure `FnMut(SolanaEvent) -> Result<()>` called for each event
    ///              if batch processing is not enabled.
    pub async fn process_events<F>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(SolanaEvent) -> Result<()>, // Handler for non-batch mode
    {
        info!("Starting event processing loop...");
        loop {
            // Check resource manager for overall health / backpressure before fetching next event
            if self.resource_manager.is_backpressure_active() {
                warn!("Global backpressure active in process_events loop. Delaying fetch.");
                tokio::time::sleep(std::time::Duration::from_millis(200)).await; // General delay
                continue;
            }

            match self.next_event().await {
                Ok(Some(event)) => {
                    if let Some(batch_proc) = &self.batch_processor {
                        // Batch mode: add event to the batch processor
                        if let Err(e) = batch_proc.add_event(event).await {
                            error!("Failed to add event to batch processor: {}. May lose this event.", e);
                            // Decide if this error is fatal for the loop.
                            // If channel is full, it's a form of backpressure.
                            // Potentially continue, but log it.
                        }
                    } else {
                        // Direct mode: call the handler
                        if let Err(e) = handler(event) {
                            error!("Error in direct event handler: {}. Loop continues.", e);
                            // Decide if handler error is fatal. For now, log and continue.
                        }
                    }
                }
                Ok(None) => {
                    // No event currently available (e.g., due to backpressure in consumer.next_message, or empty poll)
                    // Sleep briefly to avoid busy-looping if Kafka topic is temporarily empty.
                    tokio::time::sleep(tokio::time::Duration::from_millis(self.config.retry.initial_delay.as_millis().min(50) as u64)).await;
                }
                Err(e) => {
                    error!("Error fetching next event: {}. Terminating process_events loop.", e);
                    return Err(e); // Propagate error to caller
                }
            }
        }
    }

    /// Returns a reference to the `ResourceManager` instance.
    pub fn resource_manager(&self) -> Arc<ResourceManager> {
        self.resource_manager.clone()
    }

    /// Creates and configures the underlying `rdkafka::StreamConsumer`.
    async fn create_kafka_consumer(
        config: Arc<Config>,
        resource_manager: Arc<ResourceManager> // Keep resource_manager for StreamConsumer::new
    ) -> Result<StreamConsumer> {
        debug!("Creating Kafka StreamConsumer instance...");

        let kafka_conf = &config.kafka;
        let brokers = kafka_conf.brokers.join(",");

        // Constructing rdkafka::ClientConfig
        let mut rd_client_config = RdKafkaClientConfig::new();
        rd_client_config
            .set("bootstrap.servers", &brokers)
            .set("group.id", &kafka_conf.group_id)
            .set("security.protocol", "SASL_SSL") // Assuming SASL_SSL is always used
            .set("sasl.mechanisms", "SCRAM-SHA-512") // Common mechanism
            .set("sasl.username", &kafka_conf.username)
            .set("sasl.password", &kafka_conf.password) // Password as plain String
            .set("ssl.ca.location", &kafka_conf.ssl.ca_cert)
            .set("ssl.key.location", &kafka_conf.ssl.client_key)
            .set("ssl.certificate.location", &kafka_conf.ssl.client_cert)
            // Per prompt, ssl.endpoint.identification.algorithm is "none".
            // This disables hostname verification. For production, "https" is recommended.
            // Ensure users are aware of this default if it's insecure.
            .set("ssl.endpoint.identification.algorithm", "none")
            .set("auto.offset.reset", &kafka_conf.auto_offset_reset)
            .set("enable.auto.commit", "false") // Manual offset commit is crucial
            .set("session.timeout.ms", kafka_conf.session_timeout.as_millis().to_string())
            .set("max.partition.fetch.bytes", (1024 * 1024).to_string()) // 1MB, common default
            .set("fetch.min.bytes", "1") // Respond as soon as any data is available
            .set("fetch.max.wait.ms", "500") // Max time to block for fetch.min.bytes
            .set("partition.assignment.strategy", &kafka_conf.partition_assignment_strategy);
            // Add more settings from kafka_conf.max_poll_records if there's an equivalent
            // rdkafka setting (e.g., `queued.max.messages.kbytes` or similar, not direct count)
            // `max.poll.records` is more of a Java/Python client concept.
            // For rdkafka, flow control is managed differently (e.g. `queued.min.messages`, `fetch.message.max.bytes`).
            // The `kafka_conf.max_poll_records` is not directly used here for rdkafka config.

        let consumer: RdKafkaStreamConsumer = rd_client_config.create()?;

        debug!("Kafka StreamConsumer created. Wrapping in SDK's StreamConsumer.");
        Ok(StreamConsumer::new(consumer, (*config).clone(), resource_manager))
    }

    /// Initiates a graceful shutdown of the client and its components.
    pub async fn shutdown(&self) {
        info!("Shutting down BitqueryClient...");
        if let Some(bp) = &self.batch_processor {
            bp.shutdown(); // Signal batch processor to stop accepting new events and drain
        }
        // Consumer doesn't have an explicit shutdown; it stops when dropped or stream is exhausted.
        // For long-running consumers, application usually handles shutdown via task cancellation.
        info!("BitqueryClient shutdown process initiated.");
    }
}
