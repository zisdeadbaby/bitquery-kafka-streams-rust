use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, warn, info};

/// Result type for circuit breaker operations
pub type CircuitBreakerResult<T> = Result<T, CircuitBreakerError>;

/// Errors that can occur with circuit breaker operations
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerError {
    /// Circuit is open and not allowing requests
    CircuitOpen,
    /// Operation failed and was recorded
    OperationFailed(String),
    /// Configuration error
    ConfigError(String),
}

impl std::fmt::Display for CircuitBreakerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CircuitOpen => write!(f, "Circuit breaker is open"),
            Self::OperationFailed(msg) => write!(f, "Operation failed: {}", msg),
            Self::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for CircuitBreakerError {}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    /// Circuit is closed, allowing all requests
    Closed,
    /// Circuit is open, blocking all requests
    Open,
    /// Circuit is half-open, allowing limited test requests
    HalfOpen,
}

/// Configuration for the circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures required to open the circuit
    pub failure_threshold: u32,
    /// Duration to wait before transitioning from Open to HalfOpen
    pub reset_timeout: Duration,
    /// Number of successful requests required to close from HalfOpen
    pub success_threshold: u32,
    /// Maximum number of requests allowed in HalfOpen state
    pub half_open_max_requests: u32,
    /// Sliding window size for failure rate calculation
    pub sliding_window_size: usize,
    /// Whether to enable detailed logging
    pub enable_logging: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            success_threshold: 3,
            half_open_max_requests: 5,
            sliding_window_size: 100,
            enable_logging: true,
        }
    }
}

