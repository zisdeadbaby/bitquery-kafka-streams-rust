use thiserror::Error;

/// SDK result type, wrapping the SDK's [`Error`] enum.
pub type Result<T> = std::result::Result<T, Error>;

/// SDK error types.
#[derive(Error, Debug)]
pub enum Error {
    /// Errors originating from the underlying `rdkafka` library.
    #[error("Kafka client error: {0}")]
    Kafka(#[from] rdkafka::error::KafkaError),

    /// Errors related to SDK configuration.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Errors during connection establishment or communication (not covered by KafkaError).
    #[error("Connection error: {0}")]
    Connection(String),

    /// Errors during Protobuf message decoding.
    #[error("Protobuf decoding error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    /// Errors related to data compression or decompression.
    #[error("Compression error: {0}")]
    Compression(String),

    /// Errors during Base58 encoding or decoding.
    #[error("Base58 encoding/decoding error: {0}")]
    Base58(#[from] bs58::decode::Error),

    /// Errors encountered during event processing logic, batching, or filtering.
    #[error("Event processing error: {0}")]
    Processing(String),

    /// Errors related to resource management, e.g., limits exceeded.
    #[error("Resource management error: {0}")]
    ResourceError(String),

    /// Indicates that the circuit breaker is currently open, preventing operations.
    /// Note: The new SDK version uses ResourceManager's backpressure instead of a dedicated CircuitBreaker util.
    /// This might be deprecated or re-purposed if backpressure covers all needs.
    /// For now, keeping it as it was in my prior implementation. The new config doesn't have circuit breaker settings.
    #[error("Circuit breaker is open, operation temporarily suspended")]
    CircuitBreakerOpen,

    /// Indicates that an operation failed after exhausting all retry attempts.
    #[error("Operation failed after {attempts} retry attempts: {source}")]
    RetryExhausted {
        attempts: usize,
        #[source]
        source: Box<Error>
    },

    /// Errors related to I/O operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Errors from async channel operations (e.g., send/receive errors if channel is closed).
    #[error("Async channel error: {0}")]
    ChannelError(String), // For send/recv errors on async_channel or crossbeam_channel

    /// Placeholder for other types of errors not fitting specific categories.
    #[error("An unexpected error occurred: {0}")]
    Other(String),
}

impl Error {
    /// Helper to create a RetryExhausted error, boxing the source error.
    pub fn retry_exhausted(attempts: usize, last_error: Error) -> Self {
        Error::RetryExhausted { attempts, source: Box::new(last_error) }
    }
}

// Specific From implementations for channel errors if they have distinct types
impl<T> From<async_channel::SendError<T>> for Error {
    fn from(err: async_channel::SendError<T>) -> Self {
        Error::ChannelError(format!("Async channel send error: {}", err))
    }
}

impl From<async_channel::RecvError> for Error {
    fn from(err: async_channel::RecvError) -> Self {
        Error::ChannelError(format!("Async channel receive error: {}", err))
    }
}

// If using crossbeam_channel too (it's in new Cargo.toml):
impl<T> From<crossbeam_channel::SendError<T>> for Error {
    fn from(err: crossbeam_channel::SendError<T>) -> Self {
        Error::ChannelError(format!("Crossbeam channel send error: {}", err))
    }
}
impl From<crossbeam_channel::RecvError> for Error {
    fn from(err: crossbeam_channel::RecvError) -> Self {
        Error::ChannelError(format!("Crossbeam channel receive error: {}", err))
    }
}
