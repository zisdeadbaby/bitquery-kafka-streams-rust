use std::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex; // Use tokio's Mutex for async environments
use tracing::warn;

/// Implements a circuit breaker pattern to prevent repeated calls to a failing service.
///
/// The circuit breaker monitors failures and, if a threshold is reached, "opens"
/// to stop further requests for a configured timeout period. After the timeout,
/// it enters a "half-open" state, allowing a limited number of test requests.
/// If these succeed, it closes; otherwise, it re-opens.
///
/// This implementation is simplified: it opens on threshold, and after a timeout,
/// it resets (closes) automatically on the next check if the timeout has passed.
/// A more advanced version might implement a specific "half-open" state.
pub struct CircuitBreaker {
    failure_count: AtomicU32,
    is_open: AtomicBool,
    threshold: u32,
    reset_timeout: Duration,
    // last_failure_or_opened_time stores when the breaker opened or the last failure that kept it open.
    last_failure_or_opened_time: Arc<TokioMutex<Option<Instant>>>,
}

impl CircuitBreaker {
    /// Creates a new `CircuitBreaker`.
    ///
    /// # Arguments
    /// * `threshold`: The number of consecutive failures required to open the circuit.
    /// * `reset_timeout`: The duration the circuit remains open before attempting to reset.
    pub fn new(threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            is_open: AtomicBool::new(false),
            threshold,
            reset_timeout,
            last_failure_or_opened_time: Arc::new(TokioMutex::new(None)),
        }
    }

    /// Records a successful operation.
    ///
    /// If the circuit breaker was open or half-open, this might contribute to closing it.
    /// In this simplified version, it resets the failure count and closes the circuit.
    pub async fn record_success(&self) {
        let was_open = self.is_open.swap(false, Ordering::SeqCst);
        if was_open {
            warn!("Circuit breaker is now CLOSED due to success.");
        }
        self.failure_count.store(0, Ordering::SeqCst);
        let mut last_failure_time = self.last_failure_or_opened_time.lock().await;
        *last_failure_time = None; // Clear the time when it was opened
    }

    /// Records a failed operation.
    ///
    /// This increments the failure count. If the count reaches the threshold,
    /// the circuit opens.
    pub async fn record_failure(&self) {
        if self.is_open.load(Ordering::SeqCst) {
            // If already open, just ensure the last_failure_or_opened_time is recent.
            // This can prevent premature closing if checks are sparse.
            let mut last_failure_time = self.last_failure_or_opened_time.lock().await;
            if last_failure_time.is_some() { // Should always be Some if open
                 *last_failure_time = Some(Instant::now());
            }
            return; // Already open, no need to increment further or re-evaluate threshold
        }

        let current_failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;

        if current_failures >= self.threshold {
            let already_open = self.is_open.swap(true, Ordering::SeqCst);
            if !already_open { // Only log and set time if it just transitioned to open
                warn!(
                    "Circuit breaker is now OPEN after {} consecutive failures. Will remain open for at least {:?}.",
                    current_failures, self.reset_timeout
                );
                let mut last_failure_time = self.last_failure_or_opened_time.lock().await;
                *last_failure_time = Some(Instant::now());
            }
        }
    }

    /// Checks if the circuit is currently open.
    ///
    /// If the circuit is open, it also checks if the `reset_timeout` has elapsed.
    /// If the timeout has passed, it will transition the circuit to a closed state
    /// (or half-open in more complex implementations) allowing operations to be retried.
    pub async fn is_open(&self) -> bool {
        if !self.is_open.load(Ordering::SeqCst) {
            return false; // It's closed
        }

        // If it thinks it's open, check the timeout
        let mut last_failure_time_guard = self.last_failure_or_opened_time.lock().await;
        match *last_failure_time_guard {
            Some(opened_at) => {
                if opened_at.elapsed() >= self.reset_timeout {
                    // Timeout has passed. Reset the breaker (transition to closed).
                    self.is_open.store(false, Ordering::SeqCst);
                    self.failure_count.store(0, Ordering::SeqCst);
                    *last_failure_time_guard = None;
                    warn!("Circuit breaker reset timeout elapsed. Now attempting to CLOSE.");
                    false // Now considered closed for this check
                } else {
                    true // Still open, timeout not yet elapsed
                }
            }
            None => {
                // This state (is_open=true, last_failure_time=None) should ideally not happen.
                // If it does, err on the side of caution and assume it's open, but try to reset.
                warn!("Circuit breaker in inconsistent state (open but no open time). Attempting to reset.");
                self.is_open.store(false, Ordering::SeqCst); // Attempt to reset
                self.failure_count.store(0, Ordering::SeqCst);
                false
            }
        }
    }

    /// Gets the configured reset timeout.
    pub fn get_reset_timeout(&self) -> Duration {
        self.reset_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn circuit_breaker_opens_after_threshold() {
        let threshold = 3;
        let timeout = Duration::from_millis(100);
        let cb = Arc::new(CircuitBreaker::new(threshold, timeout));

        assert!(!cb.is_open().await, "Should be closed initially");

        cb.record_failure().await;
        assert!(!cb.is_open().await, "Should be closed after 1 failure");

        cb.record_failure().await;
        assert!(!cb.is_open().await, "Should be closed after 2 failures");

        cb.record_failure().await; // 3rd failure, should open
        assert!(cb.is_open().await, "Should be open after 3 failures");
    }

    #[tokio::test]
    async fn circuit_breaker_resets_after_timeout() {
        let threshold = 1;
        let timeout = Duration::from_millis(50); // Short timeout for testing
        let cb = Arc::new(CircuitBreaker::new(threshold, timeout));

        cb.record_failure().await; // Opens the circuit
        assert!(cb.is_open().await, "Should be open immediately after failure meeting threshold");

        sleep(timeout + Duration::from_millis(10)).await; // Wait for timeout to elapse

        assert!(!cb.is_open().await, "Should be closed again after timeout");
    }

    #[tokio::test]
    async fn circuit_breaker_success_resets_failures() {
        let threshold = 3;
        let timeout = Duration::from_millis(100);
        let cb = Arc::new(CircuitBreaker::new(threshold, timeout));

        cb.record_failure().await;
        cb.record_failure().await;
        assert!(!cb.is_open().await, "Still closed after 2 failures");
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 2);

        cb.record_success().await;
        assert!(!cb.is_open().await, "Should remain closed after success");
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 0, "Failure count should reset on success");

        // Try failing again, it should take full threshold to open
        cb.record_failure().await;
        cb.record_failure().await;
        assert!(!cb.is_open().await, "Still closed after 2 new failures");
        cb.record_failure().await;
        assert!(cb.is_open().await, "Should open after 3 new failures");
    }

    #[tokio::test]
    async fn circuit_breaker_stays_open_during_timeout() {
        let threshold = 1;
        let timeout = Duration::from_millis(100);
        let cb = Arc::new(CircuitBreaker::new(threshold, timeout));

        cb.record_failure().await; // Opens
        assert!(cb.is_open().await, "Open after 1st failure");

        sleep(timeout / 2).await; // Wait for half the timeout
        assert!(cb.is_open().await, "Should still be open during timeout period");

        // Recording more failures while open should not change its open state immediately,
        // but might refresh the "opened_at" time if implemented that way (current one does).
        cb.record_failure().await;
        assert!(cb.is_open().await, "Still open after another failure");
    }

    #[tokio::test]
    async fn circuit_breaker_success_closes_open_breaker() {
        let threshold = 1;
        let timeout = Duration::from_secs(1); // Long timeout
        let cb = Arc::new(CircuitBreaker::new(threshold, timeout));

        cb.record_failure().await; // Opens
        assert!(cb.is_open().await, "Should be open");

        cb.record_success().await; // Record success while it's open
        assert!(!cb.is_open().await, "Should be closed immediately after a success");
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 0, "Failure count should be 0 after success");
    }
}
