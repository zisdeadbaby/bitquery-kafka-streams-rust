use crate::{
    config::ProcessingConfig, // To get batch_size, batch_timeout from main config if needed
    error::{Error, Result},
    events::SolanaEvent,
    processors::EventProcessor,
    resource_manager::ResourceManager,
    utils::metrics, // For Timer and other metrics
};
use async_channel::{bounded, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
// use tokio::time::{interval, timeout, Instant as TokioInstant}; // Not used directly from prompt
use tokio::time::{timeout, Instant as TokioInstant}; // Corrected imports based on usage
use tracing::{debug, info, warn, error};


/// `BatchProcessor` is responsible for collecting events into batches and
/// processing these batches using worker tasks. This approach can improve
/// efficiency for I/O-bound or CPU-bound tasks that benefit from chunking.
pub struct BatchProcessor {
    batch_size: usize,
    batch_timeout: Duration,
    processor: Arc<dyn EventProcessor>,
    resource_manager: Arc<ResourceManager>,
    // Channel for sending individual events to be batched by a collector task.
    event_sender: Sender<SolanaEvent>,
    // Channel for sending completed batches to worker tasks.
    batch_sender: Sender<Vec<SolanaEvent>>,
    // Number of active workers.
    num_workers: usize,
}

impl BatchProcessor {
    /// Creates a new `BatchProcessor`.
    ///
    /// # Arguments
    /// * `processing_config`: Configuration for batch size, timeout, and number of workers.
    /// * `processor`: The `EventProcessor` implementation that will handle each event within a batch.
    /// * `resource_manager`: Shared `ResourceManager` for controlling load.
    /// * `queue_size`: The capacity of internal channels.
    pub fn new(
        processing_config: &ProcessingConfig, // Pass reference to processing config
        processor: Arc<dyn EventProcessor>,
        resource_manager: Arc<ResourceManager>,
        queue_size: usize,
    ) -> Self {
        // Channel for individual events to be collected into batches
        let (event_sender, event_receiver) = bounded(queue_size);
        // Channel for dispatching full batches to worker pool
        let (batch_sender, batch_receiver) = bounded(queue_size.min(processing_config.parallel_workers * 2)); // Keep batch queue smaller

        let batch_processor = Self {
            batch_size: processing_config.batch_size,
            batch_timeout: processing_config.batch_timeout,
            processor,
            resource_manager,
            event_sender,
            batch_sender,
            num_workers: processing_config.parallel_workers,
        };

        // Start the event collector task
        batch_processor.start_event_collector(event_receiver);

        // Start worker tasks
        batch_processor.start_batch_workers(batch_receiver);

        batch_processor
    }

    /// Starts a dedicated asynchronous task that collects individual events
    /// from `event_receiver`, forms them into batches, and sends these batches
    /// to `batch_sender`.
    fn start_event_collector(&self, event_receiver: Receiver<SolanaEvent>) {
        let batch_sender_clone = self.batch_sender.clone();
        let max_batch_size = self.batch_size;
        let batch_creation_timeout = self.batch_timeout;
        let resource_manager_clone = self.resource_manager.clone();

        tokio::spawn(async move {
            info!("Event collector task started. Batch size: {}, Timeout: {:?}", max_batch_size, batch_creation_timeout);
            let mut current_batch = Vec::with_capacity(max_batch_size);
            let mut last_batch_send_time = TokioInstant::now();

            loop {
                // Check if backpressure is active before trying to receive more events
                if resource_manager_clone.is_backpressure_active() {
                    tokio::time::sleep(Duration::from_millis(100)).await; // Backoff
                    // Also, if a batch has items, send it to relieve pressure
                    if !current_batch.is_empty() {
                        debug!("Sending partial batch due to backpressure. Size: {}", current_batch.len());
                        if batch_sender_clone.send(current_batch).await.is_err() {
                            error!("Event collector failed to send batch to workers: channel closed.");
                            break;
                        }
                        current_batch = Vec::with_capacity(max_batch_size);
                        last_batch_send_time = TokioInstant::now();
                    }
                    continue;
                }

                // Calculate remaining time for batch timeout
                let elapsed_since_last_send = last_batch_send_time.elapsed();
                let remaining_timeout = if elapsed_since_last_send >= batch_creation_timeout {
                    Duration::ZERO // Timeout already passed
                } else {
                    batch_creation_timeout - elapsed_since_last_send
                };

                match timeout(remaining_timeout, event_receiver.recv()).await {
                    Ok(Ok(event)) => { // Received an event
                        current_batch.push(event);
                        if current_batch.len() >= max_batch_size {
                            if batch_sender_clone.send(current_batch).await.is_err() {
                                error!("Event collector failed to send full batch: channel closed.");
                                break;
                            }
                            current_batch = Vec::with_capacity(max_batch_size);
                            last_batch_send_time = TokioInstant::now();
                        }
                    }
                    Ok(Err(_)) => { // Event channel closed
                        info!("Event collector: event channel closed. Sending final batch.");
                        if !current_batch.is_empty() {
                            let _ = batch_sender_clone.send(current_batch).await;
                        }
                        break;
                    }
                    Err(_) => { // Timeout occurred
                        if !current_batch.is_empty() {
                            debug!("Event collector: batch timeout. Sending batch of size {}.", current_batch.len());
                            if batch_sender_clone.send(current_batch).await.is_err() {
                                error!("Event collector failed to send timed-out batch: channel closed.");
                                break;
                            }
                            current_batch = Vec::with_capacity(max_batch_size);
                            last_batch_send_time = TokioInstant::now();
                        } else {
                            // Timeout but no events, just reset timer for next potential event
                            last_batch_send_time = TokioInstant::now();
                        }
                    }
                }
            }
            info!("Event collector task stopped.");
        });
    }

    /// Starts the batch processing worker tasks.
    /// Workers listen on `batch_receiver` for batches of events and process them.
    fn start_batch_workers(&self, batch_receiver: Receiver<Vec<SolanaEvent>>) {
        info!("Starting {} batch processing workers.", self.num_workers);
        for worker_id in 0..self.num_workers {
            let receiver_clone = batch_receiver.clone();
            let processor_clone = self.processor.clone();
            let resource_manager_clone = self.resource_manager.clone();

            tokio::spawn(async move {
                Self::worker_loop(worker_id, receiver_clone, processor_clone, resource_manager_clone).await;
            });
        }
    }

    /// Adds a single event to the batch processor.
    /// This event will be collected into a batch and processed by one of the workers.
    ///
    /// # Returns
    /// `Ok(())` if the event was successfully queued.
    /// `Err(Error::Processing)` if the event queue is full or another queuing error occurs.
    pub async fn add_event(&self, event: SolanaEvent) -> Result<()> {
        // Resource manager check is primarily for the *consumer* before it even calls add_event.
        // Here, we check if the event_sender channel itself is full, which implies downstream is too slow.
        if self.event_sender.is_full() {
            warn!("BatchProcessor event channel is full. Applying backpressure.");
            // This indicates that the event collector or workers are not keeping up.
            // The ResourceManager should ideally prevent this by slowing down the consumer.
            // However, as a last resort, we can return an error here.
            return Err(Error::Processing("Batch processor event queue is full.".to_string()));
        }

        self.event_sender.send(event).await
            .map_err(|e| Error::Processing(format!("Failed to queue event for batching: {}", e)))
    }

    /// The main loop for a batch processing worker.
    ///
    /// Waits for batches on the `receiver`, processes each event in the batch
    /// using the provided `processor`, and manages resource accounting.
    async fn worker_loop(
        worker_id: usize,
        receiver: Receiver<Vec<SolanaEvent>>,
        processor: Arc<dyn EventProcessor>,
        resource_manager: Arc<ResourceManager>,
    ) {
        info!("Batch worker {} started.", worker_id);

        loop {
            match receiver.recv().await {
                Ok(batch) => {
                    let batch_size = batch.len();
                    if batch_size == 0 { continue; }

                    debug!("Worker {} received batch of {} events.", worker_id, batch_size);

                    let _timer = metrics::Timer::new("batch_processing_worker");
                    resource_manager.start_processing(batch_size).await;

                    let mut success_count = 0;
                    let mut failure_count = 0;

                    for event in batch {
                        // Note: Event-specific filtering (if any) should ideally happen *before* batching,
                        // or at least before calling processor.process if it's expensive.
                        // The `should_process` check here is a final gate.
                        if processor.should_process(&event) {
                            match processor.process(&event).await {
                                Ok(_) => success_count += 1,
                                Err(e) => {
                                    warn!("Worker {} failed to process event (Sig: {}): {}", worker_id, event.signature(), e);
                                    failure_count += 1;
                                }
                            }
                        }
                        // No else needed, if should_process is false, it's just skipped.
                    }

                    resource_manager.complete_processing(batch_size).await;

                    metrics::record_batch_processed(batch_size, _timer.start.elapsed().as_millis() as f64);
                    if failure_count > 0 {
                         metrics::record_event_processed("unknown_batch_event", false); // Generic failure for batch
                    }


                    info!(
                        "Worker {} processed batch: Total: {}, Succeeded: {}, Failed: {}, Duration: {:.2}ms",
                        worker_id,
                        batch_size,
                        success_count,
                        failure_count,
                        _timer.start.elapsed().as_secs_f64() * 1000.0
                    );
                }
                Err(_) => { // Channel closed
                    info!("Batch worker {}: channel closed. Shutting down.", worker_id);
                    break;
                }
            }
        }
        warn!("Batch worker {} stopped.", worker_id);
    }

    /// Closes the input channel to the batch processor, signaling no more events will be added.
    /// This allows the event collector and workers to drain their queues and shut down gracefully.
    pub fn shutdown(&self) {
        info!("Shutting down BatchProcessor. Closing event sender channel.");
        self.event_sender.close();
        self.batch_sender.close(); // Also close batch sender to signal workers
    }
}

// Ensure BatchProcessor is Send + Sync if it's to be Arc-wrapped and shared
// The channels (async_channel) are Send + Sync. Processor and ResourceManager are Arc'd.
// So, BatchProcessor should be Send + Sync.
unsafe impl Send for BatchProcessor {}
unsafe impl Sync for BatchProcessor {}
