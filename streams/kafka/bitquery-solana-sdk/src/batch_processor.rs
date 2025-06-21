use crate::{
    config::ProcessingConfig,
    error::{Error, Result as SdkResult}, // Using SdkResult consistently
    events::SolanaEvent,
    processors::EventProcessor,
    resource_manager::ResourceManager,
    utils::metrics::{self as sdk_metrics, Timer as SdkTimer}, // Alias Timer to avoid conflict
};
use async_channel::{bounded, Receiver as AsyncReceiver, Sender as AsyncSender}; // Alias for clarity
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{timeout, Instant as TokioInstant};
use tracing::{debug, info, warn, error};

/// `BatchProcessor` orchestrates the collection of individual `SolanaEvent`s into
/// batches and processes these batches concurrently using a pool of worker tasks.
/// This is designed to enhance throughput for event processing, especially when
/// individual event processing involves I/O or is computationally intensive.
pub struct BatchProcessor {
    batch_size: usize,
    batch_timeout: Duration,
    processor: Arc<dyn EventProcessor>, // The user-defined processor for individual events
    resource_manager: Arc<ResourceManager>,
    // Channel for incoming individual events to be batched.
    event_collector_sender: AsyncSender<SolanaEvent>,
    // Channel for dispatching formed batches to worker tasks.
    batch_dispatch_sender: AsyncSender<Vec<SolanaEvent>>,
    num_workers: usize,
    // Handles for spawned tasks (collector, workers) for graceful shutdown if needed.
    // For simplicity, not explicitly storing handles in this version, relying on channel closure.
}

impl BatchProcessor {
    /// Creates and initializes a new `BatchProcessor`.
    ///
    /// This constructor sets up internal channels and spawns the event collector task
    /// and batch worker tasks.
    ///
    /// # Arguments
    /// * `processing_config`: SDK's processing configuration, providing batch size, timeout, and worker count.
    /// * `processor`: An `Arc`-wrapped `EventProcessor` that defines how individual events are handled.
    /// * `resource_manager`: Shared `ResourceManager` to enforce resource limits and backpressure.
    /// * `queue_size`: Capacity for internal event and batch channels.
    pub fn new(
        processing_config: &ProcessingConfig,
        processor: Arc<dyn EventProcessor>,
        resource_manager: Arc<ResourceManager>,
        queue_size: usize, // Overall capacity for incoming events before batching
    ) -> Self {
        // Channel for individual events -> event collector
        let (event_collector_sender, event_collector_receiver) = bounded(queue_size);

        // Channel for formed batches -> worker pool
        // Batch queue can be smaller, e.g., a few batches per worker.
        let worker_batch_queue_capacity = (processing_config.parallel_workers * 2).max(10);
        let (batch_dispatch_sender, batch_dispatch_receiver) = bounded(worker_batch_queue_capacity);

        let batch_proc = Self {
            batch_size: processing_config.batch_size,
            batch_timeout: processing_config.batch_timeout,
            processor,
            resource_manager,
            event_collector_sender,
            batch_dispatch_sender,
            num_workers: processing_config.parallel_workers,
        };

        // Spawn the event collector task.
        batch_proc.spawn_event_collector_task(event_collector_receiver);

        // Spawn the batch worker tasks.
        batch_proc.spawn_batch_worker_tasks(batch_dispatch_receiver);

        batch_proc
    }

