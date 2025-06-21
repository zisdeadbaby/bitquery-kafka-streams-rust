use crate::{
    config::ResourceLimits,
    error::{Error, Result},
    utils::metrics, // For metrics calls
};
use parking_lot::RwLock as ParkingRwLock; // Using parking_lot::RwLock for performance
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{debug, info, warn};

/// `ResourceManager` monitors and controls resource usage (e.g., memory, in-flight messages)
/// to prevent the system from being overwhelmed. It can trigger backpressure if limits
/// are approached.
#[derive(Clone)] // Clone is needed for passing Arc<ResourceManager> around
pub struct ResourceManager {
    config: Arc<ResourceLimits>, // Use Arc for config if it's also shared or large
    // Estimated current memory usage in bytes.
    // In a real scenario, this would be fed by a proper memory tracking mechanism.
    // For this example, it might be estimated or not used directly if `get_current_memory` is mocked.
    _current_memory_estimate_bytes: Arc<AtomicU64>, // Renamed as it's an estimate

    // Number of messages currently being processed or in queues.
    messages_in_flight: Arc<AtomicUsize>,

    // Timestamp of the last resource check that might have triggered backpressure.
    // Using parking_lot::RwLock for potentially frequent reads.
    _last_check_time: Arc<ParkingRwLock<Instant>>, // Renamed as it's an internal detail

    // Counter for active backpressure signals. >0 means backpressure is active.
    // This allows multiple components to signal backpressure without complex state.
    backpressure_signals: Arc<AtomicUsize>,
}

impl ResourceManager {
    /// Creates a new `ResourceManager` with the given configuration.
    ///
    /// It also spawns a background monitoring task that periodically logs resource status
    /// and updates metrics.
    pub fn new(config: ResourceLimits) -> Self {
        let manager = Self {
            config: Arc::new(config),
            _current_memory_estimate_bytes: Arc::new(AtomicU64::new(0)),
            messages_in_flight: Arc::new(AtomicUsize::new(0)),
            _last_check_time: Arc::new(ParkingRwLock::new(Instant::now())),
            backpressure_signals: Arc::new(AtomicUsize::new(0)),
        };

        // Clone necessary Arcs for the monitoring task
        let config_clone = manager.config.clone();
        let messages_in_flight_clone = manager.messages_in_flight.clone();
        let backpressure_signals_clone = manager.backpressure_signals.clone();
        // let current_memory_clone = manager._current_memory_estimate_bytes.clone(); // If used by get_current_memory_actual

        tokio::spawn(async move {
            Self::monitoring_loop(
                config_clone,
                messages_in_flight_clone,
                backpressure_signals_clone,
                // current_memory_clone, // Pass if needed
            ).await;
        });

        manager
    }

    /// Checks if resources are available for processing more messages.
    ///
    /// This method evaluates current messages in flight and estimated memory usage
    /// against configured limits. If limits are exceeded or the backpressure threshold
    /// is met, it activates backpressure and returns an error.
    ///
    /// # Returns
    /// `Ok(())` if resources are available.
    /// `Err(Error::Processing)` if backpressure should be applied.
    pub async fn check_resources(&self) -> Result<()> {
        // Check messages in flight
        let in_flight_count = self.messages_in_flight.load(AtomicOrdering::Relaxed);
        if in_flight_count >= self.config.max_messages_in_flight {
            self.activate_backpressure("max_messages_in_flight reached");
            return Err(Error::Processing(format!(
                "Resource limit: Too many messages in flight ({} >= {})",
                in_flight_count, self.config.max_messages_in_flight
            )));
        }

        // Check (estimated) memory usage
        let current_mem_usage = self.get_estimated_current_memory();
        let memory_threshold_bytes = (self.config.max_memory_bytes as f64 * self.config.backpressure_threshold) as u64;

        if current_mem_usage >= memory_threshold_bytes {
            self.activate_backpressure("memory_threshold reached");
            return Err(Error::Processing(format!(
                "Resource limit: Memory usage threshold reached ({} bytes >= {} bytes)",
                current_mem_usage, memory_threshold_bytes
            )));
        }

        Ok(())
    }

    /// Call when starting to process a batch of `batch_size` messages.
    /// Increments the count of messages in flight.
    pub async fn start_processing(&self, batch_size: usize) {
        self.messages_in_flight.fetch_add(batch_size, AtomicOrdering::Relaxed);
        // Potentially update _current_memory_estimate_bytes if using a model based on this
    }

    /// Call when finished processing a batch of `batch_size` messages.
    /// Decrements the count of messages in flight and checks if backpressure can be deactivated.
    pub async fn complete_processing(&self, batch_size: usize) {
        self.messages_in_flight.fetch_sub(batch_size, AtomicOrdering::Relaxed);

        // Check if we can deactivate backpressure
        let in_flight_count = self.messages_in_flight.load(AtomicOrdering::Relaxed);
        let current_mem_usage = self.get_estimated_current_memory();

        // Deactivate if well below thresholds, e.g., 50-60% of limits
        let memory_deactivation_threshold = (self.config.max_memory_bytes as f64 * (self.config.backpressure_threshold * 0.75)) as u64; // e.g., 75% of threshold
        let messages_deactivation_threshold = (self.config.max_messages_in_flight as f64 * 0.75) as usize;

        if in_flight_count < messages_deactivation_threshold && current_mem_usage < memory_deactivation_threshold {
            if self.is_backpressure_active() { // Only try to deactivate if it was active
                self.deactivate_backpressure("resources below deactivation threshold");
            }
        }
    }

