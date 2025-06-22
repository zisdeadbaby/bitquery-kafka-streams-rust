use crate::error::{Error, Result};
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{warn, debug};

/// Configuration for retry operations
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: usize,
    /// Initial delay before the first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries (cap for exponential backoff)
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (delay *= multiplier each retry)
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

/// Provides a strategy for retrying operations with exponential backoff.
///
/// This utility can wrap asynchronous operations, automatically retrying them
/// upon failure according to the configured parameters (max retries, delays, multiplier).
pub struct RetryStrategy {
    max_retries: usize,
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
}

impl RetryStrategy {
    /// Creates a new `RetryStrategy` from the given `RetryConfig`.
    pub fn from_config(config: &RetryConfig) -> Self {
        Self {
            max_retries: config.max_retries,
            initial_delay: config.initial_delay,
            max_delay: config.max_delay,
            multiplier: config.multiplier,
        }
    }

    /// Executes an asynchronous operation and retries it on failure.
    ///
    /// The operation `f` is a closure that returns a `Future`. If the future
    /// resolves to `Ok(T)`, its value is returned. If it resolves to `Err(E)`,
    /// the operation is retried after a delay. The delay increases exponentially
    /// with each attempt, up to `max_delay`.
    ///
    /// # Type Parameters
    /// * `F`: The type of the closure that produces the future.
    /// * `Fut`: The type of the future returned by the closure.
    /// * `T`: The success type of the future's output.
    /// * `E`: The error type of the future's output, which must be convertible into `SdkError`.
    ///
    /// # Arguments
    /// * `operation_name`: A descriptive name for the operation being retried (for logging).
    /// * `f`: A closure that, when called, returns a new future for the operation.
    ///
    /// # Returns
    /// A `Result<T>` which is `Ok(T)` if the operation succeeded within the retry limits,
    /// or an `Error::RetryExhausted` if all attempts failed.
    pub async fn retry<F, Fut, T, E>(&self, operation_name: &str, mut f: F) -> Result<T>
    where
        F: FnMut() -> Fut, // FnMut to allow capturing and modifying mutable state if necessary
        Fut: Future<Output = std::result::Result<T, E>>,
        E: Into<Error> + std::fmt::Debug, // Ensure error can be converted and logged
    {
        let mut attempts = 0;
        let mut current_delay = self.initial_delay;

        loop {
            attempts += 1;
            debug!(
                "Attempt {} for operation '{}'. Current delay: {:?}",
                attempts, operation_name, if attempts > 1 { Some(current_delay) } else { None }
            );

            match f().await {
                Ok(result) => {
                    if attempts > 1 {
                        debug!(
                            "Operation '{}' succeeded after {} attempts.",
                            operation_name, attempts
                        );
                    }
                    return Ok(result);
                }
                Err(e) => {
                    let error: Error = e.into();
                    if attempts > self.max_retries {
                        warn!(
                            "Operation '{}' failed after {} attempts (max retries reached). Last error: {:?}",
                            operation_name, attempts, error
                        );
                        // Return the last error wrapped in RetryExhausted
                        return Err(Error::RetryExhausted(format!("{} (last error: {:?})", operation_name, error)));
                    }

                    warn!(
                        "Operation '{}' failed on attempt {} with error: {:?}. Retrying in {:?}...",
                        operation_name, attempts, error, current_delay
                    );

                    sleep(current_delay).await;

                    // Calculate next delay with exponential backoff
                    current_delay = Duration::from_secs_f64(
                        (current_delay.as_secs_f64() * self.multiplier)
                            .min(self.max_delay.as_secs_f64())
                    );
                    // Add jitter to delay to prevent thundering herd
                    let jitter_factor = 0.1 * fastrand::f64(); // Max 10% jitter
                    let jitter = Duration::from_secs_f64(current_delay.as_secs_f64() * jitter_factor);
                    current_delay = current_delay.saturating_add(jitter);
                    if current_delay > self.max_delay {
                        current_delay = self.max_delay;
                    }
                }
            }
        }
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Arc;
    use crate::error::Error; // Import your SDK Error type

    // Dummy error type for testing
    #[derive(Debug, Clone)]
    struct TestError(String);

    impl From<TestError> for Error {
        fn from(e: TestError) -> Self {
            Error::Generic(e.0)
        }
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
    impl std::error::Error for TestError {}


    #[tokio::test]
    async fn test_retry_succeeds_on_first_attempt() {
        let config = RetryConfig::default();
        let strategy = RetryStrategy::from_config(&config);
        let attempts = Arc::new(AtomicUsize::new(0));

        let operation = || {
            let attempts_clone = attempts.clone();
            async move {
                attempts_clone.fetch_add(1, AtomicOrdering::SeqCst);
                Ok::<_, TestError>("success")
            }
        };

        let result = strategy.retry("test_op_success_first", operation).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(AtomicOrdering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let mut config = RetryConfig::default();
        config.max_retries = 3;
        config.initial_delay = Duration::from_millis(10); // Short delay for test
        let strategy = RetryStrategy::from_config(&config);
        let attempts = Arc::new(AtomicUsize::new(0));

        let operation = || {
            let attempts_clone = attempts.clone();
            async move {
                let current_attempt = attempts_clone.fetch_add(1, AtomicOrdering::SeqCst) + 1;
                if current_attempt < 3 {
                    Err(TestError(format!("fail attempt {}", current_attempt)))
                } else {
                    Ok("success finally")
                }
            }
        };

        let result = strategy.retry("test_op_success_later", operation).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success finally");
        assert_eq!(attempts.load(AtomicOrdering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausts_attempts() {
        let mut config = RetryConfig::default();
        config.max_retries = 2; // e.g., 1 initial try + 2 retries = 3 total attempts
        config.initial_delay = Duration::from_millis(10);
        let strategy = RetryStrategy::from_config(&config);
        let attempts = Arc::new(AtomicUsize::new(0));

        let operation = || {
            let attempts_clone = attempts.clone();
            async move {
                attempts_clone.fetch_add(1, AtomicOrdering::SeqCst);
                Err::<String, _>(TestError("persistent failure".to_string()))
            }
        };

        let result = strategy.retry("test_op_exhaust", operation).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::RetryExhausted(msg) => {
                assert!(msg.contains("persistent failure"));
            }
            _ => panic!("Expected RetryExhausted error"),
        }
        assert_eq!(attempts.load(AtomicOrdering::SeqCst), config.max_retries + 1);
    }

    #[tokio::test]
    async fn test_retry_delay_increases() {
        // This test is more complex to verify timing accurately without being flaky.
        // We can check that the total time taken is roughly what we expect for increasing delays.
        let mut config = RetryConfig::default();
        config.max_retries = 2; // 3 total attempts
        config.initial_delay = Duration::from_millis(20);
        config.max_delay = Duration::from_millis(100); // Ensure max_delay is hit or relevant
        config.multiplier = 2.0;
        let strategy = RetryStrategy::from_config(&config);

        let start_time = Instant::now();
        let _ = strategy.retry("test_delays", || async { Err::<(), TestError>(TestError("fail".into())) }).await;
        let elapsed = start_time.elapsed();

        // Expected delays (approx, without jitter):
        // Attempt 1: 0ms (immediate)
        // Attempt 2: waits 20ms
        // Attempt 3: waits 20ms * 2 = 40ms
        // Total delay time approx = 20 + 40 = 60ms.
        // Jitter will add some variance.
        let min_expected_duration = Duration::from_millis(50); // Sum of initial delays minus some slack
        let max_expected_duration = Duration::from_millis(100); // Sum of delays + jitter + execution time

        assert!(elapsed >= min_expected_duration, "Elapsed time {:?} was less than min expected {:?}", elapsed, min_expected_duration);
        assert!(elapsed <= max_expected_duration, "Elapsed time {:?} was more than max expected {:?}", elapsed, max_expected_duration);
    }
}
