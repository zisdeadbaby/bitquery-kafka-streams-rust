use crate::{
    config::ResourceLimits,
    error::{Error, Result as SdkResult}, // Using SdkResult consistently
};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;
use tokio::time::interval;
use tracing::{debug, info, warn};

/// `ResourceManager` is responsible for monitoring and controlling the consumption
/// of system resources, such as memory and the number of in-flight messages.
/// It helps prevent the SDK from overwhelming the system by applying backpressure
/// when predefined limits are approached or exceeded.
#[derive(Clone)] // `Clone` is implemented because `Arc<ResourceManager>` is passed around.
pub struct ResourceManager {
    config: Arc<ResourceLimits>, // Configuration for resource limits, Arc for shared ownership.

    // An estimation of current memory usage in bytes.
    // In a real-world scenario, this might be updated by a more sophisticated memory tracking mechanism.
    // For this example, it's largely illustrative or tied to `get_estimated_current_memory()`.
    current_memory_estimate_bytes: Arc<AtomicU64>,

    // Counter for messages currently being processed or queued within the SDK.
    messages_in_flight: Arc<AtomicUsize>,

    // A counter where >0 indicates that backpressure is active.
    // Multiple components can signal the need for backpressure by incrementing this.
    backpressure_active_signals: Arc<AtomicUsize>,
}

impl ResourceManager {
    /// Creates a new `ResourceManager` instance with the specified `ResourceLimits`.
    ///
    /// This also spawns a background monitoring task that periodically logs resource status
    /// and updates relevant metrics.
    pub fn new(config: ResourceLimits) -> Self {
        let manager = Self {
            config: Arc::new(config),
            current_memory_estimate_bytes: Arc::new(AtomicU64::new(0)), // Initialize estimate
            messages_in_flight: Arc::new(AtomicUsize::new(0)),
            backpressure_active_signals: Arc::new(AtomicUsize::new(0)),
        };

        // Spawn the background monitoring task.
        // Clone Arcs needed for the async task.
        let config_clone = manager.config.clone();
        let memory_estimate_clone = manager.current_memory_estimate_bytes.clone();
        let messages_in_flight_clone = manager.messages_in_flight.clone();
        let backpressure_signals_clone = manager.backpressure_active_signals.clone();

        tokio::spawn(async move {
            Self::monitoring_loop(
                config_clone,
                memory_estimate_clone,
                messages_in_flight_clone,
                backpressure_signals_clone,
            ).await;
        });

        info!("ResourceManager initialized with config: {:?}", manager.config);
        manager
    }

    /// Checks if sufficient resources are available for processing additional messages.
    ///
    /// This method evaluates the current number of in-flight messages and estimated
    /// memory usage against the configured limits. If limits are approached (based on
    /// `backpressure_threshold`) or exceeded, it activates a backpressure signal
    /// and returns an `Error::ResourceError`.
    ///
    /// # Returns
    /// - `Ok(())` if resources are deemed available.
    /// - `Err(Error::ResourceError)` if backpressure should be applied.
    pub async fn check_resources(&self) -> SdkResult<()> {
        // Check number of messages currently in flight
        let in_flight = self.messages_in_flight.load(AtomicOrdering::Relaxed);
        if in_flight >= self.config.max_messages_in_flight {
            let reason = format!("Max messages in flight reached ({} >= {})", in_flight, self.config.max_messages_in_flight);
            self.activate_backpressure_signal(&reason);
            return Err(Error::ResourceError(reason));
        }

        // Check estimated memory usage against the backpressure threshold
        let estimated_memory = self.get_estimated_current_memory();
        let memory_trigger_threshold = (self.config.max_memory_bytes as f64 * self.config.backpressure_threshold) as u64;

        if estimated_memory >= memory_trigger_threshold {
            let reason = format!("Estimated memory usage exceeds backpressure threshold ({} bytes >= {} bytes)", estimated_memory, memory_trigger_threshold);
            self.activate_backpressure_signal(&reason);
            return Err(Error::ResourceError(reason));
        }

        // All checks passed, resources are available.
        Ok(())
    }

    /// Call this method when a batch of `batch_size` messages begins processing.
    /// It increments the count of in-flight messages.
    pub async fn start_processing(&self, batch_size: usize) {
        let old_in_flight = self.messages_in_flight.fetch_add(batch_size, AtomicOrdering::Relaxed);
        debug!("Started processing batch of size {}. Messages in flight: {} -> {}.",
            batch_size, old_in_flight, old_in_flight + batch_size);
        // Optionally, update memory estimate more directly here if a model exists.
    }