    /// Spawns a dedicated asynchronous task to collect incoming `SolanaEvent`s,
    /// form them into batches, and send these batches to worker tasks.
    fn spawn_event_collector_task(&self, event_receiver: AsyncReceiver<SolanaEvent>) {
        let batch_sender = self.batch_dispatch_sender.clone();
        let max_batch_size = self.batch_size;
        let batch_creation_timeout = self.batch_timeout;
        // Not cloning resource_manager here as collector primarily reacts to channel full / backpressure from consumer.

        tokio::spawn(async move {
            info!("Event collector task started. Target batch size: {}, Batch timeout: {:?}.",
                max_batch_size, batch_creation_timeout);

            let mut current_batch = Vec::with_capacity(max_batch_size);
            let mut last_event_received_or_batch_sent_time = TokioInstant::now();

            loop {
                let time_since_last_activity = last_event_received_or_batch_sent_time.elapsed();
                let remaining_timeout = batch_creation_timeout.saturating_sub(time_since_last_activity);

                // Prefer select! for handling multiple async operations (receive, timeout)
                tokio::select! {
                    biased; // Process event reception first if available

                    // Attempt to receive an event
                    event_result = event_receiver.recv() => {
                        match event_result {
                            Ok(event) => {
                                current_batch.push(event);
                                last_event_received_or_batch_sent_time = TokioInstant::now(); // Reset timer on activity
                                if current_batch.len() >= max_batch_size {
                                    debug!("Event collector: Batch full ({} events). Sending.", current_batch.len());
                                    if batch_sender.send(current_batch).await.is_err() {
                                        error!("Event collector: Failed to send full batch to workers (channel closed). Shutting down collector.");
                                        break; // Exit loop if batch channel is closed
                                    }
                                    current_batch = Vec::with_capacity(max_batch_size); // Reset for next batch
                                    // last_event_received_or_batch_sent_time reset by sending
                                }
                            }
                            Err(_) => { // async_channel::RecvError means channel is empty and closed
                                info!("Event collector: Event channel closed. Sending final batch (if any) and shutting down.");
                                if !current_batch.is_empty() {
                                    if batch_sender.send(current_batch).await.is_err() {
                                        error!("Event collector: Failed to send final batch (channel closed).");
                                    }
                                }
                                break; // Exit loop
                            }
                        }
                    }

                    // Handle batch timeout
                    _ = tokio::time::sleep(remaining_timeout), if !current_batch.is_empty() && remaining_timeout > Duration::ZERO => {
                        debug!("Event collector: Batch timeout reached. Sending partial batch of {} events.", current_batch.len());
                        if batch_sender.send(current_batch).await.is_err() {
                            error!("Event collector: Failed to send timed-out batch to workers (channel closed). Shutting down collector.");
                            break; // Exit loop
                        }
                        current_batch = Vec::with_capacity(max_batch_size);
                        last_event_received_or_batch_sent_time = TokioInstant::now();
                    }
                }
            }
            info!("Event collector task stopped.");
        });
    }

    /// Spawns a pool of worker tasks that listen for batches of events and process them.
    fn spawn_batch_worker_tasks(&self, batch_receiver: AsyncReceiver<Vec<SolanaEvent>>) {
        info!("Spawning {} batch processing worker tasks.", self.num_workers);
        for worker_id in 0..self.num_workers {
            let worker_batch_receiver = batch_receiver.clone(); // Clone receiver for each worker
            let processor_arc = self.processor.clone();
            let resource_manager_arc = self.resource_manager.clone();

            tokio::spawn(async move {
                Self::batch_worker_loop(worker_id, worker_batch_receiver, processor_arc, resource_manager_arc).await;
            });
        }
    }

    /// Adds a single `SolanaEvent` to the batching queue.
    ///
    /// This method is called by the `BitqueryClient` when batch processing is enabled.
    /// It sends the event to the event collector task.
    ///
    /// # Returns
    /// `Ok(())` if the event was successfully queued.
    /// `Err(Error::ChannelError)` if the event queue is full or closed.
    pub async fn add_event(&self, event: SolanaEvent) -> SdkResult<()> {
        // Note: The prompt's version of BatchProcessor.add_event had a complex batch filling logic.
        // This was moved to the dedicated `event_collector_task`.
        // `add_event` should now simply send the individual event to the collector.

        // Backpressure check: if the event_collector_sender channel is full, it implies
        // the system is overloaded. The `send` will await if there's space, or fail if closed.
        // A `try_send` could be used for immediate feedback on full channel.
        if self.event_collector_sender.is_full() {
            warn!("BatchProcessor input channel (event_collector_sender) is full. This may indicate system overload.");
            // Depending on desired behavior, could return Error::Processing("Queue full")
            // or let send().await handle backpressure by waiting.
            // For now, let send await. It will error if channel is closed.
        }

        self.event_collector_sender.send(event).await
            .map_err(|e| Error::ChannelError(format!("Failed to send event to collector task: {}", e)))
    }