impl CircuitBreakerConfig {
    /// Creates a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the failure threshold
    pub fn with_failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold;
        self
    }

    /// Sets the reset timeout
    pub fn with_reset_timeout(mut self, timeout: Duration) -> Self {
        self.reset_timeout = timeout;
        self
    }

    /// Sets the success threshold for closing from half-open
    pub fn with_success_threshold(mut self, threshold: u32) -> Self {
        self.success_threshold = threshold;
        self
    }

    /// Sets the maximum requests allowed in half-open state
    pub fn with_half_open_max_requests(mut self, max_requests: u32) -> Self {
        self.half_open_max_requests = max_requests;
        self
    }

    /// Sets the sliding window size
    pub fn with_sliding_window_size(mut self, size: usize) -> Self {
        self.sliding_window_size = size;
        self
    }

    /// Enables or disables logging
    pub fn with_logging(mut self, enable: bool) -> Self {
        self.enable_logging = enable;
        self
    }

    /// Validates the configuration
    pub fn validate(&self) -> Result<(), CircuitBreakerError> {
        if self.failure_threshold == 0 {
            return Err(CircuitBreakerError::ConfigError(
                "Failure threshold must be greater than 0".to_string(),
            ));
        }
        if self.success_threshold == 0 {
            return Err(CircuitBreakerError::ConfigError(
                "Success threshold must be greater than 0".to_string(),
            ));
        }
        if self.half_open_max_requests == 0 {
            return Err(CircuitBreakerError::ConfigError(
                "Half-open max requests must be greater than 0".to_string(),
            ));
        }
        if self.sliding_window_size == 0 {
            return Err(CircuitBreakerError::ConfigError(
                "Sliding window size must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// SDK-style circuit breaker with advanced features
pub struct CircuitBreakerSdk {
    config: CircuitBreakerConfig,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    half_open_requests: AtomicU32,
    state: Arc<TokioMutex<CircuitState>>,
    last_state_change: Arc<TokioMutex<Instant>>,
    metrics: Arc<TokioMutex<CircuitBreakerMetrics>>,
}

/// Metrics collected by the circuit breaker
#[derive(Debug, Clone, Default)]
pub struct CircuitBreakerMetrics {
    /// Total number of requests processed by the circuit breaker
    pub total_requests: u64,
    /// Number of requests that completed successfully
    pub successful_requests: u64,
    /// Number of requests that failed
    pub failed_requests: u64,
    /// Number of requests rejected due to open circuit
    pub rejected_requests: u64,
    /// Number of state transitions (closed->open, open->half-open, etc.)
    pub state_transitions: u64,
    /// Total time spent in open state
    pub time_in_open_state: Duration,
    /// Total time spent in half-open state
    pub time_in_half_open_state: Duration,
    /// Timestamp of the most recent failure
    pub last_failure_time: Option<Instant>,
    /// Timestamp of the most recent success
    pub last_success_time: Option<Instant>,
}

impl CircuitBreakerSdk {
    /// Creates a new circuit breaker with the given configuration
    pub fn new(config: CircuitBreakerConfig) -> CircuitBreakerResult<Self> {
        config.validate()?;
        
        let enable_logging = config.enable_logging;
        
        let breaker = Self {
            config,
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            half_open_requests: AtomicU32::new(0),
            state: Arc::new(TokioMutex::new(CircuitState::Closed)),
            last_state_change: Arc::new(TokioMutex::new(Instant::now())),
            metrics: Arc::new(TokioMutex::new(CircuitBreakerMetrics::default())),
        };

        if enable_logging {
            info!("Circuit breaker initialized with config: {:?}", breaker.config);
        }

        Ok(breaker)
    }

    /// Creates a new circuit breaker with default configuration
    pub fn with_defaults() -> CircuitBreakerResult<Self> {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Gets the current state of the circuit breaker
    pub async fn state(&self) -> CircuitState {
        *self.state.lock().await
    }

    /// Checks if a request should be allowed through
    pub async fn allow_request(&self) -> bool {
        let mut metrics = self.metrics.lock().await;
        metrics.total_requests += 1;
        drop(metrics);

        match self.state().await {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                if self.should_attempt_reset().await {
                    self.transition_to_half_open().await;
                    true
                } else {
                    self.record_rejected_request().await;
                    false
                }
            }
            CircuitState::HalfOpen => {
                let current_requests = self.half_open_requests.load(Ordering::SeqCst);
                if current_requests < self.config.half_open_max_requests {
                    self.half_open_requests.fetch_add(1, Ordering::SeqCst);
                    true
                } else {
                    self.record_rejected_request().await;
                    false
                }
            }
        }
    }

    /// Records a successful operation
    pub async fn record_success(&self) {
        let current_state = self.state().await;
        self.success_count.fetch_add(1, Ordering::SeqCst);

        let mut metrics = self.metrics.lock().await;
        metrics.successful_requests += 1;
        metrics.last_success_time = Some(Instant::now());
        drop(metrics);

        match current_state {
            CircuitState::Closed => {
                // Reset failure count on success in closed state
                self.failure_count.store(0, Ordering::SeqCst);
            }
            CircuitState::HalfOpen => {
                let successes = self.success_count.load(Ordering::SeqCst);
                if successes >= self.config.success_threshold {
                    self.transition_to_closed().await;
                }
            }
            CircuitState::Open => {
                // This shouldn't happen if allow_request is used properly
                if self.config.enable_logging {
                    warn!("Received success while circuit is open - this may indicate improper usage");
                }
            }
        }

        if self.config.enable_logging {
            debug!("Circuit breaker recorded success. State: {:?}", current_state);
        }
    }

    /// Records a failed operation
    pub async fn record_failure(&self) {
        let current_state = self.state().await;
        self.failure_count.fetch_add(1, Ordering::SeqCst);

        let mut metrics = self.metrics.lock().await;
        metrics.failed_requests += 1;
        metrics.last_failure_time = Some(Instant::now());
        drop(metrics);

        match current_state {
            CircuitState::Closed => {
                let failures = self.failure_count.load(Ordering::SeqCst);
                if failures >= self.config.failure_threshold {
                    self.transition_to_open().await;
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open immediately opens the circuit
                self.transition_to_open().await;
            }
            CircuitState::Open => {
                // Already open, just record the failure
            }
        }

        if self.config.enable_logging {
            debug!("Circuit breaker recorded failure. State: {:?}", current_state);
        }
    }

    /// Executes a closure with circuit breaker protection
    pub async fn execute<F, T, E>(&self, operation: F) -> CircuitBreakerResult<T>
    where
        F: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        if !self.allow_request().await {
            return Err(CircuitBreakerError::CircuitOpen);
        }

        match operation.await {
            Ok(result) => {
                self.record_success().await;
                Ok(result)
            }
            Err(error) => {
                self.record_failure().await;
                Err(CircuitBreakerError::OperationFailed(error.to_string()))
            }
        }
    }

    /// Gets current metrics
    pub async fn metrics(&self) -> CircuitBreakerMetrics {
        self.metrics.lock().await.clone()
    }

    /// Resets the circuit breaker to closed state
    pub async fn reset(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        self.half_open_requests.store(0, Ordering::SeqCst);
        
        let mut state = self.state.lock().await;
        *state = CircuitState::Closed;
        drop(state);

        let mut last_change = self.last_state_change.lock().await;
        *last_change = Instant::now();
        drop(last_change);

        if self.config.enable_logging {
            info!("Circuit breaker manually reset to closed state");
        }
    }

    // Private helper methods

    async fn should_attempt_reset(&self) -> bool {
        let last_change = self.last_state_change.lock().await;
        last_change.elapsed() >= self.config.reset_timeout
    }

    async fn transition_to_open(&self) {
        let mut state = self.state.lock().await;
        if *state != CircuitState::Open {
            *state = CircuitState::Open;
            drop(state);

            let mut last_change = self.last_state_change.lock().await;
            *last_change = Instant::now();
            drop(last_change);

            let mut metrics = self.metrics.lock().await;
            metrics.state_transitions += 1;
            drop(metrics);

            if self.config.enable_logging {
                warn!("Circuit breaker opened after {} failures", self.failure_count.load(Ordering::SeqCst));
            }
        }
    }

    async fn transition_to_half_open(&self) {
        let mut state = self.state.lock().await;
        if *state == CircuitState::Open {
            *state = CircuitState::HalfOpen;
            drop(state);

            self.half_open_requests.store(0, Ordering::SeqCst);
            self.success_count.store(0, Ordering::SeqCst);

            let mut last_change = self.last_state_change.lock().await;
            *last_change = Instant::now();
            drop(last_change);

            let mut metrics = self.metrics.lock().await;
            metrics.state_transitions += 1;
            drop(metrics);

            if self.config.enable_logging {
                info!("Circuit breaker transitioned to half-open state");
            }
        }
    }

    async fn transition_to_closed(&self) {
        let mut state = self.state.lock().await;
        if *state != CircuitState::Closed {
            *state = CircuitState::Closed;
            drop(state);

            self.failure_count.store(0, Ordering::SeqCst);
            self.success_count.store(0, Ordering::SeqCst);
            self.half_open_requests.store(0, Ordering::SeqCst);

            let mut last_change = self.last_state_change.lock().await;
            *last_change = Instant::now();
            drop(last_change);

            let mut metrics = self.metrics.lock().await;
            metrics.state_transitions += 1;
            drop(metrics);

            if self.config.enable_logging {
                info!("Circuit breaker closed after successful recovery");
            }
        }
    }

    async fn record_rejected_request(&self) {
        let mut metrics = self.metrics.lock().await;
        metrics.rejected_requests += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker_basic_flow() {
        let config = CircuitBreakerConfig::new()
            .with_failure_threshold(2)
            .with_reset_timeout(Duration::from_millis(100))
            .with_logging(false);

        let cb = CircuitBreakerSdk::new(config).unwrap();

        // Initially closed
        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.allow_request().await);

        // Record failures
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Closed);
        
        cb.record_failure().await; // Should open
        assert_eq!(cb.state().await, CircuitState::Open);
        assert!(!cb.allow_request().await);

        // Wait for reset timeout
        sleep(Duration::from_millis(150)).await;
        
        // Should transition to half-open
        assert!(cb.allow_request().await);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_execute_function() {
        let config = CircuitBreakerConfig::new()
            .with_failure_threshold(1)
            .with_logging(false);

        let cb = CircuitBreakerSdk::new(config).unwrap();

        // Successful operation
        let result = cb.execute(async { Ok::<i32, &str>(42) }).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        // Failing operation
        let result = cb.execute(async { Err::<i32, &str>("test error") }).await;
        assert!(result.is_err());
        
        // Circuit should be open now
        assert_eq!(cb.state().await, CircuitState::Open);
        
        // Next request should be rejected
        let result = cb.execute(async { Ok::<i32, &str>(42) }).await;
        assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen)));
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let config = CircuitBreakerConfig::new()
            .with_failure_threshold(2)
            .with_logging(false);

        let cb = CircuitBreakerSdk::new(config).unwrap();

        // Make some requests
        cb.allow_request().await;
        cb.record_success().await;
        
        cb.allow_request().await;
        cb.record_failure().await;
        
        cb.allow_request().await;
        cb.record_failure().await; // Should open

        let metrics = cb.metrics().await;
        assert_eq!(metrics.total_requests, 3);
        assert_eq!(metrics.successful_requests, 1);
        assert_eq!(metrics.failed_requests, 2);
        assert!(metrics.state_transitions > 0);
    }

    #[tokio::test]
    async fn test_config_validation() {
        let invalid_config = CircuitBreakerConfig::new()
            .with_failure_threshold(0);

        let result = CircuitBreakerSdk::new(invalid_config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_half_open_recovery() {
        let config = CircuitBreakerConfig::new()
            .with_failure_threshold(1)
            .with_success_threshold(2)
            .with_reset_timeout(Duration::from_millis(50))
            .with_logging(false);

        let cb = CircuitBreakerSdk::new(config).unwrap();

        // Open the circuit
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);

        // Wait for reset timeout
        sleep(Duration::from_millis(100)).await;
        
        // Allow request should transition to half-open
        assert!(cb.allow_request().await);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Record successes to close
        cb.record_success().await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
        
        cb.record_success().await; // Should close
        assert_eq!(cb.state().await, CircuitState::Closed);
    }
}
