//! Metrics utilities using the `metrics` facade.
//!
//! This module provides helper structs (like `Timer`) and functions to record
//! metrics throughout the SDK. These metrics can then be collected and exported
//! by a compatible exporter (e.g., `metrics-exporter-prometheus`) initialized
//! by the application using the SDK (typically in `lib.rs` or `main.rs`).
//!
//! All metric recording within these utilities is conditional on the "metrics"
//! feature flag of the SDK being enabled during compilation.

// `once_cell` might be used for lazy static initialization of metric descriptions if needed,
// but the `metrics` crate macros handle registration implicitly.
// use once_cell::sync::Lazy;
use std::time::Instant;

/// A RAII (Resource Acquisition Is Initialization) timer for measuring the duration of a scope.
///
/// When a `Timer` instance is created, it captures the current `Instant`.
/// When the instance is dropped (goes out of scope), it calculates the elapsed
/// duration and records it as a histogram metric via the `metrics::histogram!` macro.
/// The metric name is dynamically generated using the `name` provided on creation,
/// prefixed with "bitquery_sdk_", and suffixed with "_duration_seconds".
///
/// # Example
/// ```ignore
/// // Assuming `metrics` feature is enabled and an exporter is set up.
/// use bitquery_solana_sdk::utils::metrics::Timer;
///
/// fn my_function() {
///     let _timer = Timer::new("my_function_execution"); // Metric: bitquery_sdk_my_function_execution_duration_seconds
///     // Code whose duration is to be measured...
/// } // Timer drops here, metric is recorded.
/// ```
pub struct Timer {
    /// The `Instant` when the timer was started.
    pub start: Instant, // Made public for direct access if needed, e.g. in BatchProcessor
    name: &'static str,
}

impl Timer {
    /// Creates a new `Timer` and records the current time as its start point.
    ///
    /// # Arguments
    /// * `name`: A static string slice used as a component in the generated metric name.
    ///           It should be descriptive of the scope being timed.
    pub fn new(name: &'static str) -> Self {
        Self {
            start: Instant::now(),
            name,
        }
    }
}

impl Drop for Timer {
    /// Called when the `Timer` instance goes out of scope.
    /// Calculates the elapsed time since creation and records it as a histogram metric
    /// if the "metrics" feature is enabled.
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        #[cfg(feature = "metrics")]
        {
            // The `metrics::histogram!` macro registers and records the value.
            // An actual metrics backend (like Prometheus via metrics-exporter-prometheus)
            // must be initialized in the application for these metrics to be collected.
            metrics::histogram!(
                format!("bitquery_sdk_{}_duration_seconds", self.name), // Metric name
                duration.as_secs_f64() // Value (duration in seconds)
                // Labels can be added here as key-value pairs if needed: "key" => "value".
            );
        }
    }
}

/// Records that a specific type of event has been processed, noting its success or failure.
///
/// This function increments a counter metric named `bitquery_sdk_events_processed_total`.
///
/// # Labels
/// - `type`: The type of the event (e.g., "transaction", "dex_trade"), passed as `event_type`.
/// - `status`: A string indicating "success" or "failure".
///
/// # Arguments
/// * `event_type`: A string slice describing the type of event processed.
/// * `success`: A boolean indicating whether the processing was successful.
pub fn record_event_processed(event_type: &str, success: bool) {
    #[cfg(feature = "metrics")]
    {
        metrics::counter!(
            "bitquery_sdk_events_processed_total", // Metric name
            1, // Increment value
            "type" => event_type.to_string(), // Label for event type
            "status" => if success { "success" } else { "failure" }.to_string() // Label for processing status
        );
    }
    #[cfg(not(feature = "metrics"))]
    {
        // Ensure variables are marked as "used" to prevent compiler warnings
        // when the "metrics" feature is disabled (and thus the macro call is compiled out).
        let _ = event_type;
        let _ = success;
    }
}

/// Records metrics related to the processing of a batch of events.
///
/// Metrics recorded:
///   - `bitquery_sdk_batch_size`: A histogram of the number of events in processed batches.
///   - `bitquery_sdk_batch_duration_ms`: A histogram of the time taken (in milliseconds)
///     to process entire batches.
///
/// # Arguments
/// * `size`: The number of events in the processed batch.
/// * `duration_ms`: The total duration in milliseconds taken to process the batch.
pub fn record_batch_processed(size: usize, duration_ms: f64) {
    #[cfg(feature = "metrics")]
    {
        metrics::histogram!("bitquery_sdk_batch_size", size as f64);
        metrics::histogram!("bitquery_sdk_batch_duration_ms", duration_ms);
    }
    #[cfg(not(feature = "metrics"))]
    {
        let _ = size;
        let _ = duration_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Import items from the parent module (metrics.rs)

    // These tests primarily ensure that the metric recording functions are callable
    // and do not panic, both when the "metrics" feature is enabled and when it's disabled.
    // Verifying the actual emission and content of metrics typically requires setting up
    // a test metrics recorder/exporter, which is beyond the scope of simple unit tests
    // for these utility functions.

    #[test]
    fn timer_instantiates_and_drops_without_panic() {
        // This test verifies that a Timer can be created and will execute its Drop logic.
        // If the "metrics" feature is enabled, this will involve a call to metrics::histogram!.
        {
            let _test_timer = Timer::new("test_operation_scope");
            // Simulate a small amount of work.
            std::thread::sleep(std::time::Duration::from_nanos(100));
        } // _test_timer is dropped here. Metric recording logic (if feature-enabled) runs.
          // No explicit assertion is made about metric values, just that it runs.
    }

    #[test]
    fn record_event_processed_is_callable() {
        // Test that record_event_processed can be called with various inputs.
        record_event_processed("sample_event", true);  // Simulate a successful event
        record_event_processed("another_sample_event", false); // Simulate a failed event
        // No explicit assertion; ensures the function calls don't panic.
    }

    #[test]
    fn record_batch_processed_is_callable() {
        // Test that record_batch_processed can be called with various inputs.
        record_batch_processed(150, 75.5); // Example: 150 events in 75.5 ms
        record_batch_processed(0, 0.1);   // Example: An empty batch processed quickly
        // No explicit assertion; ensures the function calls don't panic.
    }
}