    /// Provides an estimated current memory usage.
    /// In a real production system, this would integrate with actual memory measurement tools
    /// or a more sophisticated memory model. Here, it's a simple estimate.
    fn get_estimated_current_memory(&self) -> u64 {
        // For this example, estimate based on messages in flight.
        // This is a placeholder for a more accurate memory measurement.
        let in_flight_count = self.messages_in_flight.load(AtomicOrdering::Relaxed) as u64;
        let average_message_size_estimate = 10 * 1024; // Assume 10KB per message on average in memory (very rough)

        // Also consider fixed overheads or other memory consumers if known.
        let base_memory_usage = 50 * 1024 * 1024; // Base 50MB for the application itself (example)

        base_memory_usage + (in_flight_count * average_message_size_estimate)
    }

    /// Activates backpressure, logging the reason.
    fn activate_backpressure(&self, reason: &str) {
        // fetch_add returns the *previous* value. If it was 0, this is the first signal.
        if self.backpressure_signals.fetch_add(1, AtomicOrdering::Relaxed) == 0 {
            warn!("Backpressure ACTIVATED. Reason: {}", reason);
            #[cfg(feature = "metrics")]
            metrics::gauge!("bitquery_sdk_backpressure_status", 1.0); // 1.0 for active
        }
    }

    /// Deactivates one signal of backpressure. If all signals are gone, logs deactivation.
    fn deactivate_backpressure(&self, reason: &str) {
        // fetch_sub returns the *previous* value. If it was 1, this is the last signal being removed.
        if self.backpressure_signals.fetch_sub(1, AtomicOrdering::Relaxed) == 1 {
            info!("Backpressure DEACTIVATED. Reason: {}", reason);
            #[cfg(feature = "metrics")]
            metrics::gauge!("bitquery_sdk_backpressure_status", 0.0); // 0.0 for inactive
        }
    }

    /// Checks if backpressure is currently active (i.e., any component has signaled it).
    pub fn is_backpressure_active(&self) -> bool {
        self.backpressure_signals.load(AtomicOrdering::Relaxed) > 0
    }

    /// Background task for periodically monitoring and logging resource usage.
    async fn monitoring_loop(
        config: Arc<ResourceLimits>,
        messages_in_flight: Arc<AtomicUsize>,
        backpressure_signals: Arc<AtomicUsize>,
        // current_memory_actual: Arc<AtomicU64>, // If using a more direct memory measure
    ) {
        let mut ticker = interval(config.memory_check_interval);

        loop {
            ticker.tick().await;

            // This is where you'd get actual memory if not estimating:
            // let mem_usage = get_process_memory_usage(); // Platform-specific or via a crate
            // current_memory_actual.store(mem_usage, AtomicOrdering::Relaxed);

            // For estimation based on in-flight messages:
            let in_flight_count = messages_in_flight.load(AtomicOrdering::Relaxed);
            let average_message_size_estimate = 10 * 1024;
            let estimated_mem_usage = (in_flight_count * average_message_size_estimate) as u64;

            let backpressure_active_status = backpressure_signals.load(AtomicOrdering::Relaxed) > 0;

            #[cfg(feature = "metrics")]
            {
                // If using the estimated memory for metrics:
                metrics::gauge!("bitquery_sdk_estimated_memory_usage_bytes", estimated_mem_usage as f64);
                metrics::gauge!("bitquery_sdk_messages_in_flight", in_flight_count as f64);
                metrics::gauge!("bitquery_sdk_backpressure_status", if backpressure_active_status { 1.0 } else { 0.0 });

            }

            debug!(
                "Resource Monitor: Est. Memory Usage: {} bytes, Messages In-Flight: {}, Backpressure Active: {}",
                estimated_mem_usage,
                in_flight_count,
                backpressure_active_status
            );

            // Example of proactive backpressure based on monitoring loop (optional)
            // if estimated_mem_usage >= (config.max_memory_bytes as f64 * config.backpressure_threshold) as u64 {
            //     if backpressure_signals.fetch_add(1, AtomicOrdering::Relaxed) == 0 {
            //         warn!("MONITOR: Backpressure ACTIVATED due to high estimated memory.");
            //     }
            // }
        }
    }
}

// Note: The original prompt had `current_memory: Arc<AtomicU64>`.
// If this was meant to be updated by an external source (e.g. a proper memory profiler),
// then `get_current_memory` should read from it.
// The current `get_estimated_current_memory` calculates based on messages_in_flight.
// I've kept `_current_memory_estimate_bytes` as a field to align with the idea but made it internal.
// The `Clone` derive for `ResourceManager` is fine as all its fields are `Arc` or copyable (`ResourceLimits` is Arc'd).