    /// The main processing loop for a single batch worker task.
    async fn batch_worker_loop(
        worker_id: usize,
        batch_receiver: AsyncReceiver<Vec<SolanaEvent>>,
        processor: Arc<dyn EventProcessor>,
        resource_manager: Arc<ResourceManager>,
    ) {
        info!("Batch worker #{} started.", worker_id);

        loop {
            match batch_receiver.recv().await {
                Ok(event_batch) => {
                    if event_batch.is_empty() { continue; }

                    let batch_len = event_batch.len();
                    debug!("Batch worker #{}: Received batch of {} events.", worker_id, batch_len);

                    let _batch_processing_timer = SdkTimer::new("batch_worker_processing_batch");

                    // Inform ResourceManager that processing for this batch size is starting.
                    resource_manager.start_processing(batch_len).await;

                    let mut successfully_processed_count = 0;
                    let mut failed_to_process_count = 0;

                    for event in event_batch {
                        // The event filter in `StreamConsumer` or `BitqueryClient` should handle initial filtering.
                        // `processor.should_process` is a final check specific to the processor's logic.
                        if processor.should_process(&event) {
                            match processor.process(&event).await {
                                Ok(_) => successfully_processed_count += 1,
                                Err(e) => {
                                    warn!("Batch worker #{}: Error processing event (Sig: {}): {}",
                                        worker_id, event.signature(), e);
                                    failed_to_process_count += 1;
                                }
                            }
                        }
                    }

                    // Inform ResourceManager that processing for this batch is complete.
                    resource_manager.complete_processing(batch_len).await;

                    // Record metrics for the processed batch
                    sdk_metrics::record_batch_processed(batch_len, _batch_processing_timer.start.elapsed().as_millis() as f64);
                    // Optionally, record event-level success/failure if meaningful for the batch
                    if successfully_processed_count > 0 {
                        // Could use a generic type or specific type if all events in batch are same
                        // sdk_metrics::record_event_processed("batch_processed_event", true);
                    }
                    if failed_to_process_count > 0 {
                        // sdk_metrics::record_event_processed("batch_processed_event", false);
                    }

                    info!(
                        "Batch worker #{}: Processed batch. Total: {}, Succeeded: {}, Failed: {}, Duration: {:.2}ms",
                        worker_id,
                        batch_len,
                        successfully_processed_count,
                        failed_to_process_count,
                        _batch_processing_timer.start.elapsed().as_secs_f64() * 1000.0
                    );
                }
                Err(_) => { // async_channel::RecvError indicates channel is empty and closed.
                    info!("Batch worker #{}: Batch dispatch channel closed. Shutting down worker.", worker_id);
                    break; // Exit the loop, terminating the worker task.
                }
            }
        }
        warn!("Batch worker #{} stopped.", worker_id);
    }

    /// Signals the `BatchProcessor` to shut down gracefully.
    /// Closes the input event channel, allowing existing events to be processed
    /// and worker tasks to terminate once their queues are empty.
    pub fn shutdown(&self) {
        info!("Initiating shutdown of BatchProcessor...");
        // Closing the event_collector_sender will cause the collector task to finish
        // after processing any remaining events in its buffer.
        self.event_collector_sender.close();

        // The collector task, upon exiting, will drop its batch_dispatch_sender.
        // When all senders to batch_dispatch_sender are dropped (i.e., only collector has one),
        // the batch_dispatch_receiver in workers will eventually detect closure.
        // Alternatively, can also explicitly close batch_dispatch_sender if BatchProcessor owns it exclusively
        // and collector doesn't hold a clone that needs to be dropped first.
        // self.batch_dispatch_sender.close(); // If appropriate.
        debug!("BatchProcessor event input channel closed. Workers will shut down once queues are drained.");
    }
}

// Manual impl of Send/Sync if needed, though Arc and async_channel types are Send/Sync.
// The `processor: Arc<dyn EventProcessor>` requires `EventProcessor` to be `'static`.
// (Already added 'static to EventProcessor trait in processors/mod.rs)
// This should make BatchProcessor Send + Sync by default if all fields are.
// Adding explicit unsafe impls is generally a last resort if compiler can't deduce.
// Default derive should work if all members are Send/Sync.
// Let's assume the compiler can derive it. If not, this is where `unsafe impl` would go.
// unsafe impl Send for BatchProcessor {}
// unsafe impl Sync for BatchProcessor {}
