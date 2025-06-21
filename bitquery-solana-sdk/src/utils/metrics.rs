//! Metrics utilities using the `metrics` facade.
//!
//! This module provides helper structs (like `Timer`) and functions to record
//! metrics throughout the SDK. These metrics can then be collected and exported
//! by a compatible exporter (e.g., `metrics-exporter-prometheus`) initialized
//! by the application using the SDK.
//!
//! All metric recording is conditional on the "metrics" feature flag of the SDK.

// `once_cell` is not directly used in this version of metrics.rs, but often useful for global state.
// use once_cell::sync::Lazy;
use std::time::Instant;

/// A RAII timer for measuring the duration of a scope.
///
/// When a `Timer` instance is created, it records the current time. When it is
/// dropped (goes out of scope), it calculates the elapsed duration and records
/// it as a histogram metric. The metric name is dynamically generated using the
/// `name` provided on creation, prefixed with "bitquery_sdk_".
///
/// Example:
/// ```ignore
/// {
///     let _timer = Timer::new("my_critical_section");
///     // Code to measure...
/// } // Metric "bitquery_sdk_my_critical_section_duration_seconds" is recorded here.
/// ```
pub struct Timer {
    start: Instant,
    name: &'static str,
}

impl Timer {
    /// Creates a new `Timer` and records the start time.
    ///
    /// # Arguments
    /// * `name`: A static string used as part of the metric name.
    pub fn new(name: &'static str) -> Self {
        Self {
            start: Instant::now(),
            name,
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        #[cfg(feature = "metrics")]
        {
            // Uses the `metrics::histogram!` macro from the `metrics` crate.
            // An exporter (like Prometheus) needs to be set up in the application
            // for these metrics to be collected and exposed.
            metrics::histogram!(
                format!("bitquery_sdk_{}_duration_seconds", self.name),
                duration.as_secs_f64()
                // Additional labels can be added here if needed: "key" => "value".
            );
        }
    }
}

/// Records that an event has been processed, along with its status (success/failure).
///
/// Metric name: `bitquery_sdk_events_processed_total` (counter)
/// Labels:
///   - `type`: The type of the event (e.g., "transaction", "dex_trade").
///   - `status`: "success" or "failure".
pub fn record_event_processed(event_type: &str, success: bool) {
    #[cfg(feature = "metrics")]
    {
        metrics::counter!(
            "bitquery_sdk_events_processed_total",
            1, // Increment by 1
            "type" => event_type.to_string(), // Label for event type
            "status" => if success { "success" } else { "failure" }.to_string() // Label for status
        );
    }
    #[cfg(not(feature = "metrics"))]
    {
        // Ensure variables are "used" to prevent compiler warnings when metrics are off.
        let _ = event_type;
        let _ = success;
    }
}

/// Records metrics related to the processing of a batch of events.
///
/// Metrics recorded:
///   - `bitquery_sdk_batch_size`: Histogram of the number of events in processed batches.
///   - `bitquery_sdk_batch_duration_ms`: Histogram of the time taken to process batches, in milliseconds.
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

// Note on exporting metrics:
// This utility module focuses on *recording* metrics using the `metrics` facade.
// The actual *exporting* of these metrics (e.g., via a Prometheus HTTP endpoint)
// is handled by an exporter crate (like `metrics-exporter-prometheus`) and should be
// initialized in the main application binary (e.g., in `lib.rs`'s `init_with_config`
// or the application's `main.rs`).

#[cfg(test)]
mod tests {
    use super::*;
    // For these tests, we mostly ensure the functions are callable and don't panic.
    // Verifying actual metric emission usually requires setting up a test exporter,
    // which can be complex for simple unit tests.

    #[test]
    fn timer_creates_and_drops() {
        // Test that Timer can be instantiated and dropped.
        // If `metrics` feature is enabled, `metrics::histogram!` will be called on drop.
        {
            let _timer = Timer::new("test_scope");
            // Simulate some work that takes time.
            std::thread::sleep(std::time::Duration::from_micros(10));
        } // _timer is dropped here.
          // No direct assertion possible here without an exporter.
    }

    #[test]
    fn record_event_processed_callable() {
        // Test that the function can be called with different parameters.
        record_event_processed("test_event_type", true);
        record_event_processed("another_event_type", false);
        // No direct assertion.
    }

    #[test]
    fn record_batch_processed_callable() {
        // Test that the function can be called.
        record_batch_processed(100, 123.45);
        record_batch_processed(50, 67.89);
        // No direct assertion.
    }
}