    /// Call this method when processing of a batch of `batch_size` messages is completed.
    /// It decrements the count of in-flight messages and checks if backpressure can be eased.
    pub async fn complete_processing(&self, batch_size: usize) {
        let old_in_flight = self.messages_in_flight.fetch_sub(batch_size, AtomicOrdering::Relaxed);
        debug!("Completed processing batch of size {}. Messages in flight: {} -> {}.",
            batch_size, old_in_flight, old_in_flight.saturating_sub(batch_size)); // Use saturating_sub for display logic

        // Check if conditions allow for deactivating a backpressure signal
        let current_in_flight = self.messages_in_flight.load(AtomicOrdering::Relaxed);
        let estimated_memory = self.get_estimated_current_memory();

        // Define more lenient thresholds for deactivating backpressure (e.g., 60-70% of the activation threshold)
        let memory_deactivation_point = (self.config.max_memory_bytes as f64 * self.config.backpressure_threshold * 0.70) as u64;
        let messages_deactivation_point = (self.config.max_messages_in_flight as f64 * 0.70) as usize;

        if current_in_flight < messages_deactivation_point && estimated_memory < memory_deactivation_point {
            if self.is_backpressure_active() { // Only attempt to deactivate if it was active
                self.deactivate_backpressure_signal("Resource usage dropped below deactivation thresholds.");
            }
        }
    }

    /// Provides an estimation of the current memory usage in bytes.
    ///
    /// **Note:** This is a simplified estimation based on in-flight messages.
    /// For accurate production monitoring, integrate with actual memory profiling tools
    /// (e.g., `jemalloc` stats if `jemallocator` feature is used, or OS-level metrics).
    fn get_estimated_current_memory(&self) -> u64 {
        let in_flight_count = self.messages_in_flight.load(AtomicOrdering::Relaxed) as u64;
        // Average size per message in memory (highly dependent on event data complexity).
        // This is a very rough estimate and should be tuned based on observation.
        let avg_message_ram_footprint_bytes: u64 = 20 * 1024; // Example: 20KB per event/message

        let estimated_dynamic_usage = in_flight_count * avg_message_ram_footprint_bytes;
        self.current_memory_estimate_bytes.store(estimated_dynamic_usage, AtomicOrdering::Relaxed);
        estimated_dynamic_usage
    }

    /// Activates a backpressure signal.
    /// If this is the first signal to activate backpressure, a warning is logged.
    fn activate_backpressure_signal(&self, reason: &str) {
        // `fetch_add` returns the *previous* value. If it was 0, this is the first signal.
        if self.backpressure_active_signals.fetch_add(1, AtomicOrdering::Relaxed) == 0 {
            warn!("ResourceManager: Backpressure ACTIVATED. Reason: {}", reason);
            #[cfg(feature = "metrics")]
            sdk_metrics::gauge!("bitquery_sdk_backpressure_status", 1.0); // 1.0 for active
        }
    }

    /// Deactivates one backpressure signal.
    /// If this was the last active signal, an info message is logged.
    fn deactivate_backpressure_signal(&self, reason: &str) {
        // `fetch_sub` returns the *previous* value. If it was 1, this means count will become 0.
        if self.backpressure_active_signals.fetch_sub(1, AtomicOrdering::Relaxed) == 1 {
            info!("ResourceManager: Backpressure DEACTIVATED. Reason: {}", reason);
            #[cfg(feature = "metrics")]
            sdk_metrics::gauge!("bitquery_sdk_backpressure_status", 0.0); // 0.0 for inactive
        }
    }

    /// Returns `true` if any backpressure signal is currently active.
    pub fn is_backpressure_active(&self) -> bool {
        self.backpressure_active_signals.load(AtomicOrdering::Relaxed) > 0
    }

    /// The background monitoring loop task.
    /// Periodically logs current resource estimates and updates metrics.
    async fn monitoring_loop(
        config: Arc<ResourceLimits>,
        current_memory_estimate: Arc<AtomicU64>, // This is the one updated by get_estimated_current_memory
        messages_in_flight: Arc<AtomicUsize>,
        backpressure_active_signals: Arc<AtomicUsize>,
    ) {
        let mut tick_interval = interval(config.memory_check_interval);
        info!("ResourceManager monitoring loop started. Check interval: {:?}.", config.memory_check_interval);

        loop {
            tick_interval.tick().await;

            let mem_val = current_memory_estimate.load(AtomicOrdering::Relaxed); // Reads the estimate set by get_estimated_current_memory
            let in_flight_val = messages_in_flight.load(AtomicOrdering::Relaxed);
            let backpressure_status = backpressure_active_signals.load(AtomicOrdering::Relaxed) > 0;

            #[cfg(feature = "metrics")]
            {
                sdk_metrics::gauge!("bitquery_sdk_estimated_memory_usage_bytes", mem_val as f64);
                sdk_metrics::gauge!("bitquery_sdk_messages_in_flight", in_flight_val as f64);
                // Backpressure status metric is updated in activate/deactivate methods.
            }

            debug!(
                "ResourceManager Status: Est. Memory: {} bytes, In-Flight Msgs: {}, Backpressure: {}",
                mem_val,
                in_flight_val,
                if backpressure_status { "ACTIVE" } else { "INACTIVE" }
            );
        }
    }
}

// Note: The original prompt's ResourceManager had a `last_check: Arc<RwLock<Instant>>`.
// This field (`_last_check_time` here) is not actively used in the provided logic for ResourceManager.
// It might have been intended for more complex backpressure timeout/reset logic, but the current
// implementation activates/deactivates based on thresholds and signal counts.
// It's kept here as `_last_check_time` to align with the field being present, but marked as unused.
// The `current_memory: Arc<AtomicU64>` field was also present. I've named it
// `current_memory_estimate_bytes` and it's updated by `get_estimated_current_memory` and read by the monitor.
